package e2e

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"

	"pocketd/internal/ops"
	"pocketd/internal/state"
)

func TestSigtermKillsEveryTerminalAndExitsZero(t *testing.T) {
	h := Start(t)
	f := filepath.Join(h.Home, "pid")
	h.Spawn("sh", "-c", `nohup sh -c 'sleep 60 & echo $! > `+f+`; wait' >/dev/null 2>&1 & wait`)
	var pid int
	h.eventually("the grandchild's pid", func() bool {
		b, _ := os.ReadFile(f)
		pid, _ = strconv.Atoi(strings.TrimSpace(string(b)))
		return pid > 0
	})
	t.Cleanup(func() { syscall.Kill(pid, syscall.SIGKILL) })
	if code := h.Stop(syscall.SIGTERM); code != 0 {
		t.Fatalf("exit code %d, want 0", code)
	}
	h.eventually("the grandchild gone", func() bool { return syscall.Kill(pid, 0) != nil })
}

func TestSigtermKeepsTheTerminalsInTheStateFile(t *testing.T) {
	h := Start(t)
	id := h.Spawn("sh", "-c", "sleep 60")
	h.Stop(syscall.SIGTERM)
	f, err := state.Load(state.Path(h.Home))
	if err != nil || len(f.Terminals) != 1 || f.Terminals[0].TerminalID != id || f.Terminals[0].LaunchDir != h.Home {
		t.Fatalf("state = %+v, %v", f, err)
	}
}

func TestARestartBringsTerminalsBackByID(t *testing.T) {
	h := Start(t)
	id := h.Spawn("sh", "-c", "sleep 60")
	h.Restart()
	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	m, err := c.Recv()
	if err != nil || len(m.Items) != 1 || m.Items[0].ID != id || m.Items[0].Cwd != h.Home || m.Items[0].Cmd != "/bin/sh" {
		t.Fatalf("list = %+v, %v", m, err)
	}
}

func TestResumeOffBringsTheClaudeBackAsAShell(t *testing.T) {
	h, _, term, _ := StartClaude(t)
	h.Restart()
	phone := h.Phone()
	phone.Send(map[string]any{"type": "agent.list", "id": "l"})
	phone.WaitFor("an empty agent.list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "l" && len(m.Agents) == 0 })
	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	if m, err := c.Recv(); err != nil || len(m.Items) != 1 || m.Items[0].ID != term {
		t.Fatalf("list = %+v, %v", m, err)
	}
}
