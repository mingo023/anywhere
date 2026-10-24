package wsserver

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/config"
	"pocketd/internal/devices"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

var phoneCaps = []string{proto.CapTerminal, proto.CapWorkspace, proto.CapGit, proto.CapGitHub, proto.CapFiles, proto.CapPush}

// servingPhoneCaps lets a test negotiate the caps whose handlers haven't landed.
func servingPhoneCaps(t *testing.T) {
	served := proto.ServerCaps
	proto.ServerCaps = append(append([]string(nil), served...), phoneCaps...)
	t.Cleanup(func() { proto.ServerCaps = served })
}

// phoneGoldens are the client golden files of the phone verbs, by name.
func phoneGoldens(t *testing.T) map[string]string {
	files, _ := filepath.Glob(filepath.Join("..", "proto", "testdata", "golden", "client", "*.json"))
	verbs := proto.PhoneVerbs()
	out := map[string]string{}
	for _, f := range files {
		raw, _ := os.ReadFile(f)
		var m struct{ Type string }
		json.Unmarshal(raw, &m)
		for _, v := range verbs {
			if m.Type == v {
				out[filepath.Base(f)] = strings.TrimSpace(string(raw))
			}
		}
	}
	if len(out) < len(verbs) {
		t.Fatalf("%d goldens for %d verbs", len(out), len(verbs))
	}
	return out
}

func helloWith(p *phone, caps ...string) {
	raw, _ := json.Marshal(append([]string{}, caps...))
	p.send(`{"type":"hello","id":"h","clientId":"c","protocolVersion":3,"caps":` + string(raw) + `}`)
	for m := p.recv(); m["type"] != "agent.list"; m = p.recv() {
	}
}

func TestAPhoneVerbWithoutItsCapIsMalformed(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1))
	helloWith(p)
	for name, raw := range phoneGoldens(t) {
		p.send(raw)
		if m := p.recv(); m["type"] != "error" || m["message"] != "Malformed message" || m["code"] != nil {
			t.Errorf("%s: %v", name, m)
		}
	}
}

// servedVerbs are the phone verbs whose handlers landed.
var servedVerbs = []string{"worktree.create", "worktree.delete", "branch.list", "account.list", "agent.account"}

func TestEveryPhoneVerbUnderItsCapIsNotImplementedYet(t *testing.T) {
	servingPhoneCaps(t)
	_, _, p := local(t, peer.OwnerOf(1))
	helloWith(p, phoneCaps...)
	for name, raw := range phoneGoldens(t) {
		var req struct{ ID, Type string }
		json.Unmarshal([]byte(raw), &req)
		if strings.HasPrefix(req.Type, "terminal.") || slices.Contains(servedVerbs, req.Type) {
			continue
		}
		p.send(raw)
		if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeNotImplemented || m["id"] != req.ID {
			t.Errorf("%s: %v", name, m)
		}
	}
}

func TestAPhoneVerbIsCheckedForScopeBeforeItsCap(t *testing.T) {
	_, _, p := local(t, peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes})
	helloWith(p)
	p.send(`{"type":"git.stage","id":"s","tree":"/p","paths":["a"],"staged":true}`)
	if m := p.recv(); m["code"] != proto.CodeScopeDenied {
		t.Fatalf("%v", m)
	}
}

func withSettings(t *testing.T, set func(*config.Settings)) func(*Server) {
	s := config.NewSettings(t.TempDir())
	set(s)
	return func(srv *Server) { srv.Settings = s }
}

func TestWithTerminalsOffAPhoneMayWatchButNotTypeOrOpen(t *testing.T) {
	servingPhoneCaps(t)
	off := withSettings(t, func(s *config.Settings) { s.Set("phone.terminals", "off") })
	_, _, p := local(t, peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes}), off)
	helloWith(p, proto.CapTerminal)
	for _, raw := range []string{
		`{"type":"terminal.input","id":"1","terminalId":"t1","data":"bHMNCg=="}`,
		`{"type":"terminal.resize","id":"2","terminalId":"t1","cols":40,"rows":20}`,
		`{"type":"terminal.create","id":"3","requestId":"r"}`,
		`{"type":"terminal.close","id":"4","terminalId":"t1"}`,
	} {
		p.send(raw)
		if m := p.recv(); m["code"] != proto.CodeTerminalsOff {
			t.Errorf("%s: %v", raw, m)
		}
	}
	p.send(`{"type":"terminal.list","id":"5"}`)
	if m := p.recv(); m["type"] != "terminal.list" || m["id"] != "5" {
		t.Errorf("list: %v", m)
	}
}

func TestTheOwnerTurnsPhoneTerminalsOffWithConfigSet(t *testing.T) {
	var s *Server
	_, _, p := local(t, peer.OwnerOf(1), withSettings(t, func(*config.Settings) {}), func(srv *Server) { s = srv })
	p.hello()
	p.send(`{"type":"config.set","id":"c","key":"phone.terminals","value":"off"}`)
	if m := p.recv(); m["type"] != "ack" || s.Settings.PhoneTerminals() {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"config.set","id":"c","key":"push.relay","value":"http://relay.example.com"}`)
	if m := p.recv(); m["code"] != "invalid_config" || s.Settings.PushRelay() != config.DefaultPushRelay {
		t.Fatalf("%v", m)
	}
}

func TestPagingBackFromASeqSaysExactlyWhetherOlderRemain(t *testing.T) {
	reg, _, p := setup(t)
	a, _ := reg.Get("a1")
	for range 5 {
		a.Record(timeline.Event{Kind: "user", Text: "x"})
	}
	p.hello()
	for _, c := range []struct {
		req   string
		seqs  []float64
		older bool
	}{
		{`{"type":"agent.timeline","id":"t","agentId":"a1","beforeSeq":5,"limit":2}`, []float64{3, 4}, true},
		{`{"type":"agent.timeline","id":"t","agentId":"a1","beforeSeq":3,"limit":2}`, []float64{1, 2}, false},
		{`{"type":"agent.timeline","id":"t","agentId":"a1","beforeSeq":1}`, nil, false},
	} {
		p.send(c.req)
		m := p.recv()
		for m["type"] != "agent.timeline" {
			m = p.recv()
		}
		var seqs []float64
		for _, it := range m["items"].([]any) {
			seqs = append(seqs, it.(map[string]any)["seq"].(float64))
		}
		if !reflect.DeepEqual(seqs, c.seqs) || m["hasOlder"] != c.older || m["maxSeq"] != 5.0 {
			t.Fatalf("%s: %v", c.req, m)
		}
	}
}

func TestATerminalFrameArrivesAsOneBinaryMessage(t *testing.T) {
	sent := make(chan error, 1)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ws, err := websocket.Accept(w, r, nil)
		if err != nil {
			sent <- err
			return
		}
		defer ws.CloseNow()
		sent <- writeFrame(r.Context(), ws, proto.FrameSnapshot, "t7", []byte("\x1b[2J$ "))
		ws.Read(r.Context())
	}))
	defer srv.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	ws, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http"), nil)
	if err != nil {
		t.Fatal(err)
	}
	defer ws.CloseNow()
	kind, raw, err := ws.Read(ctx)
	if err != nil || <-sent != nil {
		t.Fatal(err)
	}
	if kind != websocket.MessageBinary || string(raw) != "\x01\x02t7\x1b[2J$ " {
		t.Fatalf("%v %q", kind, raw)
	}
	if err := writeFrame(ctx, ws, proto.FrameOutput, "", nil); err == nil {
		t.Fatal("a frame without a terminal id was written")
	}
}
