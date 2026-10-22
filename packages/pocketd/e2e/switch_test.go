//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/ops"
	"pocketd/internal/registry"
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

func (h *Harness) accountsOp(m ops.Msg) {
	h.t.Helper()
	c := h.Ops()
	c.Send(m)
	if r, err := c.Recv(); err != nil || r.Ev != "accounts" {
		h.t.Fatalf("%s: %+v %v", m.Op, r, err)
	}
}

// runningOn counts the agents on claude's account id, once it has some or after a few seconds.
func (h *Harness) runningOn(id string) int {
	h.t.Helper()
	c := h.Ops()
	for deadline := time.Now().Add(5 * time.Second); ; time.Sleep(100 * time.Millisecond) {
		c.Send(ops.Msg{Op: "accounts"})
		m, err := c.Recv()
		if err != nil {
			h.t.Fatal(err)
		}
		for _, a := range m.Accounts {
			if a.Provider == "claude" && a.ID == id && (a.Running > 0 || time.Now().After(deadline)) {
				return a.Running
			}
		}
		if time.Now().After(deadline) {
			return 0
		}
	}
}

func (h *Harness) setDefault(provider, id string) {
	h.accountsOp(ops.Msg{Op: "accounts.default", Key: provider, ID: id})
}

// switchTo moves agent to claude's account id.
func (h *Harness) switchTo(agent, id string) {
	h.accountsOp(ops.Msg{Op: "accounts.switch", Key: "claude", ID: id, Text: agent})
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

// startedClaude launches a claude for the chat through Pocket: only those switch accounts.
func startedClaude(t *testing.T) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t, launchReady(""), resumingAccounts)
	phone = h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"chat": true}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	term, agent = done.TerminalID, done.AgentID
	h.WaitScreen(term, "fake claude ready")
	return h, phone, term, agent
}

func TestAnIdleAgentSwitchesToTheAccountPickedInItsTerminal(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	phone.Prompt(agent, "marker")
	h.WaitScreen(term, "echo: marker")
	phone.WaitFor("the turn to end", between(agent))

	h.switchTo(agent, "work")

	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))
	h.WaitScreen(term, "fake claude ready")
	if strings.Contains(h.Screen(term), "echo: marker") {
		t.Fatalf("the old process's screen stayed:\n%s", h.Screen(term))
	}
	if n := h.runningOn("work"); n != 1 {
		t.Fatalf("running on work = %d", n)
	}
}

func TestABusyAgentSwitchesOnceItsTurnEnds(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	phone.Prompt(agent, "run ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.switchTo(agent, "work")

	phone.WaitFor("the switch shown", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == agent && m.Agent.Switching == "work"
	})
	time.Sleep(time.Second)
	if got := h.Screen(term); !strings.Contains(got, "prompt: run ls") {
		t.Fatalf("restarted with a request open:\n%s", got)
	}
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})

	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))
}

func TestAnAgentSwitchedBackBeforeItsTurnEndsStays(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	phone.Prompt(agent, "run ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.switchTo(agent, "work")
	h.switchTo(agent, "builtin")
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	phone.WaitFor("the turn to end", between(agent))
	h.WaitScreen(term, "can_use_tool: allow")
	time.Sleep(time.Second)

	if got := h.Screen(term); !strings.Contains(got, "prompt: run ls") || !strings.Contains(got, "can_use_tool: allow") {
		t.Fatalf("the agent restarted:\n%s", got)
	}
}

func TestAnAgentSwitchesBackToTheBuiltInLogin(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	h.switchTo(agent, "work")
	phone.WaitFor("the agent on work", isOn(t, term, agent, "work"))

	h.switchTo(agent, "builtin")

	phone.WaitFor("the agent on the built-in login", isOn(t, term, agent, ""))
	h.WaitScreen(term, "fake claude ready")
}

func TestANewDefaultLeavesRunningAgentsWhereTheyAre(t *testing.T) {
	h, phone, term, agent := startedClaude(t)
	phone.Prompt(agent, "marker")
	phone.WaitFor("the turn to end", between(agent))

	h.setDefault("claude", "work")
	time.Sleep(time.Second)

	if got := h.Screen(term); !strings.Contains(got, "echo: marker") {
		t.Fatalf("the agent restarted:\n%s", got)
	}
	create(phone, "r2", inRepo(h, map[string]any{"chat": true}))
	if done := reply(phone, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	if n := h.runningOn("work"); n != 1 {
		t.Fatalf("running on work = %d", n)
	}
}

// onWork has the test's Project pick claude's account "work".
func onWork(h *Harness) {
	path := filepath.Join(h.Home, "desktop.json")
	var f registry.File
	raw, _ := os.ReadFile(path)
	json.Unmarshal(raw, &f)
	r := f.Repos[h.Repo]
	r.Accounts = map[string]string{"claude": "work"}
	f.Repos[h.Repo] = r
	raw, _ = json.Marshal(f)
	os.WriteFile(path, raw, 0o600)
}

func TestAProjectsNewSessionsStartOnItsAccount(t *testing.T) {
	h := Start(t, launchReady(""), resumingAccounts, onWork)
	phone := h.Owner()

	create(phone, "r1", inRepo(h, map[string]any{"chat": true}))

	if done := reply(phone, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	if n := h.runningOn("work"); n != 1 {
		t.Fatalf("running on work = %d", n)
	}
}

func TestAnAgentTabTheDesktopOpensStartsOnItsProjectsAccount(t *testing.T) {
	h := Start(t, launchReady(""), resumingAccounts, onWork)
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Login: true, Cwd: h.Repo, Argv: []string{"claude"}, Agent: "claude", Cols: 100, Rows: 30})
	if m, err := c.Recv(); err != nil || m.Ev != "spawned" {
		t.Fatalf("%+v %v", m, err)
	}
	if n := h.runningOn("work"); n != 1 {
		t.Fatalf("running on work = %d", n)
	}
}

func between(agent string) func(Message) bool {
	return func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == agent && (m.Agent.Status == "idle" || m.Agent.Status == "done")
	}
}
