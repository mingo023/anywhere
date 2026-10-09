package terminal

import (
	"encoding/base64"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"slices"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"
)

func spawn(t *testing.T, m *Manager, script string) *Terminal {
	t.Helper()
	s, err := m.Spawn(Spec{Cmd: "sh", Args: []string{"-c", script}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	return s
}

func waitScreen(t *testing.T, s *Terminal, want string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if strings.Contains(s.Screen(), want) {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("screen never showed %q; got:\n%s", want, s.Screen())
}

func TestPromptReachesProcess(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; echo "got:$x"; sleep 5`)
	waitScreen(t, s, "ready")
	if err := s.Prompt("hi"); err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "got:hi")
}

func TestOnInputSeesEachWriteBeforeTheProcess(t *testing.T) {
	var mu sync.Mutex
	var got []string
	m := NewManager()
	m.OnInput = func(id string, b []byte) {
		mu.Lock()
		defer mu.Unlock()
		got = append(got, id+" "+string(b))
	}
	s := spawn(t, m, `read x; echo "got:$x"; sleep 5`)
	s.Write([]byte("a"))
	s.Prompt("hi")
	waitScreen(t, s, "got:ahi")
	mu.Lock()
	defer mu.Unlock()
	id := s.Info().ID
	if want := []string{id + " a", id + " hi", id + " \r"}; !slices.Equal(got, want) {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestAttachGetsSnapshotThenOutput(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'first\n'; read x; echo second; sleep 5`)
	waitScreen(t, s, "first")
	events := make(chan Event, 16)
	snap, detach, err := s.Attach(false, func(e Event) { events <- e })
	if err != nil {
		t.Fatal(err)
	}
	defer detach()
	if !strings.Contains(string(snap), "first") {
		t.Fatalf("snapshot = %q", snap)
	}
	s.Write([]byte("\r"))
	var out strings.Builder
	timeout := time.After(5 * time.Second)
	for !strings.Contains(out.String(), "second") {
		select {
		case e := <-events:
			out.Write(e.Data)
		case <-timeout:
			t.Fatalf("output = %q", out.String())
		}
	}
}

func TestAnswersTheSupportQueryOnThePty(t *testing.T) {
	const query = "\x1b]7501;?\x1b\\"
	s := spawn(t, NewManager(), `stty raw -echo; printf ready; head -c 1 >/dev/null; printf '\033]7501;?\033\\'; head -c 10`)
	waitScreen(t, s, "ready")
	events := make(chan Event, 16)
	_, detach, err := s.Attach(false, func(e Event) { events <- e })
	if err != nil {
		t.Fatal(err)
	}
	defer detach()
	s.Write([]byte("g"))
	// head echoes the reply back after the query the shell printed itself.
	var out strings.Builder
	timeout := time.After(5 * time.Second)
	for strings.Count(out.String(), query) < 2 {
		select {
		case e := <-events:
			out.Write(e.Data)
		case <-timeout:
			t.Fatalf("output = %q", out.String())
		}
	}
}

func waitReport(t *testing.T, got <-chan Report, what string) Report {
	t.Helper()
	select {
	case r := <-got:
		return r
	case <-time.After(3 * time.Second):
		t.Fatalf("no %s report", what)
		return Report{}
	}
}

func TestDeliversRootReportsInOrder(t *testing.T) {
	m := NewManager()
	got := make(chan Report, 4)
	m.OnReport = func(id string, r Report) { got <- r }
	spawn(t, m, `printf '\033]7501;state=working\033\\\033]7501;state=done\033\\'; sleep 1`)
	for _, want := range []string{"working", "done"} {
		if r := waitReport(t, got, want); r.State != want {
			t.Fatalf("got %q, want %q", r.State, want)
		}
	}
}

func TestRemembersTheLatestReport(t *testing.T) {
	m := NewManager()
	got := make(chan Report, 4)
	m.OnReport = func(id string, r Report) { got <- r }
	s := spawn(t, m, `stty raw -echo; printf '\033]7501;state=working\033\\\033]7501;state=done\033\\'; head -c 1 >/dev/null; printf '\033]7501;state=clear\033\\'; sleep 5`)
	waitReport(t, got, "working")
	waitReport(t, got, "done")
	if r := s.Program(); r == nil || r.State != "done" {
		t.Fatalf("Program() = %+v, want done", r)
	}
	s.Write([]byte("x"))
	waitReport(t, got, "clear")
	if r := s.Program(); r != nil {
		t.Fatalf("Program() = %+v after clear", r)
	}
}

func TestExitRemovesTerminalAndKeepsCode(t *testing.T) {
	m := NewManager()
	s := spawn(t, m, "exit 3")
	if code := s.ExitCode(); code != 3 {
		t.Fatalf("code = %d", code)
	}
	if m.Get(s.Info().ID) != nil || len(m.List()) != 0 {
		t.Fatal("terminal still listed after exit")
	}
}

func TestLookPathUsesCallerPath(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(dir+"/only-here", []byte("#!/bin/sh\n"), 0o755)
	got, err := LookPath("only-here", []string{"PATH=/nowhere", "PATH=" + dir})
	if err != nil || got != dir+"/only-here" {
		t.Fatalf("LookPath = %q, %v", got, err)
	}
}

func TestNewIDIsUUIDv4(t *testing.T) {
	id := NewID()
	if len(id) != 36 || id[14] != '4' {
		t.Fatalf("id = %q", id)
	}
}

func TestSetForegroundBroadcastsOnlyChanges(t *testing.T) {
	s := spawn(t, NewManager(), "sleep 5")
	var got []string
	_, detach, err := s.Attach(false, func(e Event) {
		if e.Kind == "foreground" {
			got = append(got, e.Text)
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	defer detach()
	s.SetForeground("npm run dev")
	s.SetForeground("npm run dev")
	if f := s.Info().Foreground; f != "npm run dev" {
		t.Fatalf("Info().Foreground = %q", f)
	}
	s.SetForeground("")
	if !slices.Equal(got, []string{"npm run dev", ""}) {
		t.Fatalf("events = %q", got)
	}
}

func TestPgrpIsTheChildUntilExit(t *testing.T) {
	m := NewManager()
	s := spawn(t, m, "sleep 5")
	if pgrp, err := s.Pgrp(); err != nil || pgrp != s.Pid() {
		t.Fatalf("Pgrp = %d, %v; Pid = %d", pgrp, err, s.Pid())
	}
	if all := m.All(); len(all) != 1 || all[0] != s {
		t.Fatalf("All = %v", all)
	}
	s.Close()
	s.ExitCode()
	if pgrp, err := s.Pgrp(); err != nil || pgrp != 0 {
		t.Fatalf("Pgrp after exit = %d, %v", pgrp, err)
	}
	if all := m.All(); len(all) != 0 {
		t.Fatalf("All after exit = %v", all)
	}
}

func TestRootsMapEachTerminalPidToItsID(t *testing.T) {
	m := NewManager()
	s := spawn(t, m, "sleep 5")
	if got := m.Roots(); len(got) != 1 || got[s.Pid()] != s.Info().ID {
		t.Fatalf("roots = %v", got)
	}
}

func TestTheOriginIsTakenOnce(t *testing.T) {
	s, err := NewManager().Spawn(Spec{Cmd: "sleep", Args: []string{"5"}, Origin: "phone"})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if first, second := s.TakeOrigin(), s.TakeOrigin(); first != "phone" || second != "" {
		t.Fatalf("took %q then %q", first, second)
	}
}

func pidIn(t *testing.T, path string) int {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		b, _ := os.ReadFile(path)
		if pid, err := strconv.Atoi(strings.TrimSpace(string(b))); err == nil {
			t.Cleanup(func() { syscall.Kill(pid, syscall.SIGKILL) })
			return pid
		}
	}
	t.Fatalf("no pid in %s", path)
	return 0
}

func waitGone(t *testing.T, pid int) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		if syscall.Kill(pid, 0) != nil {
			return
		}
	}
	t.Fatalf("pid %d still alive", pid)
}

func TestCloseKillsAChildOfAChild(t *testing.T) {
	f := t.TempDir() + "/pid"
	s := spawn(t, NewManager(), `nohup sh -c 'sleep 30 & echo $! > `+f+`; wait' >/dev/null 2>&1 & wait`)
	pid := pidIn(t, f)
	s.Close()
	waitGone(t, pid)
}

func TestCloseKillsABackgroundJob(t *testing.T) {
	f := t.TempDir() + "/pid"
	s := spawn(t, NewManager(), `set -m; sleep 30 & echo $! > `+f+`; wait`)
	pid := pidIn(t, f)
	if pgid, _ := syscall.Getpgid(pid); pgid == s.Pid() {
		t.Fatal("the job shares the shell's group; the test needs its own")
	}
	s.Close()
	waitGone(t, pid)
}

func TestCloseEscalatesToSigkillAfterGrace(t *testing.T) {
	old := CloseGrace
	CloseGrace = 300 * time.Millisecond
	t.Cleanup(func() { CloseGrace = old })
	s := spawn(t, NewManager(), `trap '' TERM HUP; echo ready; while :; do sleep 1; done`)
	waitScreen(t, s, "ready")
	s.Close()
	select {
	case <-s.Done():
		t.Fatal("ended before the grace ran out")
	case <-time.After(100 * time.Millisecond):
	}
	select {
	case <-s.Done():
	case <-time.After(3 * time.Second):
		t.Fatal("still running after the grace")
	}
}

func TestCloseAllWaitsInParallel(t *testing.T) {
	m := NewManager()
	for range 3 {
		s := spawn(t, m, `trap '' TERM HUP; echo ready; while :; do sleep 1; done`)
		waitScreen(t, s, "ready")
	}
	start := time.Now()
	m.CloseAll(300 * time.Millisecond)
	if took := time.Since(start); took < 300*time.Millisecond || took > 900*time.Millisecond {
		t.Fatalf("CloseAll took %v, want one grace, not three", took)
	}
	if len(m.List()) != 0 {
		t.Fatalf("still listed: %v", m.List())
	}
}

func TestSpawnKeepsTheGivenID(t *testing.T) {
	m := NewManager()
	s, err := m.Spawn(Spec{ID: "t-1", Cmd: "sh", Args: []string{"-c", "sleep 5"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if s.Info().ID != "t-1" || m.Get("t-1") != s {
		t.Fatalf("id = %q", s.Info().ID)
	}
}

func TestAPausedTerminalKeepsItsOutputUntilResumed(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; echo "got:$x"; sleep 5`)
	waitScreen(t, s, "ready")
	if err := s.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	s.Write([]byte("hi\r"))
	time.Sleep(300 * time.Millisecond)
	if strings.Contains(s.Screen(), "got:hi") {
		t.Fatal("a paused terminal read the pty")
	}
	s.Resume()
	waitScreen(t, s, "got:hi")
}

func TestAResizedTerminalCanStillPause(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; stty size; sleep 5`)
	waitScreen(t, s, "ready")
	s.Resize(60, 10)
	s.Write([]byte("\r"))
	waitScreen(t, s, "10 60")
	if err := s.Pause(500 * time.Millisecond); err != nil {
		t.Fatalf("pause after a resize: %v", err)
	}
	s.Resume()
}

func TestPausingAnExitedTerminalSaysItIsClosed(t *testing.T) {
	s := spawn(t, NewManager(), `exit 0`)
	<-s.Done()
	if err := s.Pause(time.Second); !errors.Is(err, ErrClosed) {
		t.Fatalf("err = %v", err)
	}
}

func TestHandoffNeedsAPausedTerminal(t *testing.T) {
	s := spawn(t, NewManager(), `sleep 5`)
	if _, err := s.Handoff(); err == nil {
		t.Fatal("handed over a terminal that is still reading")
	}
}

func TestATerminalSurvivesAnExec(t *testing.T) {
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command(exe, "-test.run=^TestHandoffHelper$")
	cmd.Env = append(os.Environ(), "POCKETD_HANDOFF=before")
	out, err := cmd.CombinedOutput()
	if err != nil || !strings.Contains(string(out), "ADOPTED exit=7") {
		t.Fatalf("%v\n%s", err, out)
	}
}

// TestHandoffHelper is both images of TestATerminalSurvivesAnExec.
func TestHandoffHelper(t *testing.T) {
	switch os.Getenv("POCKETD_HANDOFF") {
	case "before":
		handBefore(t)
	case "after":
		handAfter(t)
	default:
		t.Skip("runs only under TestATerminalSurvivesAnExec")
	}
}

func handBefore(t *testing.T) {
	s, err := NewManager().Spawn(Spec{ID: "t-1", Cmd: "sh", Args: []string{"-c", `echo before; read x; sleep 0.3; echo "after $x"; sleep 1; exit 7`}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "before")
	if err := s.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	h, err := s.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	s.Write([]byte("go\r"))
	exe, _ := os.Executable()
	env := slices.DeleteFunc(os.Environ(), func(kv string) bool { return strings.HasPrefix(kv, "POCKETD_HANDOFF") })
	env = append(env, "POCKETD_HANDOFF=after", "POCKETD_HANDOFF_FD="+strconv.Itoa(h.FD), "POCKETD_HANDOFF_PID="+strconv.Itoa(h.Pid),
		"POCKETD_HANDOFF_SCREEN="+base64.StdEncoding.EncodeToString(h.Screen))
	t.Fatal(syscall.Exec(exe, []string{exe, "-test.run=^TestHandoffHelper$"}, env))
}

func handAfter(t *testing.T) {
	fd, _ := strconv.Atoi(os.Getenv("POCKETD_HANDOFF_FD"))
	pid, _ := strconv.Atoi(os.Getenv("POCKETD_HANDOFF_PID"))
	screen, _ := base64.StdEncoding.DecodeString(os.Getenv("POCKETD_HANDOFF_SCREEN"))
	s, err := NewManager().Adopt(Adopted{ID: "t-1", Cmd: "sh", Cols: 40, Rows: 5, FD: fd, Pid: pid, Screen: screen})
	if err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "after go")
	if !strings.Contains(s.Screen(), "before") {
		t.Fatalf("the old screen is gone:\n%s", s.Screen())
	}
	fmt.Printf("ADOPTED exit=%d\n", s.ExitCode())
}
