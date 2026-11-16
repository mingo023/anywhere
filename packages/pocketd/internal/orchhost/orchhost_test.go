package orchhost

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"strings"
	"testing"
	"time"

	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/timeline"
)

func TestClaudeGetsItsTokenFromAPrivateFileNotArgv(t *testing.T) {
	dir := t.TempDir()
	store, err := orch.OpenPath(filepath.Join(dir, "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	tokens := mcp.NewTokens(store)
	h := &Host{Tokens: tokens, StateDir: dir, URL: "http://127.0.0.1:1/mcp"}

	flags := h.ClaudeFlags("t1", "claude")
	path := filepath.Join(dir, "mcp", "t1.json")
	if strings.Join(flags, " ") != "--mcp-config="+path+" --allowedTools=mcp__anywhere__*" {
		t.Fatalf("flags = %v", flags)
	}
	info, err := os.Stat(path)
	if err != nil || info.Mode().Perm() != 0o600 {
		t.Fatalf("config: %v, mode %v", err, info.Mode())
	}
	var cfg struct {
		McpServers map[string]struct {
			Headers map[string]string `json:"headers"`
		} `json:"mcpServers"`
	}
	raw, _ := os.ReadFile(path)
	json.Unmarshal(raw, &cfg)
	token, _ := strings.CutPrefix(cfg.McpServers["anywhere"].Headers["Authorization"], "Bearer ")
	if g, err := tokens.Resolve(token); err != nil || g.Terminal != "t1" {
		t.Fatalf("token resolves to %+v, %v", g, err)
	}
	if strings.Contains(strings.Join(flags, " "), token) {
		t.Fatal("token in argv")
	}
	if h.ClaudeFlags("t2", "codex") != nil {
		t.Fatal("flags for a non-Claude agent")
	}
}

func TestAChildStartsInPlanModeWhenItsParentIsInIt(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	reg.Add("planner", "/r", "claude", nil).SetTerminal("t-plan")
	reg.Add("doer", "/r", "claude", nil).SetTerminal("t-do")
	h := &Host{Registry: reg, Planning: func(terminal string) bool { return terminal == "t-plan" }}
	for parent, want := range map[string]bool{"planner": true, "doer": false, "": false, "gone": false} {
		if got := h.planning(parent); got != want {
			t.Errorf("planning(%q) = %v", parent, got)
		}
	}
}

func TestALaunchOnALinkedHostIsRefusedAsUnsupported(t *testing.T) {
	h := &Host{}
	payload, _ := json.Marshal(orch.LaunchPayload{Agent: "codex", Env: "devbox", HostProject: "/srv/app"})
	_, err := h.Run(context.Background(), orch.Effect{Kind: orch.EffLaunch, Lane: "launch:t1", Payload: payload})
	if f := orch.AsFailure(err); err == nil || f.Code != orch.CodeUnsupported {
		t.Fatalf("err = %v, want unsupported", err)
	}
}

func TestAnUpgradeHoldsTheBackgroundTasksAnAgentDeclaredAgain(t *testing.T) {
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	err = store.Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`INSERT INTO held (session_id, task_id, until_at) VALUES ('a', 'bg_1', ?)`, time.Now().Add(time.Hour).UnixMilli())
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a", "/r", "claude", nil)
	h := &Host{Registry: reg}
	h.SetEngine(orch.NewEngine(store, nil))

	h.Rehold()

	a.Working()
	a.TurnEnded(false)
	if got := a.Turn(); got != "working" {
		t.Fatalf("the turn ended as %s with its background task running", got)
	}
}

func TestAnUpgradeDoesntHoldABackgroundTaskThatEndedBeforeIt(t *testing.T) {
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	err = store.Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`INSERT INTO held (session_id, task_id, until_at) VALUES ('a', 'b1', ?)`, time.Now().Add(time.Hour).UnixMilli())
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	sink := orch.NewSink(store)
	defer sink.Close()
	reg := agent.NewRegistry(hub.New())
	reg.Sink = sink
	a := reg.Add("a", "/r", "claude", nil)
	a.Working()
	a.Record(timeline.Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"make e2e","run_in_background":true}`)})
	a.Record(timeline.Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "b1"})
	a.Hold([]string{"b1"}, true, time.Hour)
	a.Record(timeline.Event{Kind: "tool_end", TaskID: "b1", OK: true})
	a.TurnEnded(false)
	sink.Flush()
	h := &Host{Registry: reg}
	h.SetEngine(orch.NewEngine(store, nil))

	h.Rehold()

	a.Working()
	a.TurnEnded(false)
	if got := a.Turn(); got == "working" {
		t.Fatal("the turn waits on a task that ended before the upgrade")
	}
}
