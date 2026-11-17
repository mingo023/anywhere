package main

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/hub"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type fakeSchedules struct {
	autos map[string]proto.Automation
	ran   []string
}

func (f *fakeSchedules) List() []proto.Automation                          { return nil }
func (f *fakeSchedules) Save(a proto.Automation) (proto.Automation, error) { return a, nil }
func (f *fakeSchedules) Delete(string) error                               { return nil }
func (f *fakeSchedules) Get(id string) (proto.Automation, bool)            { a, ok := f.autos[id]; return a, ok }
func (f *fakeSchedules) RunNow(id string) (proto.Run, error) {
	f.ran = append(f.ran, id)
	return proto.Run{ID: "run-" + id}, nil
}

func post(h http.Handler, path, body, sig string) int {
	req := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
	if sig != "" {
		req.Header.Set("X-Anywhere-Signature", sig)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	return w.Code
}

func TestAWebhookWithABadSignatureIsSkipped(t *testing.T) {
	s := &fakeSchedules{autos: map[string]proto.Automation{
		"signed": {ID: "signed", Webhook: true, WebhookSecret: "s3cret", Enabled: true},
		"open":   {ID: "open", Webhook: true, Enabled: true},
		"timed":  {ID: "timed"},
	}}
	h := hooks(s)
	mac := hmac.New(sha256.New, []byte("s3cret"))
	mac.Write([]byte(`{"x":1}`))
	good := hex.EncodeToString(mac.Sum(nil))

	for _, c := range []struct {
		path, sig string
		want      int
	}{
		{"/hooks/signed", "", http.StatusUnauthorized},
		{"/hooks/signed", "00" + good[2:], http.StatusUnauthorized},
		{"/hooks/signed", good, http.StatusAccepted},
		{"/hooks/open", "", http.StatusAccepted},
		{"/hooks/timed", "", http.StatusNotFound},
		{"/hooks/missing", "", http.StatusNotFound},
	} {
		if got := post(h, c.path, `{"x":1}`, c.sig); got != c.want {
			t.Errorf("%s sig %q: %d, want %d", c.path, c.sig, got, c.want)
		}
	}
	if strings.Join(s.ran, ",") != "signed,open" {
		t.Fatalf("ran %v", s.ran)
	}
}

func TestAPausedAutomationsWebhookRunsNothing(t *testing.T) {
	s := &fakeSchedules{autos: map[string]proto.Automation{"paused": {ID: "paused", Webhook: true}}}
	if got := post(hooks(s), "/hooks/paused", `{}`, ""); got != http.StatusNotFound {
		t.Errorf("status %d, want %d", got, http.StatusNotFound)
	}
	if len(s.ran) > 0 {
		t.Fatalf("ran %v", s.ran)
	}
}

type turnRecorder struct {
	mu    sync.Mutex
	lanes []string
}

func (r *turnRecorder) Run(_ context.Context, e orch.Effect) (map[string]any, error) {
	if e.Kind == orch.EffTurnStart {
		r.mu.Lock()
		r.lanes = append(r.lanes, e.Lane)
		r.mu.Unlock()
	}
	return nil, nil
}

func (r *turnRecorder) turns() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]string(nil), r.lanes...)
}

type quietDriver struct{}

func (quietDriver) Prompt(string) error                { return nil }
func (quietDriver) Interrupt() error                   { return nil }
func (quietDriver) Compact() error                     { return nil }
func (quietDriver) Settings(string, string) error      { return nil }
func (quietDriver) Commands() ([]proto.Command, error) { return nil, nil }
func (quietDriver) Access(string) error                { return nil }
func (quietDriver) Close()                             {}

func boundRig(t *testing.T) (*agent.Registry, func(runID string, a proto.Automation) (string, bool), *turnRecorder, *orch.Engine) {
	t.Helper()
	reg := agent.NewRegistry(hub.New())
	store, err := orch.Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	rec := &turnRecorder{}
	e := orch.NewEngine(store, rec)
	t.Cleanup(e.Close)
	e.Live = func(id string) (orch.Session, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return orch.Session{}, false
		}
		s := a.Summary()
		return orch.Session{ID: s.ID, Agent: s.Provider, Status: s.Status, Cwd: s.Cwd}, true
	}
	post := postBound(reg, e)
	return reg, func(runID string, a proto.Automation) (string, bool) {
		res, _, entry, ok := post(runID, a)
		if ok && res.AgentID != a.Bound {
			t.Fatalf("%+v", res)
		}
		return entry, ok
	}, rec, e
}

func TestABoundRunPutsItsPromptIntoTheOpenSessionAsATurn(t *testing.T) {
	reg, post, rec, _ := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{})
	if entry, ok := post("run1", proto.Automation{Bound: "a1", Prompt: "check the build"}); !ok || entry != "" {
		t.Fatalf("an idle session: ok %v, entry %q, want its next turn to be the run's", ok, entry)
	}
	deadline := time.Now().Add(5 * time.Second)
	for len(rec.turns()) == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	if got := rec.turns(); len(got) != 1 || got[0] != "a1" {
		t.Fatalf("turns started in %v", got)
	}
}

func TestABoundRunQueuedInABusySessionIsFollowedByItsPrompt(t *testing.T) {
	reg, post, _, e := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{}).Working()
	entry, ok := post("run1", proto.Automation{Bound: "a1", Prompt: "check the build"})
	if !ok || entry == "" {
		t.Fatalf("ok %v, entry %q", ok, entry)
	}
	if text, state := queuedEntry(e.Store())("a1", entry); state != orch.EntryPending || text != "check the build" {
		t.Fatalf("queued: %q %s", text, state)
	}
}

func TestABoundRunsQueuedPromptIsFoundEditedOrCancelledInTheQueue(t *testing.T) {
	reg, post, _, e := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{}).Working()
	entry, _ := post("run1", proto.Automation{Bound: "a1", Prompt: "check the build"})
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdQueueEdit, "a1", orch.EntryPayload{EntryID: entry, Text: "check the tests"})); err != nil {
		t.Fatal(err)
	}
	if text, state := queuedEntry(e.Store())("a1", entry); state != orch.EntryPending || text != "check the tests" {
		t.Fatalf("after the edit: %q %s", text, state)
	}
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdQueueCancel, "a1", orch.EntryPayload{EntryID: entry})); err != nil {
		t.Fatal(err)
	}
	if _, state := queuedEntry(e.Store())("a1", entry); state != orch.EntryCancelled {
		t.Fatalf("after the cancel: %s", state)
	}
	seen, _ := automation.TurnWatcher(reg, 0, "check the build", entry, queuedEntry(e.Store()))("a1")
	if run, _ := automation.Step(proto.Run{Status: "running"}, seen, true, true); run.Status != "cancelled" {
		t.Fatalf("the run is %q after its queued prompt was cancelled", run.Status)
	}
}

func TestABoundRunsPromptHeldByAStopWaitsUntilTheNextSendReleasesItAndThenFollowsItsTurn(t *testing.T) {
	reg, post, rec, e := boundRig(t)
	a := reg.Add("a1", "/w", "claude", quietDriver{})
	a.Working()
	_, after := a.Timeline.State()
	entry, _ := post("run1", proto.Automation{Bound: "a1", Prompt: "check the build"})
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdSessionStop, "a1", nil)); err != nil {
		t.Fatal(err)
	}
	a.TurnEnded(false)
	if _, state := queuedEntry(e.Store())("a1", entry); state != orch.EntryHeld {
		t.Fatalf("after the stop: %s", state)
	}
	watch := automation.TurnWatcher(reg, after, "check the build", entry, queuedEntry(e.Store()))
	run := proto.Run{Status: "running"}
	seen, _ := watch("a1")
	if run, _ = automation.Step(run, seen, true, true); run.Status != "running" {
		t.Fatalf("the held run is %q: %+v", run.Status, seen)
	}

	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdMessageSend, "a1", orch.SendPayload{Text: "go on", Origin: orch.OriginUser})); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(5 * time.Second)
	for _, state := queuedEntry(e.Store())("a1", entry); state != orch.EntrySent; _, state = queuedEntry(e.Store())("a1", entry) {
		if time.Now().After(deadline) {
			t.Fatalf("the released prompt is %s after %d turns", state, len(rec.turns()))
		}
		time.Sleep(10 * time.Millisecond)
	}
	a.Record(timeline.Event{Kind: "user", Text: "check the build"})
	a.Working()
	a.Record(timeline.Event{Kind: "assistant_text", Text: "Built."})
	a.TurnEnded(false)
	seen, _ = watch("a1")
	if run, _ = automation.Step(run, seen, true, true); run.Status != "succeeded" || run.Summary != "Built." {
		t.Fatalf("after its turn: %+v from %+v", run, seen)
	}
}

func TestABoundRunFallsBackWhenItsSessionIsGoneOrClosed(t *testing.T) {
	reg, post, rec, _ := boundRig(t)
	if _, ok := post("run1", proto.Automation{Bound: "nope", Prompt: "p"}); ok {
		t.Fatal("a session that never existed took the run")
	}
	reg.Add("a2", "/w", "claude", quietDriver{})
	reg.Remove("a2")
	if _, ok := post("run2", proto.Automation{Bound: "a2", Prompt: "p"}); ok {
		t.Fatal("a closed session took the run")
	}
	time.Sleep(50 * time.Millisecond)
	if got := rec.turns(); len(got) != 0 {
		t.Fatalf("turns started in %v", got)
	}
}

func TestABoundRunStartsANewSessionWhenItsSessionWasRaisedAboveTheAutomationsAccess(t *testing.T) {
	reg, post, rec, _ := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{}).SetAccess("full")
	if _, ok := post("run1", proto.Automation{Bound: "a1", Access: "ask", Prompt: "p"}); ok {
		t.Fatal("a Full session took an Ask automation's run")
	}
	if _, ok := post("run2", proto.Automation{Bound: "a1", Access: "full", Prompt: "p"}); !ok {
		t.Fatal("a Full session refused a Full automation's run")
	}
	deadline := time.Now().Add(5 * time.Second)
	for len(rec.turns()) == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	time.Sleep(50 * time.Millisecond)
	if got := rec.turns(); len(got) != 1 {
		t.Fatalf("turns started in %v", got)
	}
}
