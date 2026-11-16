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
	"pocketd/internal/hub"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
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
		"signed": {ID: "signed", Webhook: true, WebhookSecret: "s3cret"},
		"open":   {ID: "open", Webhook: true},
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

func boundRig(t *testing.T) (*agent.Registry, func(runID string, a proto.Automation) bool, *turnRecorder) {
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
	return reg, func(runID string, a proto.Automation) bool {
		res, watch, ok := post(runID, a)
		if ok && (res.AgentID != a.Bound || watch == nil) {
			t.Fatalf("%+v", res)
		}
		return ok
	}, rec
}

func TestABoundRunPutsItsPromptIntoTheOpenSessionAsATurn(t *testing.T) {
	reg, post, rec := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{})
	if !post("run1", proto.Automation{Bound: "a1", Prompt: "check the build"}) {
		t.Fatal("an open session was refused")
	}
	deadline := time.Now().Add(5 * time.Second)
	for len(rec.turns()) == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	if got := rec.turns(); len(got) != 1 || got[0] != "a1" {
		t.Fatalf("turns started in %v", got)
	}
}

func TestABoundRunFallsBackWhenItsSessionIsGoneOrClosed(t *testing.T) {
	reg, post, rec := boundRig(t)
	if post("run1", proto.Automation{Bound: "nope", Prompt: "p"}) {
		t.Fatal("a session that never existed took the run")
	}
	reg.Add("a2", "/w", "claude", quietDriver{})
	reg.Remove("a2")
	if post("run2", proto.Automation{Bound: "a2", Prompt: "p"}) {
		t.Fatal("a closed session took the run")
	}
	time.Sleep(50 * time.Millisecond)
	if got := rec.turns(); len(got) != 0 {
		t.Fatalf("turns started in %v", got)
	}
}

func TestABoundRunStartsANewSessionWhenItsSessionWasRaisedAboveTheAutomationsAccess(t *testing.T) {
	reg, post, rec := boundRig(t)
	reg.Add("a1", "/w", "claude", quietDriver{}).SetAccess("full")
	if post("run1", proto.Automation{Bound: "a1", Access: "ask", Prompt: "p"}) {
		t.Fatal("a Full session took an Ask automation's run")
	}
	if !post("run2", proto.Automation{Bound: "a1", Access: "full", Prompt: "p"}) {
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
