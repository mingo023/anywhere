package orchhost

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

func run(t *testing.T, h *Host, session, kind string, payload any) {
	t.Helper()
	b, _ := json.Marshal(payload)
	if _, err := h.Run(context.Background(), orch.Effect{Lane: session, Kind: kind, Payload: b}); err != nil {
		t.Fatalf("%s: %v", kind, err)
	}
}

func TestPinUnpinAndMarkUnreadChangeTheAgentAndNothingElse(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("s1", "/r", "claude", nil)
	h := &Host{Registry: reg}
	base := a.Summary()

	run(t, h, "s1", orch.EffPin, map[string]bool{"pinned": true})
	want := base
	want.Pinned = true
	if got := a.Summary(); !equalSummary(got, want) {
		t.Errorf("pin: %+v, want only Pinned changed from %+v", got, base)
	}
	run(t, h, "s1", orch.EffPin, map[string]bool{"pinned": false})
	if got := a.Summary(); !equalSummary(got, base) {
		t.Errorf("unpin: %+v, want %+v", got, base)
	}
	run(t, h, "s1", orch.EffMarkUnread, nil)
	if got := a.Summary(); got.Status != "done" || got.Pinned || got.Title != base.Title {
		t.Errorf("mark_unread: %+v", got)
	}
}

func TestRenameChangesOnlyTheTitle(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("s1", "/r", "claude", nil)
	h := &Host{Registry: reg}
	base := a.Summary()
	run(t, h, "s1", orch.EffRename, orch.TitlePayload{Title: "A better name"})
	want := base
	want.Title = "A better name"
	if got := a.Summary(); !equalSummary(got, want) {
		t.Errorf("rename: %+v, want only Title changed from %+v", got, base)
	}
}

func TestRegeneratingATitleReachesTheSessionsNaming(t *testing.T) {
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	reg := agent.NewRegistry(hub.New())
	reg.Add("s1", "/r", "claude", nil)
	h := &Host{Registry: reg}
	e := orch.NewEngine(store, h)
	e.Live = func(id string) (orch.Session, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return orch.Session{}, false
		}
		s := a.Summary()
		return orch.Session{ID: s.ID, Agent: s.Provider, Status: s.Status}, true
	}
	t.Cleanup(e.Close)

	_, err = e.DispatchWait(orch.NewCommand("", orch.CmdSessionRetitle, "s1", nil), 3*time.Second)
	if err == nil || !strings.Contains(err.Error(), "no prompt to name it from") {
		t.Fatalf("retitle = %v, want naming to read s1's empty timeline", err)
	}
}

func equalSummary(a, b proto.AgentSummary) bool {
	x, _ := json.Marshal(a)
	y, _ := json.Marshal(b)
	return string(x) == string(y)
}

type closing struct {
	agent.Driver
	reg *agent.Registry
	id  string
}

func (c closing) Close() { c.reg.Remove(c.id) }

func TestArchivingASessionRevokesItsTokenAndRemovesItsConfigThroughTheCloseHook(t *testing.T) {
	dir := t.TempDir()
	store, err := orch.OpenPath(filepath.Join(dir, "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	reg := agent.NewRegistry(hub.New())
	sink := orch.NewSink(store)
	reg.Sink = sink
	tokens := mcp.NewTokens(store)
	h := &Host{Registry: reg, Tokens: tokens, StateDir: dir, URL: "http://127.0.0.1:1/mcp"}
	e := orch.NewEngine(store, h)
	h.SetEngine(e)
	e.Agent = AgentInfo
	e.Live = func(id string) (orch.Session, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return orch.Session{}, false
		}
		s := a.Summary()
		return orch.Session{ID: s.ID, Agent: s.Provider, Access: s.Access, Status: s.Status, Terminal: s.TerminalID}, true
	}
	sink.OnTurn = e.OnTurn
	e.OnClosed = h.Closed
	t.Cleanup(func() { e.Close(); sink.Close() })

	a := reg.Add("s1", "/r", "claude", closing{reg: reg, id: "s1"})
	a.SetTerminal("t1")
	h.ClaudeFlags("t1", "claude")
	path := filepath.Join(dir, "mcp", "t1.json")
	var cfg struct {
		McpServers map[string]struct {
			Headers map[string]string `json:"headers"`
		} `json:"mcpServers"`
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	json.Unmarshal(raw, &cfg)
	token, _ := strings.CutPrefix(cfg.McpServers["anywhere"].Headers["Authorization"], "Bearer ")
	if _, err := tokens.Resolve(token); err != nil {
		t.Fatalf("a fresh token: %v", err)
	}

	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdSessionOrganize, "s1", orch.OrganizePayload{Action: "archive"})); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(3 * time.Second)
	for {
		_, err := tokens.Resolve(token)
		_, statErr := os.Stat(path)
		if err == mcp.ErrBadToken && os.IsNotExist(statErr) {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("after archive: token %v, config %v", err, statErr)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestAContextBlockComesBeforeThePromptASessionStartsWith(t *testing.T) {
	if got := firstPrompt("<context>x</context>", "carry on"); got != "<context>x</context>\n\ncarry on" {
		t.Errorf("prompt = %q", got)
	}
	if got := firstPrompt("<context>x</context>", ""); !strings.HasPrefix(got, "<context>x</context>\n\n") || !strings.Contains(got, "Wait for the next instruction") {
		t.Errorf("with no prompt = %q", got)
	}
	if got := firstPrompt("", "just this"); got != "just this" {
		t.Errorf("no context = %q", got)
	}
}

type switching struct {
	agent.Driver
	calls  []string
	refuse string
}

func (d *switching) Settings(model, effort string) error {
	d.calls = append(d.calls, "settings "+model+" "+effort)
	return nil
}

func (d *switching) SetOptions(o map[string]string) error {
	d.calls = append(d.calls, "options "+o["fast"])
	if d.refuse == "options" {
		return errors.New("no options now")
	}
	return nil
}

func (d *switching) Access(access string) error {
	d.calls = append(d.calls, "access "+access)
	return nil
}

func TestSettingsSwitchTheModelThenItsOptionsThenTheAccessAndStopAtTheFirstRefusal(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	d := &switching{}
	reg.Add("s1", "/r", "claude", d)
	h := &Host{Registry: reg}
	run(t, h, "s1", orch.EffSettings, orch.SettingsPayload{Model: "opus", Effort: "high", Options: map[string]string{"fast": "true"}, Access: "full"})
	if want := []string{"settings opus high", "options true", "access full"}; strings.Join(d.calls, ",") != strings.Join(want, ",") {
		t.Fatalf("calls = %q, want %q", d.calls, want)
	}
	d.calls, d.refuse = nil, "options"
	b, _ := json.Marshal(orch.SettingsPayload{Options: map[string]string{"fast": "true"}, Access: "ask"})
	if _, err := h.Run(context.Background(), orch.Effect{Lane: "s1", Kind: orch.EffSettings, Payload: b}); err == nil || err.Error() != "no options now" {
		t.Fatalf("err = %v", err)
	}
	if strings.Join(d.calls, ",") != "options true" {
		t.Fatalf("calls after a refusal = %q", d.calls)
	}
	b, _ = json.Marshal(orch.SettingsPayload{Model: "-bad"})
	if _, err := h.Run(context.Background(), orch.Effect{Lane: "s1", Kind: orch.EffSettings, Payload: b}); err == nil || !strings.HasPrefix(err.Error(), "Model names use") {
		t.Fatalf("a bad model: %v", err)
	}
}

// promptRecorder takes prompts and reports no turn: its agent says when one began.
type promptRecorder struct {
	mu    sync.Mutex
	texts []string
}

func (d *promptRecorder) Prompt(text string) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.texts = append(d.texts, text)
	return nil
}
func (d *promptRecorder) prompts() []string {
	d.mu.Lock()
	defer d.mu.Unlock()
	return slices.Clone(d.texts)
}
func (*promptRecorder) Interrupt() error                   { return nil }
func (*promptRecorder) Compact() error                     { return nil }
func (*promptRecorder) Settings(string, string) error      { return nil }
func (*promptRecorder) Commands() ([]proto.Command, error) { return nil, nil }
func (*promptRecorder) Access(string) error                { return nil }
func (*promptRecorder) Close()                             {}

// promptRig is an engine whose effects run on session s1, driven by d.
func promptRig(t *testing.T, d agent.Driver) (*agent.Agent, *orch.Engine) {
	t.Helper()
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("s1", "/r", "claude", d)
	store, err := orch.Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	e := orch.NewEngine(store, &Host{Registry: reg})
	t.Cleanup(e.Close)
	e.Live = func(id string) (orch.Session, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return orch.Session{}, false
		}
		s := a.Summary()
		return orch.Session{ID: s.ID, Agent: s.Provider, Status: s.Status, Cwd: s.Cwd}, true
	}
	return a, e
}

func TestAMessageSentBeforeTheAgentSaysItsTurnBeganQueuesBehindThatTurn(t *testing.T) {
	d := &promptRecorder{}
	a, e := promptRig(t, d)
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdMessageSend, "s1", orch.SendPayload{Text: "first"})); err != nil {
		t.Fatal(err)
	}
	for deadline := time.Now().Add(3 * time.Second); len(d.prompts()) == 0 && time.Now().Before(deadline); {
		time.Sleep(5 * time.Millisecond)
	}
	// The agent has the prompt, and its turn shows only later.
	time.Sleep(100 * time.Millisecond)
	r, err := e.Dispatch(orch.NewCommand("", orch.CmdMessageSend, "s1", orch.SendPayload{Text: "second"}))
	if err != nil || !strings.Contains(string(r.Result), `"queued":true`) {
		t.Fatalf("the second before the first turn showed: %s, %v", r.Result, err)
	}
	a.Working()
	time.Sleep(50 * time.Millisecond)
	if got := d.prompts(); !slices.Equal(got, []string{"first"}) {
		t.Fatalf("the agent got %q", got)
	}
}

// turnlessRecorder is a promptRecorder whose agent says /goal pause starts no turn.
type turnlessRecorder struct{ promptRecorder }

func (*turnlessRecorder) Turnless(text string) bool { return text == "/goal pause" }

func TestAMessageSentAfterAPromptThatStartsNoTurnGoesOutWithoutWaiting(t *testing.T) {
	d := &turnlessRecorder{}
	_, e := promptRig(t, d)
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdMessageSend, "s1", orch.SendPayload{Text: "/goal pause"})); err != nil {
		t.Fatal(err)
	}
	for deadline := time.Now().Add(3 * time.Second); len(d.prompts()) == 0 && time.Now().Before(deadline); {
		time.Sleep(5 * time.Millisecond)
	}
	time.Sleep(100 * time.Millisecond)
	r, err := e.Dispatch(orch.NewCommand("", orch.CmdMessageSend, "s1", orch.SendPayload{Text: "second"}))
	if err != nil || !strings.Contains(string(r.Result), `"queued":false`) {
		t.Fatalf("the second after a prompt that starts no turn: %s, %v", r.Result, err)
	}
}
