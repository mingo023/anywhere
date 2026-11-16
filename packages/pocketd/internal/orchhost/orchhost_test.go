package orchhost

import (
	"encoding/json"
	"os"
	"path/filepath"
	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"strings"
	"testing"

	"pocketd/internal/mcp"
	"pocketd/internal/orch"
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
