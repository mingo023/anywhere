//go:build unix

package e2e

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/ops"
)

func TestAnMCPCallWithABadTokenIsRefusedWith401(t *testing.T) {
	h, _, term, _ := chatClaude(t)
	url, auth := anywhereOf(t, h, term)
	if got := toolsListStatus(t, url, auth); got != http.StatusOK {
		t.Fatalf("its own token: status %d", got)
	}
	for name, bad := range map[string]string{"none": "", "wrong": "Bearer not-a-token", "not a bearer": "Basic dXNlcjpwYXNz"} {
		if got := toolsListStatus(t, url, bad); got != http.StatusUnauthorized {
			t.Errorf("%s: status %d, want 401", name, got)
		}
	}
}

func TestAnMCPTokenOfASessionThatClosedIsRefusedWith401(t *testing.T) {
	h, phone, term, agent := chatClaude(t)
	url, auth := anywhereOf(t, h, term)
	if got := toolsListStatus(t, url, auth); got != http.StatusOK {
		t.Fatalf("before it closed: status %d", got)
	}
	h.Ops().Send(ops.Msg{Op: "close", ID: term})
	phone.WaitStatus(agent, "closed")
	h.eventually("its token revoked", func() bool { return toolsListStatus(t, url, auth) == http.StatusUnauthorized })
}

func TestAToolCallOverTheCallersAccessIsRefusedAsForbidden(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	create(phone, "r2", inRepo(h, map[string]any{"chat": true, "access": "full"}))
	broader := reply(phone, "agent.created", "error")
	if broader.Type != "agent.created" {
		t.Fatalf("got %s", broader.Raw)
	}
	args, _ := json.Marshal(map[string]string{"threadId": broader.AgentID, "message": "hello from below"})
	phone.Prompt(parent, "mcp anywhere_thread_send "+string(args))
	phone.WaitFor("the refusal", saysAbout(parent, `"code":"forbidden"`, "broader than this session"))
	if got := timelineOf(phone, broader.AgentID); len(got) != 0 {
		t.Fatalf("the session above it got %q", got)
	}
}

func TestAnUpgradeKeepsTheMCPPortAndAChildLaunchedBeforeItStillCallsATool(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Hi"}`)
	task := settledAndWoken(h, phone, parent, "completed")
	before := mcpPort(t, h)

	h.Upgrade()

	if after := mcpPort(t, h); after != before {
		t.Fatalf("the MCP port was %d before the upgrade and %d after", before, after)
	}
	again := h.Owner()
	// The child is adopted again a moment after the upgrade; until then a prompt is refused.
	result := saysAbout(task.ChildID, `"limits"`)
	said := false
	h.eventually("the child takes a prompt", func() bool {
		again.Prompt(task.ChildID, "mcp orchestrator_capabilities")
		return again.WaitFor("the prompt's answer", func(m Message) bool {
			said = said || result(m)
			return m.ID == "prompt-mcp orchestrator_capabilities" && (m.Type == "ack" || m.Type == "error")
		}).Type == "ack"
	})
	if !said {
		again.WaitFor("the child's tool result", result)
	}
}

func mcpPort(t *testing.T, h *Harness) int {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(h.Home, "config.json"))
	if err != nil {
		t.Fatal(err)
	}
	var cfg struct {
		MCP struct {
			Port int `json:"port"`
		} `json:"mcp"`
	}
	json.Unmarshal(raw, &cfg)
	if cfg.MCP.Port == 0 {
		t.Fatalf("no MCP port saved in %s", raw)
	}
	return cfg.MCP.Port
}
