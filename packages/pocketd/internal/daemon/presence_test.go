package daemon

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/codex"
	"pocketd/internal/codex/codextest"
	"pocketd/internal/ops"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

// TestMain is also the fake agent that fakeAgent links: a #!/bin/sh script
// would not do, the kernel shows its argv as /bin/sh <script>.
func TestMain(m *testing.M) {
	switch filepath.Base(os.Args[0]) {
	case "claude", "codex":
		time.Sleep(time.Minute)
		os.Exit(0)
	}
	os.Exit(m.Run())
}

// fakeAgent links this test binary as name, in a dir short enough for a
// codex socket under it.
func fakeAgent(t *testing.T, name string) string {
	t.Helper()
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	dir, _ := os.MkdirTemp("/tmp", "pd")
	t.Cleanup(func() { os.RemoveAll(dir) })
	bin := filepath.Join(dir, name)
	if err := os.Symlink(exe, bin); err != nil {
		t.Fatal(err)
	}
	return bin
}

func agentIn(d *Daemon, term *terminal.Terminal) (proto.AgentSummary, bool) {
	for _, a := range d.Agents.List() {
		if a.TerminalID == term.Info().ID {
			return a, true
		}
	}
	return proto.AgentSummary{}, false
}

func waitAgent(t *testing.T, d *Daemon, term *terminal.Terminal) proto.AgentSummary {
	t.Helper()
	var a proto.AgentSummary
	eventually(t, "agent", func() bool {
		d.poll()
		var ok bool
		a, ok = agentIn(d, term)
		return ok
	})
	return a
}

func waitGone(t *testing.T, d *Daemon, id string) {
	t.Helper()
	eventually(t, "agent gone", func() bool {
		d.poll()
		_, err := d.Agents.Get(id)
		return err != nil
	})
}

func agentPid(term *terminal.Terminal) int {
	_, procs := foreground(term)
	_, p, _ := agentProc(procs)
	return p.Pid
}

func TestClaudeInATerminalIsAnAgentWhileItRuns(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	a := waitAgent(t, d, term)
	if a.Provider != "claude" || !a.Attached || a.ID == term.Info().ID || a.Status != "idle" {
		t.Fatalf("agent = %+v", a)
	}
	ag, _ := d.Agents.Get(a.ID)
	ag.SetTitle("Fix the login bug")
	term.Write([]byte{0x03})
	waitGone(t, d, a.ID)
	if i := term.Info(); i.LastProvider != "claude" || i.LastTitle != "Fix the login bug" {
		t.Fatalf("info = %+v", i)
	}
}

func TestANewClaudePidIsANewAgent(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	claude := fakeAgent(t, "claude")
	term.Write([]byte(claude + "; " + claude + "\r"))
	first := waitAgent(t, d, term)
	pid := d.present[term.Info().ID].pid
	syscall.Kill(pid, syscall.SIGKILL)
	eventually(t, "the second claude", func() bool { p := agentPid(term); return p != 0 && p != pid })
	d.poll()
	if a, ok := agentIn(d, term); !ok || a.ID == first.ID {
		t.Fatalf("agent = %+v, %v", a, ok)
	}
	if _, err := d.Agents.Get(first.ID); err == nil {
		t.Fatal("the first agent is still listed")
	}
}

func TestClosingADetectedAgentStopsOnlyItsProcess(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	a := waitAgent(t, d, term)
	req := proto.PermissionRequest{AgentID: a.ID, ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "ls"}}
	answer := make(chan broker.Answer, 1)
	go func() { answer <- d.Broker.Ask(context.Background(), req, "k") }()
	eventually(t, "open request", func() bool { return len(d.Broker.Open()) == 1 })
	ag, _ := d.Agents.Get(a.ID)
	ag.Driver().Close()
	waitGone(t, d, a.ID)
	select {
	case got := <-answer:
		if got.Decision != "deny" {
			t.Fatalf("answer = %+v", got)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the open request was never answered")
	}
	select {
	case <-term.Done():
		t.Fatal("closing the agent closed its terminal")
	default:
	}
}

func TestAnAgentOutOfTheForegroundTypesNothing(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	a := waitAgent(t, d, term)
	ag, _ := d.Agents.Get(a.ID)
	term.Write([]byte{0x1a})
	eventually(t, "the shell in the foreground", func() bool { p, _ := term.Pgrp(); return p == term.Pid() })
	if err := ag.Driver().Prompt("echo INJ"); err == nil {
		t.Fatal("prompted an agent out of the foreground")
	}
	if err := ag.Driver().Compact(); err == nil || ag.Summary().Compacting {
		t.Fatalf("compact = %v, compacting = %v", err, ag.Summary().Compacting)
	}
	if err := ag.Driver().Interrupt(); err == nil {
		t.Fatal("interrupted an agent out of the foreground")
	}
	term.Write([]byte("echo $((6*7))\r"))
	eventually(t, "the shell's own command", func() bool { return strings.Contains(term.Screen(), "\n42") })
	if s := term.Screen(); strings.Contains(s, "INJ") || strings.Contains(s, "/compact") {
		t.Fatalf("screen = %q", s)
	}
}

func TestClosingLeavesAPidOutsideTheTerminalAlone(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	other := exec.Command("sleep", "30")
	if err := other.Start(); err != nil {
		t.Fatal(err)
	}
	defer other.Process.Kill()
	exited := make(chan struct{})
	go func() { other.Wait(); close(exited) }()
	termDriver{t: term, pid: other.Process.Pid}.Close()
	select {
	case <-exited:
		t.Fatal("closing signalled a pid outside the terminal")
	case <-time.After(500 * time.Millisecond):
	}
}

func TestAnAgentEndsOnce(t *testing.T) {
	d := newDaemon(t)
	term, first := claudeIn(t, d)
	d.endAgent(first)
	waitAgent(t, d, term)
	second := d.presentIn(term.Info().ID)
	d.endAgent(first)
	if d.presentIn(term.Info().ID) != second {
		t.Fatal("ending the first agent again ended the second")
	}
}

func TestAnEndedCodexTakesNoMoreOfItsThread(t *testing.T) {
	d := newDaemon(t)
	home, _ := os.MkdirTemp("/tmp", "cx")
	t.Cleanup(func() { os.RemoveAll(home) })
	long := `{"thread":{"turns":[{"id":"t1","status":"completed","items":[` +
		strings.Repeat(`{"type":"userMessage","id":"u","content":[{"type":"text","text":"old"}]},`, 20000) +
		`{"type":"agentMessage","id":"a","text":"end"}]}]}}`
	codextest.Start(t, codex.Sock([]string{"CODEX_HOME=" + home}), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(long), ""
		}
		return map[string]any{}, ""
	})
	term, pr := codexIn(t, d, home)
	term.Write([]byte("\r"))
	(&codexSock{d: d, sock: pr.sock}).ThreadStatus("th1", "active", nil)
	eventually(t, "the replay", func() bool { return pr.a.Summary().MaxSeq > 0 })
	d.endAgent(pr)
	epoch, _ := pr.a.Timeline.State()
	time.Sleep(200 * time.Millisecond)
	if now, _ := pr.a.Timeline.State(); now != epoch {
		t.Fatalf("the replay went on after the agent ended: epoch %d, then %d", epoch, now)
	}
}

func TestAgentEndsWithItsTerminal(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	a := waitAgent(t, d, term)
	term.Close()
	<-term.Done()
	d.poll()
	if _, err := d.Agents.Get(a.ID); err == nil {
		t.Fatal("the agent outlived its terminal")
	}
}

func TestPocketSpawnedCodexRunsPlainAndIsDetected(t *testing.T) {
	d := newDaemon(t)
	bin := fakeAgent(t, "codex")
	term, err := d.Spawn(ops.Msg{Cmd: bin, Args: []string{"resume"}, Env: []string{"PATH=/bin:/usr/bin", "CODEX_HOME=" + filepath.Dir(bin)}})
	if err != nil {
		t.Fatal(err)
	}
	defer term.Close()
	a := waitAgent(t, d, term)
	if all := d.Agents.List(); len(all) != 1 || a.ID == term.Info().ID {
		t.Fatalf("agents = %+v", all)
	}
	if p, _ := proc.Read(d.present[term.Info().ID].pid); !slices.Equal(p.Argv, []string{bin, "resume"}) {
		t.Fatalf("argv = %q", p.Argv)
	}
}

func TestPocketSpawnedClaudeRunsPlainAndIsDetected(t *testing.T) {
	d := newDaemon(t)
	bin := fakeAgent(t, "claude")
	term, err := d.Spawn(ops.Msg{Cmd: bin, Args: []string{"--resume"}, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	defer term.Close()
	a := waitAgent(t, d, term)
	if all := d.Agents.List(); len(all) != 1 || a.ID == term.Info().ID {
		t.Fatalf("agents = %+v", all)
	}
	if p, _ := proc.Read(d.present[term.Info().ID].pid); !slices.Equal(p.Argv, []string{bin, "--resume"}) {
		t.Fatalf("argv = %q", p.Argv)
	}
}
