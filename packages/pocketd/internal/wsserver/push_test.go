package wsserver

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/push"
)

// relaySends is a relay that takes every send and keeps their payloads.
type relaySends struct {
	mu   sync.Mutex
	sent []push.Send
}

func (r *relaySends) list() []push.Send {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]push.Send(nil), r.sent...)
}

// withPush serves push.v1 through a real Notifier sending to a fake relay.
func withPush(t *testing.T, sends *relaySends, s **Server) func(*Server) {
	relay := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var one push.Send
		json.Unmarshal(body, &one)
		sends.mu.Lock()
		sends.sent = append(sends.sent, one)
		sends.mu.Unlock()
		w.Write([]byte(`{"apnsId":"x"}`))
	}))
	t.Cleanup(relay.Close)
	return func(srv *Server) {
		store, _ := push.Open(push.Path(t.TempDir()))
		timing := push.DefaultTiming
		timing.NeedsYou, timing.Done = 20*time.Millisecond, 20*time.Millisecond
		n := &push.Notifier{Store: store, Relay: &push.Client{URL: func() string { return relay.URL }}, Agents: srv.Agents, Broker: srv.Broker,
			Mac: srv.Hostname, MacName: "Studio", Foreground: srv.Foreground, Timing: timing}
		srv.Agents.OnStatus = func(id, _, from, to string) { n.Kick(id, from, to) }
		ctx, cancel := context.WithCancel(context.Background())
		done := make(chan struct{})
		go func() {
			n.Run(ctx)
			close(done)
		}()
		t.Cleanup(func() {
			cancel()
			<-done
		})
		srv.Push = n
		*s = srv
	}
}

func golden(t *testing.T, name string) string {
	raw, err := os.ReadFile(filepath.Join("..", "proto", "testdata", "golden", "client", name))
	if err != nil {
		t.Fatal(err)
	}
	return strings.TrimSpace(string(raw))
}

func pushHello(p *phone) {
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["push.v1"]}`)
	for m := p.recv(); m["type"] != "agent.list"; m = p.recv() {
	}
}

func (p *phone) ack(raw string) {
	p.t.Helper()
	p.send(raw)
	var req struct{ ID string }
	json.Unmarshal([]byte(raw), &req)
	if m := p.recv(); m["type"] != "ack" || m["id"] != req.ID {
		p.t.Fatalf("%s: %v", raw, m)
	}
}

func TestAPhoneRegistersSetsItsActivityAndPrefsThenUnregisters(t *testing.T) {
	var s *Server
	sends := &relaySends{}
	_, _, p := setup(t, withPush(t, sends, &s))
	pushHello(p)
	p.ack(golden(t, "push_register.json"))
	rec, ok := s.Push.(*push.Notifier).Store.Get(devices.LegacyID)
	if !ok || len(rec.Key) != 32 || rec.Env != "production" || rec.Alert.Handle != "aGFuZGxl" || rec.Start.Handle != "c3RhcnQ" || !rec.Prefs.Details || !rec.Frequent {
		t.Fatalf("%+v", rec)
	}
	p.ack(golden(t, "push_activity.json"))
	p.ack(golden(t, "push_prefs.json"))
	rec, _ = s.Push.(*push.Notifier).Store.Get(devices.LegacyID)
	if rec.Activity == nil || rec.Activity.ID != "A1B2" || rec.Activity.Handle.Handle != "YWN0aXZpdHk" || rec.Prefs.Done {
		t.Fatalf("%+v", rec)
	}
	p.ack(golden(t, "push_activity_ended.json"))
	p.ack(golden(t, "push_test.json"))
	if got := sends.list(); len(got) == 0 || got[len(got)-1].Handle != "aGFuZGxl" {
		t.Fatalf("%+v", got)
	}
	p.ack(golden(t, "push_unregister.json"))
	if _, ok := s.Push.(*push.Notifier).Store.Get(devices.LegacyID); ok {
		t.Fatal("still registered")
	}
	p.send(golden(t, "push_test.json"))
	if m := p.recv(); m["type"] != "error" || m["message"] != push.ErrNotRegistered.Error() {
		t.Fatalf("%v", m)
	}
}

func TestOnlyAPairedPhoneRegistersForPushes(t *testing.T) {
	var s *Server
	_, _, p := local(t, peer.OwnerOf(1), withPush(t, &relaySends{}, &s))
	helloWith(p, proto.CapPush)
	p.send(golden(t, "push_register.json"))
	if m := p.recv(); m["type"] != "error" || m["message"] != errNotAPhone.Error() {
		t.Fatalf("%v", m)
	}
}

func TestAPhoneIsInFrontUntilItViewsNothing(t *testing.T) {
	var s *Server
	_, _, p := setup(t, withPush(t, &relaySends{}, &s))
	pushHello(p)
	if !s.Foreground(devices.LegacyID) {
		t.Fatal("a phone that just said hello is in the background")
	}
	p.ack(`{"type":"agent.view","id":"v","agentIds":[]}`)
	if s.Foreground(devices.LegacyID) {
		t.Fatal("still in front after viewing nothing")
	}
	p.ack(`{"type":"agent.view","id":"v","agentIds":["a1"]}`)
	if !s.Foreground(devices.LegacyID) || s.Foreground("other") {
		t.Fatal("not in front while viewing a session")
	}
}

func TestARegisteredPhoneInTheBackgroundGetsOneAlertWhenAnUnseenSessionNeedsIt(t *testing.T) {
	var s *Server
	sends := &relaySends{}
	reg, _, p := setup(t, withPush(t, sends, &s))
	pushHello(p)
	p.ack(golden(t, "push_register.json"))
	p.ack(`{"type":"agent.view","id":"v","agentIds":[]}`)
	a, _ := reg.Get("a1")
	a.Working()
	a.NeedsYou()
	deadline := time.Now().Add(3 * time.Second)
	for len(alerts(sends)) == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	time.Sleep(200 * time.Millisecond)
	got := alerts(sends)
	if len(got) != 1 || got[0].Priority != 10 {
		t.Fatalf("%+v", got)
	}
	var payload struct {
		Aps map[string]any `json:"aps"`
	}
	json.Unmarshal(got[0].Payload, &payload)
	if payload.Aps["category"] != "QUESTION" || payload.Aps["interruption-level"] != "time-sensitive" {
		t.Fatalf("%v", payload.Aps)
	}
}

func alerts(r *relaySends) []push.Send {
	var out []push.Send
	for _, s := range r.list() {
		if s.Handle == "aGFuZGxl" {
			out = append(out, s)
		}
	}
	return out
}
