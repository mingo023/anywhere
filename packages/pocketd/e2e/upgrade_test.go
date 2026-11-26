//go:build unix

package e2e

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"testing"

	"pocketd/internal/ops"
)

// counting spawns a shell that prints n=1, n=2, … every 200ms and returns
// its terminal and pid.
func counting(h *Harness) (string, int) {
	h.t.Helper()
	f := filepath.Join(h.Home, "counter.pid")
	id := h.Spawn("sh", "-c", `echo $$ > `+f+`; i=0; while :; do i=$((i+1)); echo n=$i; sleep 0.2; done`)
	var pid int
	h.eventually("the counter's pid", func() bool {
		b, _ := os.ReadFile(f)
		pid, _ = strconv.Atoi(strings.TrimSpace(string(b)))
		return pid > 0
	})
	return id, pid
}

var countLine = regexp.MustCompile(`(?m)^n=(\d+)$`)

// countsPast waits until id's screen counts past n and returns every count it shows.
func (h *Harness) countsPast(id string, n int) []int {
	h.t.Helper()
	var got []int
	h.eventually(fmt.Sprintf("a count past %d", n), func() bool {
		got = nil
		for _, m := range countLine.FindAllStringSubmatch(h.Screen(id), -1) {
			c, _ := strconv.Atoi(m[1])
			got = append(got, c)
		}
		return len(got) > 0 && got[len(got)-1] > n
	})
	return got
}

func TestAnUpgradeKeepsTheShellRunning(t *testing.T) {
	h := Start(t)
	id, shell := counting(h)
	before := h.countsPast(id, 3)
	last := before[len(before)-1]
	pid := h.Status().PID

	h.Upgrade()

	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	if m, err := c.Recv(); err != nil || len(m.Items) != 1 || m.Items[0].ID != id || m.Items[0].Cmd != "sh" {
		t.Fatalf("list = %+v, %v", m, err)
	}
	after := h.countsPast(id, last+5)
	if after[0] > last {
		t.Fatalf("the screen before the upgrade is gone: %v, last was %d", after, last)
	}
	for i := 1; i < len(after); i++ {
		if after[i] != after[i-1]+1 {
			t.Fatalf("output was lost: %v", after)
		}
	}
}

func TestAnUpgradeKeepsTheShellWhenTheOwnerPortIsTaken(t *testing.T) {
	h := Start(t, ownerOverTCP)
	_, shell := counting(h)
	pid := h.Status().PID
	busy, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer busy.Close()
	if out, code := h.Pocketd("config", "set", "ownerListen", busy.Addr().String()); code != 0 {
		t.Fatalf("config set ownerListen: %s", out)
	}

	h.Upgrade()

	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
}

func TestAnUpgradeKeepsTheAgentAndReasksItsOpenPermission(t *testing.T) {
	h, phone, term, id := StartClaude(t)
	h.Prompt(term, "run ls")
	asked := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.Upgrade()

	again := h.Phone()
	req := again.WaitFor("the request asked again", func(m Message) bool {
		return m.Type == "permission.request" && m.Request.AgentID == id && m.Request.RequestID != asked.Request.RequestID
	})
	again.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	again.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "r1" })
	h.WaitScreen(term, "hook: allow")
	again.Send(map[string]any{"type": "agent.list", "id": "l"})
	list := again.WaitFor("agent.list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "l" })
	if len(list.Agents) != 1 || list.Agents[0].ID != id || list.Agents[0].Restore != "" {
		t.Fatalf("agents after the upgrade: %s", list.Raw)
	}
}

func TestAFailedDryRunKeepsTheOldPocketd(t *testing.T) {
	h := Start(t, ownBinary)
	owner := h.Owner("host.v1")
	id, shell := counting(h)
	last := h.countsPast(id, 0)
	pid := h.Status().PID
	bad := filepath.Join(t.TempDir(), "pocketd")
	os.WriteFile(bad, []byte("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocketd v-bad' && exit 0\nexit 1\n"), 0o755)
	h.replaceBinary(bad)

	out, code := h.Pocketd("upgrade")

	if code == 0 || !strings.Contains(out, "check") {
		t.Fatalf("upgrade exited %d: %s", code, out)
	}
	owner.WaitFor("upgradeFailed", func(m Message) bool { return m.Type == "host.changed" && m.Host.UpgradeFailed == "v-bad" })
	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	h.countsPast(id, last[len(last)-1]+5)
}

func TestReplacingTheBinaryUpgradesPocketd(t *testing.T) {
	h := Start(t, ownBinary)
	id, shell := counting(h)
	last := h.countsPast(id, 0)
	pid := h.Status().PID

	h.replaceBinary(filepath.Join(binDir, "pocketd-next"))

	h.eventually("pocketd e2e-next", func() bool { return h.Status().Version == "e2e-next" })
	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	h.countsPast(id, last[len(last)-1]+5)
}

func TestAnUpgradeKeepsTheTranscriptLinesWrittenWhilePocketdWasNotTailing(t *testing.T) {
	h := Start(t, ownBinary)
	phone := h.Phone()
	term := h.SpawnReady("claude")
	id := phone.WaitFor("the claude's agent", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.TerminalID == term && m.Agent.ProviderSessionID != ""
	}).Agent.ID
	phone.Prompt(id, "hang")
	phone.WaitFor("the turn under way", streamOf(id, "user", "hang"))
	transcripts, _ := filepath.Glob(filepath.Join(h.ClaudeDir, "projects", "*", "*.jsonl"))
	if len(transcripts) != 1 {
		t.Fatalf("transcripts %q", transcripts)
	}
	line := filepath.Join(h.Home, "line.jsonl")
	os.WriteFile(line, []byte(`{"type":"assistant","uuid":"u-gap","message":{"id":"msg_gap","role":"assistant","model":"fake-model","content":[{"type":"text","text":"written between the two"}]}}`+"\n"), 0o600)
	// The next pocketd's exec is the one moment no pocketd tails the transcript.
	wrapper := filepath.Join(h.Home, "wrapper")
	os.WriteFile(wrapper, []byte(fmt.Sprintf("#!/bin/sh\n[ \"$1\" = serve ] && cat '%s' >> '%s'\nexec '%s' \"$@\"\n", line, transcripts[0], filepath.Join(binDir, "pocketd-next"))), 0o755)

	h.replaceBinary(wrapper)

	h.eventually("pocketd e2e-next", func() bool { return h.Status().Version == "e2e-next" })
	again := h.Phone()
	h.eventually("the line in the timeline", func() bool {
		return slices.Contains(timelineOf(again, id), "assistant written between the two")
	})
	if got := timelineOf(again, id); !slices.Equal(got, []string{"user hang", "assistant written between the two"}) {
		t.Fatalf("timeline %q", got)
	}
}

func TestAFailedUpgradeGoesOnFollowingTheClaudeTranscript(t *testing.T) {
	h := Start(t, ownBinary)
	phone := h.Phone()
	term := h.SpawnReady("claude")
	id := phone.WaitFor("the claude's agent", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.TerminalID == term && m.Agent.ProviderSessionID != ""
	}).Agent.ID
	bad := filepath.Join(t.TempDir(), "pocketd")
	os.WriteFile(bad, []byte("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocketd v-bad' && exit 0\nexit 1\n"), 0o755)
	h.replaceBinary(bad)
	if out, code := h.Pocketd("upgrade"); code == 0 {
		t.Fatalf("upgrade exited 0: %s", out)
	}

	phone.Prompt(id, "hello")

	phone.WaitFor("the reply", streamOf(id, "assistant", "echo: hello"))
}
