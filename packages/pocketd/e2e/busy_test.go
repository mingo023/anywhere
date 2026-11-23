//go:build unix

package e2e

import (
	"os/exec"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

// holdersOf is the holders pocketd at pid started.
func holdersOf(t *testing.T, pid int) []int {
	t.Helper()
	out, _ := exec.Command("pgrep", "-P", strconv.Itoa(pid), "-f", "pocketd hold").Output()
	var pids []int
	for _, f := range strings.Fields(string(out)) {
		if n, err := strconv.Atoi(f); err == nil {
			pids = append(pids, n)
		}
	}
	if len(pids) == 0 {
		t.Fatal("no holder")
	}
	return pids
}

// freeze stops pids for d, as a Mac too loaded to run them does.
func freeze(t *testing.T, pids []int, d time.Duration) {
	t.Helper()
	for _, pid := range pids {
		syscall.Kill(pid, syscall.SIGSTOP)
	}
	thaw := func() {
		for _, pid := range pids {
			syscall.Kill(pid, syscall.SIGCONT)
		}
	}
	t.Cleanup(thaw)
	time.AfterFunc(d, thaw)
}

func keepsTheSession(h *Harness, agent string) {
	h.t.Helper()
	again := h.Owner()
	sessionsListed(h, again, agent)
	h.eventually("the session takes a prompt", func() bool {
		again.Prompt(agent, "after")
		return again.WaitFor("the prompt's answer", func(m Message) bool {
			return m.ID == "prompt-after" && (m.Type == "ack" || m.Type == "error")
		}).Type == "ack"
	})
	h.eventually("the reply", func() bool { return hasReply(again, agent, "after") })
}

func TestASessionOutlivesAHolderTooBusyToAnswer(t *testing.T) {
	h, _, _, agent := StartClaude(t)
	freeze(t, holdersOf(t, h.cmd.Process.Pid), 4*time.Second)
	time.Sleep(5 * time.Second)
	keepsTheSession(h, agent)
}

func TestARestartWhileItsHoldersAreTooBusyToAnswerKeepsTheSession(t *testing.T) {
	h, _, _, agent := StartClaude(t)
	holders := holdersOf(t, h.cmd.Process.Pid)
	h.Stop(syscall.SIGTERM)
	freeze(t, holders, 3*time.Second)
	h.up()
	keepsTheSession(h, agent)
}
