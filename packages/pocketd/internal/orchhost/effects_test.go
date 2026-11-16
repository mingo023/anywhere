package orchhost

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
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
