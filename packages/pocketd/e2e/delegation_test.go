//go:build unix

package e2e

import (
	"bytes"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// anywhereOf is the MCP server the Claude chat in terminal was launched with: its URL and Authorization header.
func anywhereOf(t *testing.T, h *Harness, terminal string) (url, auth string) {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(h.Home, "state", "mcp", terminal+".json"))
	if err != nil {
		t.Fatal(err)
	}
	var cfg struct {
		McpServers map[string]struct {
			URL     string            `json:"url"`
			Headers map[string]string `json:"headers"`
		} `json:"mcpServers"`
	}
	json.Unmarshal(raw, &cfg)
	s := cfg.McpServers["anywhere"]
	return s.URL, s.Headers["Authorization"]
}

func toolsListStatus(t *testing.T, url, auth string) int {
	t.Helper()
	req, _ := http.NewRequest(http.MethodPost, url, bytes.NewBufferString(`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`))
	req.Header.Set("Content-Type", "application/json")
	if auth != "" {
		req.Header.Set("Authorization", auth)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	return resp.StatusCode
}

func saysAbout(agentID string, parts ...string) func(Message) bool {
	return func(m Message) bool {
		if m.Type != "agent.stream" || m.AgentID != agentID || m.Item.Kind != "assistant" {
			return false
		}
		for _, p := range parts {
			if !strings.Contains(m.Item.Text, p) {
				return false
			}
		}
		return true
	}
}

func childOf(phone *Phone, parent string) Message {
	return phone.WaitFor("the child", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.Relationship == "subagent"
	})
}

func TestAClaudeParentDelegatesToACodexChildAndIsWokenWithItsResult(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","title":"Tests","target":{"agent":"codex"}}`)

	child := childOf(phone, parent).Agent
	h.WaitScreen(child.TerminalID, "fake codex ready app-server")
	task := settledAndWoken(h, phone, parent, "completed")
	if task.ChildID != child.ID || task.Agent != "codex" || task.Summary != "echo: write the tests" {
		t.Fatalf("task = %+v, want the codex child's reply", task)
	}
}

func TestACodexParentSeesTheAnywhereToolsAndDelegates(t *testing.T) {
	h, phone, _, parent := chatCodex(t, nil)
	phone.Prompt(parent, "mcp tools/list")
	phone.WaitFor("the tools", streamOf(parent, "assistant", "echo: anywhere_thread_wait delegate_task orchestrator_capabilities task_status tool_search"))

	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","title":"Tests"}`)
	task := settledAndWoken(h, phone, parent, "completed")
	if task.Agent != "codex" || task.Summary != "echo: write the tests" {
		t.Fatalf("task = %+v, want the codex child's reply", task)
	}
}

func TestAnACPParentSeesTheAnywhereToolsAndDelegates(t *testing.T) {
	h, phone, _, parent := chatACP(t, "grok", nil)
	phone.Prompt(parent, "mcp tools/list")
	phone.WaitFor("the tools", streamOf(parent, "assistant", "echo: delegate_task orchestrator_capabilities task_status tool_search"))

	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","title":"Tests"}`)
	task := settledAndWoken(h, phone, parent, "completed")
	if task.Agent != "grok" || task.Summary != "echo: write the tests" {
		t.Fatalf("task = %+v, want the grok child's reply", task)
	}
}
