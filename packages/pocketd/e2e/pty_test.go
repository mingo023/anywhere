package e2e

import (
	"path/filepath"
	"testing"

	"pocketd/internal/ops"
)

func TestPromptFromOpsReachesCLI(t *testing.T) {
	h := Start(t)
	id := h.SpawnReady("claude")
	h.Prompt(id, "hello")
	h.WaitScreen(id, "echo: hello")
}

func TestForegroundJobShowsInList(t *testing.T) {
	h := Start(t)
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Cmd: "sh", Cwd: h.Home, Env: []string{"PATH=/bin:/usr/bin", "PS1=ready$ "}})
	m, err := c.Recv()
	if err != nil || m.Ev != "spawned" {
		t.Fatalf("spawn: %+v %v", m, err)
	}
	id := m.ID
	h.WaitScreen(id, "ready$")
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte("sleep 30\r")})
	h.eventually("sleep 30 in the list", func() bool {
		c.Send(ops.Msg{Op: "list"})
		m, _ := c.Recv()
		return len(m.Items) == 1 && m.Items[0].Foreground == "sleep 30"
	})
}

func TestClaudeTypedInAShellIsAnAgent(t *testing.T) {
	h := Start(t)
	phone := h.Phone()
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Cmd: "sh", Cwd: h.Home, Env: []string{"PATH=" + filepath.Join(binDir, "fake") + ":/bin:/usr/bin", "PS1=ready$ "}})
	m, err := c.Recv()
	if err != nil || m.Ev != "spawned" {
		t.Fatalf("spawn: %+v %v", m, err)
	}
	id := m.ID
	h.WaitScreen(id, "ready$")
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte("claude\r")})
	flashed := false
	a := phone.WaitFor("bound claude", func(m Message) bool {
		mine := m.Type == "agent.update" && m.Agent.TerminalID == id
		flashed = flashed || mine && !m.Agent.Attached
		return mine && m.Agent.ProviderSessionID != ""
	}).Agent
	if flashed || a.ID == id {
		t.Fatalf("flashed not attached: %v, agent: %+v", flashed, a)
	}
	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": a.ID, "text": "hello"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" })
	phone.WaitStatus(a.ID, "working")
	phone.WaitStatus(a.ID, "done")
	h.WaitScreen(id, "echo: hello")
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte{0x03}})
	phone.WaitStatus(a.ID, "closed")
	c.Send(ops.Msg{Op: "list"})
	if m, _ := c.Recv(); len(m.Items) != 1 || m.Items[0].LastProvider != "claude" {
		t.Fatalf("list: %+v", m.Items)
	}
}
