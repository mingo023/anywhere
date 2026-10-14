//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/ops"
)

// resumingAccounts resumes agents and lists a claude account "work", with it and the built-in login signed in.
func resumingAccounts(h *Harness) {
	h.resume = true
	root := filepath.Join(h.Home, ".anywhere", "accounts")
	os.MkdirAll(filepath.Join(root, "claude", "work"), 0o700)
	os.WriteFile(filepath.Join(root, "claude", "work", ".credentials.json"), []byte(`{"claudeAiOauth":{"accessToken":"tok"}}`), 0o600)
	os.MkdirAll(filepath.Join(h.Home, ".claude"), 0o700)
	os.WriteFile(filepath.Join(h.Home, ".claude", ".credentials.json"), []byte(`{"claudeAiOauth":{"accessToken":"tok"}}`), 0o600)
	os.WriteFile(filepath.Join(root, "accounts.json"), []byte(`{"accounts":[{"id":"work","provider":"claude","email":"work@example.com"}]}`), 0o600)
}

func (h *Harness) setDefault(provider, id string) {
	h.t.Helper()
	c := h.Ops()
	c.Send(ops.Msg{Op: "accounts.default", Key: provider, ID: id})
	if m, err := c.Recv(); err != nil || m.Ev != "accounts" {
		h.t.Fatalf("accounts.default: %+v %v", m, err)
	}
}

// isOn matches the agent's update that shows it on account, and fails the
// test if the agent closes first: the desktop closes its pane on that.
func isOn(t *testing.T, term, agent, account string) func(Message) bool {
	return func(m Message) bool {
		if m.Type != "agent.update" || m.Agent.ID != agent {
			return false
		}
		if m.Agent.Status == "closed" {
			t.Fatal("the agent closed while it moved")
		}
		return m.Agent.TerminalID == term && m.Agent.Account == account
	}
}

func startedClaude(t *testing.T) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t, launchReady(""), resumingAccounts)
	phone = h.Phone()
	term = h.SpawnReady("claude")
	agent = phone.WaitFor("the claude's agent", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.TerminalID == term && m.Agent.ProviderSessionID != "" && m.Agent.Attached
	}).Agent.ID
	return h, phone, term, agent
}

func TestAnIdleAgentFollowsTheDefaultAccountInItsTerminal(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	h.Prompt(term, "marker")
	h.WaitScreen(term, "echo: marker")
	phone.WaitFor("the turn to end", between(agent))

	h.setDefault("claude", "work")

	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))
	h.WaitScreen(term, "fake claude ready")
	if strings.Contains(h.Screen(term), "echo: marker") {
		t.Fatalf("the old process's screen stayed:\n%s", h.Screen(term))
	}
	var running int
	c := h.Ops()
	c.Send(ops.Msg{Op: "accounts.usage", ID: "work"})
	if m, err := c.Recv(); err == nil {
		for _, a := range m.Accounts {
			if a.ID == "work" {
				running = a.Running
			}
		}
	}
	if running != 1 {
		t.Fatalf("running on work = %d", running)
	}
}

func TestABusyAgentFollowsOnceItsTurnEnds(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	h.Prompt(term, "run ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.setDefault("claude", "work")

	time.Sleep(time.Second)
	if got := h.Screen(term); !strings.Contains(got, "run ls") {
		t.Fatalf("restarted with a request open:\n%s", got)
	}
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})

	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))
}

func TestAnAgentThatIsBackOnTheDefaultBeforeItsTurnEndsStays(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	h.Prompt(term, "run ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.setDefault("claude", "work")
	h.setDefault("claude", "builtin")
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	phone.WaitFor("the turn to end", between(agent))
	h.WaitScreen(term, "hook: allow")
	time.Sleep(time.Second)

	if got := h.Screen(term); !strings.Contains(got, "run ls") || !strings.Contains(got, "hook: allow") {
		t.Fatalf("the agent restarted:\n%s", got)
	}
}

func TestAnAgentReturnsToTheBuiltInLoginWhenTheDefaultDoes(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	h.setDefault("claude", "work")
	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))

	h.setDefault("claude", "builtin")

	phone.WaitFor("the agent on the built-in login", isOn(t, term, agent, ""))
	h.WaitScreen(term, "fake claude ready")
}

func between(agent string) func(Message) bool {
	return func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == agent && (m.Agent.Status == "idle" || m.Agent.Status == "done")
	}
}
