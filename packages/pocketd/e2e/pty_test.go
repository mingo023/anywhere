//go:build unix

package e2e

import (
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/ops"
	"pocketd/internal/terminal"
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
	phone.WaitAll("ack and the turn", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" }, workedThenDone(a.ID))
	h.WaitScreen(id, "echo: hello")
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte{0x03}})
	phone.WaitStatus(a.ID, "closed")
	phone.Send(map[string]any{"type": "agent.list", "id": "l"})
	phone.WaitFor("exited agent unlisted", func(m Message) bool {
		return m.Type == "agent.list" && m.ID == "l" && len(m.Agents) == 0
	})
}

// The desktop closes a terminal, then closes it again on each list that still
// names it; a terminal slow to end may be gone by the time the last one lands.
func TestClosingATerminalSlowToEndAgainAfterItEndedIsNoError(t *testing.T) {
	h := Start(t)
	id := h.Spawn("sh", "-c", "trap '' HUP TERM; echo trapped; sleep 30")
	h.WaitScreen(id, "trapped")
	c := h.Ops()
	listed := func() bool {
		c.Send(ops.Msg{Op: "list"})
		for {
			m, err := c.Recv()
			if err != nil || m.Ev == "error" {
				t.Fatalf("waiting for the list: %+v %v", m, err)
			}
			if m.Ev == "terminals" {
				return slices.ContainsFunc(m.Items, func(i terminal.Info) bool { return i.ID == id })
			}
		}
	}
	c.Send(ops.Msg{Op: "close", ID: id})
	if !listed() {
		t.Fatal("a closing terminal is listed until it ends")
	}
	h.eventually("the terminal ends", func() bool { return !listed() })
	c.Send(ops.Msg{Op: "close", ID: id})
	if listed() {
		t.Fatal("an ended terminal is listed")
	}
}
