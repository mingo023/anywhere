package push

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// fakeRelay is an HTTP relay that checks each send's signature as the real one does and records it.
type fakeRelay struct {
	t      *testing.T
	mu     sync.Mutex
	sends  []relayed
	answer map[string]func() (int, string)
	srv    *httptest.Server
}

type relayed struct {
	Send
	at time.Time
}

func secretOf(handle string) string {
	return base64.RawURLEncoding.EncodeToString([]byte("secret-" + handle))
}

func newRelay(t *testing.T) *fakeRelay {
	f := &fakeRelay{t: t, answer: map[string]func() (int, string){}}
	f.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var s Send
		json.Unmarshal(body, &s)
		want, _ := Sign(secretOf(s.Handle), r.Header.Get("X-Anywhere-Timestamp"), body)
		if r.URL.Path != "/v1/send" || r.Header.Get("X-Anywhere-Signature") != want {
			t.Errorf("bad signature or path %s", r.URL.Path)
			w.WriteHeader(401)
			return
		}
		f.mu.Lock()
		f.sends = append(f.sends, relayed{s, time.Now()})
		answer := f.answer[s.Handle]
		f.mu.Unlock()
		status, reply := 200, `{"apnsId":"x"}`
		if answer != nil {
			status, reply = answer()
		}
		w.WriteHeader(status)
		w.Write([]byte(reply))
	}))
	t.Cleanup(f.srv.Close)
	return f
}

func (f *fakeRelay) to(prefix string) []relayed {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out []relayed
	for _, s := range f.sends {
		if strings.HasPrefix(s.Handle, prefix) {
			out = append(out, s)
		}
	}
	return out
}

// await waits for n sends to handles with prefix, then for quiet more to show no more come.
func (f *fakeRelay) await(prefix string, n int, quiet time.Duration) []relayed {
	f.t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for len(f.to(prefix)) < n && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	time.Sleep(quiet)
	got := f.to(prefix)
	if len(got) != n {
		f.t.Fatalf("%d sends to %s, want %d", len(got), prefix, n)
	}
	return got
}

type driver struct{}

func (driver) Prompt(string) error                { return nil }
func (driver) Interrupt() error                   { return nil }
func (driver) Compact() error                     { return nil }
func (driver) Settings(string, string) error      { return nil }
func (driver) Access(string) error                { return nil }
func (driver) Commands() ([]proto.Command, error) { return nil, nil }
func (driver) Close()                             {}

type rig struct {
	t      *testing.T
	agents *agent.Registry
	broker *broker.Broker
	n      *Notifier
	relay  *fakeRelay
	key    []byte
	mu     sync.Mutex
	front  map[string]bool
}

var fast = Timing{NeedsYou: 30 * time.Millisecond, Done: 30 * time.Millisecond, Coalesce: 250 * time.Millisecond, Slow: 250 * time.Millisecond,
	Heartbeat: time.Hour, EndAfter: time.Hour, Stale: 5 * time.Minute, Dismiss: 15 * time.Minute, Restart: time.Hour, Retry: 5 * time.Millisecond}

func newRig(t *testing.T, timing Timing, restore ...agent.Restored) *rig {
	r := &rig{t: t, relay: newRelay(t), key: bytes.Repeat([]byte{5}, 32), front: map[string]bool{}}
	h := hub.New()
	r.agents = agent.NewRegistry(h)
	r.broker = broker.New(h)
	store, err := Open(Path(t.TempDir()))
	if err != nil {
		t.Fatal(err)
	}
	r.n = &Notifier{Store: store, Relay: &Client{URL: func() string { return r.relay.srv.URL }}, Agents: r.agents, Broker: r.broker,
		Mac: "studio", MacName: "Studio", Timing: timing, Foreground: func(d string) bool {
			r.mu.Lock()
			defer r.mu.Unlock()
			return r.front[d]
		}}
	r.agents.OnStatus = func(id, _, from, to string) { r.n.Kick(id, from, to) }
	r.broker.OnAsk = r.n.Ask
	for _, x := range restore {
		r.agents.Restore(x, driver{})
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go r.n.Run(ctx)
	return r
}

func h(name string) *proto.PushHandle {
	return &proto.PushHandle{Handle: name, Secret: secretOf(name), ExpiresAt: 1800000000}
}

var everything = proto.PushPrefs{NeedsYou: true, Done: true, Failed: true, Automations: true}

func (r *rig) register(device string, start bool) {
	reg := Registration{Key: r.key, Env: "production", Alert: h("alert-" + device), Prefs: everything, Frequent: true}
	if start {
		reg.Start = h("start-" + device)
	}
	if err := r.n.Register(device, reg); err != nil {
		r.t.Fatal(err)
	}
}

func (r *rig) add(id string) *agent.Agent {
	a := r.agents.Add(id, "/w", "claude", driver{})
	a.SetTitle("Fix the flaky test")
	return a
}

// ask opens a shell ask on agent until the test ends.
func (r *rig) ask(agentID string) {
	ctx, cancel := context.WithCancel(context.Background())
	r.t.Cleanup(cancel)
	go r.broker.Ask(ctx, proto.PermissionRequest{AgentID: agentID, ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "npm test"}}, "k")
}

func (r *rig) notice(s relayed) (Notice, map[string]any) {
	r.t.Helper()
	var p struct {
		Aps map[string]any `json:"aps"`
		E   string         `json:"e"`
	}
	json.Unmarshal(s.Payload, &p)
	plain, err := Unseal(r.key, "studio", p.E)
	var n Notice
	if err != nil || json.Unmarshal(plain, &n) != nil {
		r.t.Fatalf("%v %s", err, s.Payload)
	}
	return n, p.Aps
}

const quiet = 150 * time.Millisecond

func TestAPhoneGetsExactlyOneAlertWhenAnUnseenSessionNeedsIt(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	r.ask("a1")
	sent := r.relay.await("alert-", 1, 2*quiet)
	n, aps := r.notice(sent[0])
	if n.Kind != NeedsYou || n.Title != "Fix the flaky test" || n.Line != "Wants to run npm test" || n.Request == "" || n.Badge != 1 || n.Agent != "a1" || n.Mac != "studio" {
		t.Fatalf("%+v", n)
	}
	if aps["category"] != "PERM" || aps["interruption-level"] != "time-sensitive" || sent[0].Priority != 10 || sent[0].CollapseID != Opaque(r.key, "a1") {
		t.Fatalf("%v %+v", aps, sent[0].Send)
	}
	if left := time.Until(time.Unix(sent[0].Expiration, 0)); left < 59*time.Minute || left > time.Hour+time.Minute {
		t.Fatalf("expires in %s", left)
	}
	if got := r.n.Store.Sends(); len(got) != 1 || got[0].Outcome != "sent" || got[0].Kind != NeedsYou {
		t.Fatalf("%+v", got)
	}
}

func TestASessionTheUserIsLookingAtGetsNoAlert(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	a := r.add("a1")
	r.agents.SetView("desktop", []string{"a1"})
	a.Working()
	a.NeedsYou()
	a.TurnEnded(false)
	r.relay.await("alert-", 0, 3*quiet)
}

func TestAnAskAnsweredWithinTheDebounceGetsNoAlert(t *testing.T) {
	r := newRig(t, Timing{NeedsYou: 200 * time.Millisecond, Done: time.Hour, Coalesce: time.Hour, Slow: time.Hour, Heartbeat: time.Hour, EndAfter: time.Hour, Restart: time.Hour, Retry: time.Millisecond})
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	a.Working()
	r.relay.await("alert-", 0, 400*time.Millisecond)
}

func TestAPhoneWithTheAppInFrontGetsNoAlert(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	r.register("d2", false)
	r.front["d1"] = true
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	if got := r.relay.await("alert-", 1, 2*quiet); got[0].Handle != "alert-d2" {
		t.Fatalf("%s", got[0].Handle)
	}
}

func TestATurnThatEndsUnseenAlertsWithTheReplysFirstParagraph(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	a.Record(timeline.Event{Kind: "assistant_text", Text: "All **42** tests pass.\n\nI also tidied the fixture."})
	a.TurnEnded(false)
	b := r.add("b1")
	b.Working()
	b.TurnEnded(true)
	sent := r.relay.await("alert-", 2, 2*quiet)
	byAgent := map[string]Notice{}
	cats := map[string]any{}
	for _, s := range sent {
		n, aps := r.notice(s)
		byAgent[n.Agent], cats[n.Agent] = n, aps["category"]
	}
	if n := byAgent["a1"]; n.Kind != Done || n.Line != "All **42** tests pass." || cats["a1"] != "DONE" {
		t.Fatalf("%+v %v", n, cats["a1"])
	}
	if n := byAgent["b1"]; n.Kind != Failed || n.Line != "Failed" || cats["b1"] != "FAILED" || n.Badge != 2 {
		t.Fatalf("%+v %v", n, cats["b1"])
	}
}

func TestOneTransitionIsPushedOnceEvenWhenToldTwice(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	r.relay.await("alert-", 1, quiet)
	r.n.Kick("a1", "working", "needsYou")
	r.relay.await("alert-", 1, 2*quiet)
	r.ask("a1")
	r.relay.await("alert-", 2, quiet)
}

func TestARestartWithThreeDoneSessionsPushesNothing(t *testing.T) {
	var restored []agent.Restored
	for _, id := range []string{"a1", "a2", "a3"} {
		restored = append(restored, agent.Restored{ID: id, Cwd: "/w", Provider: "claude", Done: true})
	}
	r := newRig(t, fast, restored...)
	r.register("d1", true)
	r.relay.await("alert-", 0, 3*quiet)
	r.relay.await("start-", 0, 0)
	if got := r.agents.List(); len(got) != 3 || got[0].Status != "done" {
		t.Fatalf("%+v", got)
	}
}

func TestPrefsTurnEachKindOfAlertOff(t *testing.T) {
	r := newRig(t, fast)
	r.n.Register("d1", Registration{Key: r.key, Alert: h("alert-d1"), Prefs: proto.PushPrefs{NeedsYou: true}})
	a := r.add("a1")
	a.Working()
	a.TurnEnded(false)
	r.relay.await("alert-", 0, 3*quiet)
	r.n.Prefs("d1", proto.PushPrefs{Done: true})
	a.Working()
	a.TurnEnded(false)
	r.relay.await("alert-", 1, quiet)
}

func TestLiveActivityUpdatesCoalesceButNeedsYouGoesAtOnce(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	agents := []*agent.Agent{r.add("a1"), r.add("a2"), r.add("a3")}
	if err := r.n.Activity("d1", "act-1", h("act-d1"), false); err != nil {
		t.Fatal(err)
	}
	r.relay.await("act-", 1, 0)
	for _, a := range agents {
		a.Working()
	}
	sent := r.relay.await("act-", 2, 0)
	agents[0].NeedsYou()
	sent = r.relay.await("act-", 3, quiet)
	type state struct {
		Aps struct {
			Event        string `json:"event"`
			State        State  `json:"content-state"`
			Relevance    int    `json:"relevance-score"`
			Stale, Stamp int64
		} `json:"aps"`
	}
	var got []string
	for _, s := range sent {
		var p state
		json.Unmarshal(s.Payload, &p)
		got = append(got, jsonOf([]any{s.Priority, p.Aps.Event, p.Aps.State.Working, p.Aps.State.NeedsYou, p.Aps.Relevance}))
	}
	want := []string{`[5,"update",0,0,10]`, `[5,"update",3,0,50]`, `[10,"update",2,1,100]`}
	if jsonOf(got) != jsonOf(want) {
		t.Fatalf("%v\nwant %v", got, want)
	}
	if gap := sent[1].at.Sub(sent[0].at); gap < fast.Coalesce-20*time.Millisecond {
		t.Fatalf("second update after %s", gap)
	}
	var p map[string]map[string]any
	json.Unmarshal(sent[2].Payload, &p)
	if stale := int64(p["aps"]["stale-date"].(float64)) - time.Now().Unix(); stale < 290 || stale > 301 {
		t.Fatalf("stale in %d s", stale)
	}
}

func TestEnteringNeedsYouWithoutAnActivityStartsOneByPushOnce(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", true)
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	sent := r.relay.await("start-", 1, quiet)
	var p struct {
		Aps map[string]any `json:"aps"`
	}
	json.Unmarshal(sent[0].Payload, &p)
	if p.Aps["event"] != "start" || p.Aps["input-push-token"] != 1.0 || p.Aps["attributes-type"] != "MacActivity" ||
		jsonOf(p.Aps["attributes"]) != `{"mac":"studio","macName":"Studio"}` || p.Aps["alert"] == nil || p.Aps["sound"] != nil || sent[0].Priority != 10 {
		t.Fatalf("%v", p.Aps)
	}
	a.Working()
	a.NeedsYou()
	r.relay.await("start-", 1, 2*quiet)
}

func TestAnActivityEndsOnceNothingHasWorkedForAWhile(t *testing.T) {
	timing := fast
	timing.EndAfter = 300 * time.Millisecond
	r := newRig(t, timing)
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	r.n.Activity("d1", "act-1", h("act-d1"), false)
	r.relay.await("act-", 1, 0)
	a.TurnEnded(false)
	sent := r.relay.await("act-", 3, quiet)
	var p struct {
		Aps map[string]any `json:"aps"`
	}
	json.Unmarshal(sent[2].Payload, &p)
	var st State
	raw, _ := json.Marshal(p.Aps["content-state"])
	json.Unmarshal(raw, &st)
	if p.Aps["event"] != "end" || st.Busy() || st.Unseen != 1 || st.Top.Slot != "a1" || st.Top.Status != Done {
		t.Fatalf("%v", p.Aps)
	}
	if left := int64(p.Aps["dismissal-date"].(float64)) - time.Now().Unix(); left < 890 || left > 901 {
		t.Fatalf("dismissed in %d s", left)
	}
	if rec, _ := r.n.Store.Get("d1"); rec.Activity != nil {
		t.Fatalf("%+v", rec.Activity)
	}
}

func TestAnActivityWaitingOnTheUserKeepsBeatingSoItNeverGoesStale(t *testing.T) {
	timing := fast
	timing.Coalesce, timing.Heartbeat = 20*time.Millisecond, 200*time.Millisecond
	r := newRig(t, timing)
	r.register("d1", false)
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	r.n.Activity("d1", "act-1", h("act-d1"), false)
	sent := r.relay.await("act-", 3, 0)
	for i, s := range sent[1:] {
		var p struct {
			Aps struct {
				State State `json:"content-state"`
			} `json:"aps"`
		}
		json.Unmarshal(s.Payload, &p)
		if s.Priority != 5 || p.Aps.State.NeedsYou != 1 || p.Aps.State.Working != 0 {
			t.Fatalf("beat %d: p%d %s", i, s.Priority, s.Payload)
		}
	}
	if gap := sent[2].at.Sub(sent[1].at); gap < timing.Heartbeat-20*time.Millisecond {
		t.Fatalf("beat after %s", gap)
	}
}

func TestAGoneHandleIsDroppedAndABusyRelayIsTriedFourTimes(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	r.relay.answer["alert-d1"] = func() (int, string) { return 410, `{"reason":"Unregistered"}` }
	a := r.add("a1")
	a.Working()
	a.NeedsYou()
	r.relay.await("alert-", 1, quiet)
	if rec, _ := r.n.Store.Get("d1"); rec.Alert != nil {
		t.Fatal("kept a gone handle")
	}
	r.register("d2", false)
	r.relay.answer["alert-d2"] = func() (int, string) { return 503, `{"retryAfter":0}` }
	b := r.add("b1")
	b.Working()
	b.NeedsYou()
	r.relay.await("alert-d2", 4, 3*quiet)
	if rec, _ := r.n.Store.Get("d2"); rec.Alert == nil {
		t.Fatal("dropped a handle the relay only called busy")
	}
}

func TestAnActivityTheRelayCallsGoneIsForgotten(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	r.relay.answer["act-d1"] = func() (int, string) { return 410, `{"reason":"ExpiredToken"}` }
	r.n.Activity("d1", "act-1", h("act-d1"), false)
	r.relay.await("act-", 1, quiet)
	if rec, _ := r.n.Store.Get("d1"); rec.Activity != nil {
		t.Fatalf("%+v", rec.Activity)
	}
}

func TestAFinishedAutomationAlertsQuietlyUnlessItFailed(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	r.n.Automation(proto.Run{ID: "run1", Status: "succeeded", Summary: "Bumped 3 deps.\n\nAll green."}, "Nightly deps")
	r.n.Automation(proto.Run{ID: "run2", Status: "failed", Why: "The agent exited"}, "Nightly deps")
	r.n.Automation(proto.Run{ID: "run3", Status: "skipped"}, "Nightly deps")
	sent := r.relay.await("alert-", 2, quiet)
	for i, want := range []struct {
		line, level string
		priority    int
	}{{"Bumped 3 deps.", "passive", 5}, {"The agent exited", "active", 10}} {
		n, aps := r.notice(sent[i])
		if n.Kind != Automation || n.Title != "Nightly deps" || n.Line != want.line || aps["interruption-level"] != want.level || sent[i].Priority != want.priority || aps["category"] != "AUTOMATION" {
			t.Errorf("%+v %v", n, aps)
		}
	}
}

func TestATestPushGoesToThatPhoneAtOnce(t *testing.T) {
	r := newRig(t, fast)
	r.register("d1", false)
	if err := r.n.Test(context.Background(), "d1"); err != nil {
		t.Fatal(err)
	}
	n, _ := r.notice(r.relay.await("alert-", 1, 0)[0])
	if n.Kind != Test || n.Place != "Studio" {
		t.Fatalf("%+v", n)
	}
	if err := r.n.Test(context.Background(), "nobody"); err != ErrNotRegistered {
		t.Fatal(err)
	}
}
