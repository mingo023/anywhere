//go:build unix

package terminal

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/creack/pty"
	"golang.org/x/sys/unix"

	"pocketd/internal/holder"
)

// TestMain lets this test binary stand in for `pocketd hold`.
func TestMain(m *testing.M) {
	if len(os.Args) == 2 && os.Args[1] == "hold" {
		var spec holder.Spec
		if err := json.NewDecoder(os.Stdin).Decode(&spec); err != nil {
			os.Exit(1)
		}
		holder.Serve(spec, nil, os.Stdout)
		os.Exit(0)
	}
	os.Exit(m.Run())
}

// held is a Manager that runs each Terminal under a holder in dir.
func held(t *testing.T, dir string) *Manager {
	t.Helper()
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	m := NewManager()
	m.Holder = &holder.Starter{Exe: exe, Dir: dir}
	return m
}

func holderDir(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "held")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		holder.EndAll(dir, 0)
		os.RemoveAll(dir)
	})
	return dir
}

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

func TestPromptOverReplacesWhatTheInputHolds(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; echo "got:$x"; sleep 5`)
	waitScreen(t, s, "ready")
	s.Write([]byte("stopped prompt"))
	if err := s.PromptOver("hi"); err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "got:hi")
}

func TestTwoPromptsTypeOneAfterTheOther(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read a; read b; echo "got:$a,$b"; sleep 5`)
	waitScreen(t, s, "ready")
	s.Prompt("one")
	s.Prompt("two")
	waitScreen(t, s, "got:one,two")
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

func TestWatchersHearASpawnAForegroundChangeAndAnExit(t *testing.T) {
	m := NewManager()
	heard := make(chan string, 16)
	stop := m.Watch(func() {
		ids := []string{}
		for _, i := range m.List() {
			ids = append(ids, i.ID+":"+i.Foreground)
		}
		heard <- strings.Join(ids, ",")
	})
	s := spawn(t, m, `read x`)
	id := s.Info().ID
	if got := <-heard; got != id+":" {
		t.Fatalf("spawn: %q", got)
	}
	s.SetForeground("vim")
	s.SetForeground("vim")
	if got := <-heard; got != id+":vim" {
		t.Fatalf("foreground: %q", got)
	}
	s.Write([]byte("\n"))
	if got := <-heard; got != "" {
		t.Fatalf("exit: %q", got)
	}
	stop()
	spawn(t, m, `read x`)
	select {
	case got := <-heard:
		t.Fatalf("a stopped watcher heard %q", got)
	case <-time.After(100 * time.Millisecond):
	}
}

func TestReplaceRunsAnotherProcessInTheSameTerminalWithoutAnExit(t *testing.T) {
	m := NewManager()
	s := spawn(t, m, `echo old; sleep 100`)
	waitScreen(t, s, "old")
	oldPid := s.Pid()
	events := make(chan Event, 64)
	_, detach, err := s.Attach(false, func(e Event) { events <- e })
	if err != nil {
		t.Fatal(err)
	}
	defer detach()

	if err := s.Replace("sh", []string{"-c", "echo new; sleep 100"}, os.Environ(), ""); err != nil {
		t.Fatal(err)
	}

	waitScreen(t, s, "new")
	if strings.Contains(s.Screen(), "old") {
		t.Fatalf("screen kept the old process's output:\n%s", s.Screen())
	}
	waitGone(t, oldPid)
	select {
	case <-s.Done():
		t.Fatal("terminal ended with its old process")
	default:
	}
	if m.Get(s.Info().ID) != s {
		t.Fatal("terminal left the manager")
	}
	if s.Pid() == oldPid {
		t.Fatal("still the old process")
	}
	for len(events) > 0 {
		if e := <-events; e.Kind == "exit" {
			t.Fatal("old process's exit reached the attached screen")
		}
	}
	s.Close()
	<-s.Done()
	if m.Get(s.Info().ID) != nil {
		t.Fatal("terminal still listed after the new process ended")
	}
}

func TestReplaceThatCannotStartLeavesTheTerminalAlone(t *testing.T) {
	s := spawn(t, NewManager(), `echo old; sleep 100`)
	waitScreen(t, s, "old")

	if err := s.Replace("/no/such/binary", nil, os.Environ(), ""); err == nil {
		t.Fatal("replaced with a binary that is not there")
	}

	select {
	case <-s.Done():
		t.Fatal("terminal ended")
	case <-time.After(200 * time.Millisecond):
	}
	if !strings.Contains(s.Screen(), "old") {
		t.Fatalf("screen = %q", s.Screen())
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

// handBefore stands in for a pocketd from before holders: its pty fd stays open across exec.
func handBefore(t *testing.T) {
	cmd := exec.Command("sh", "-c", `echo before; read x; sleep 0.3; echo "after $x"; sleep 1; exit 7`)
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 64)
	for got := ""; !strings.Contains(got, "before"); {
		n, err := f.Read(buf)
		if err != nil {
			t.Fatal(err)
		}
		got += string(buf[:n])
	}
	if _, err := unix.FcntlInt(f.Fd(), unix.F_SETFD, 0); err != nil {
		t.Fatal(err)
	}
	f.Write([]byte("go\r"))
	exe, _ := os.Executable()
	env := slices.DeleteFunc(os.Environ(), func(kv string) bool { return strings.HasPrefix(kv, "POCKETD_HANDOFF") })
	env = append(env, "POCKETD_HANDOFF=after", "POCKETD_HANDOFF_FD="+strconv.Itoa(int(f.Fd())), "POCKETD_HANDOFF_PID="+strconv.Itoa(cmd.Process.Pid),
		"POCKETD_HANDOFF_SCREEN="+base64.StdEncoding.EncodeToString([]byte("before\r\n")))
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

func TestAForegroundChangeTheMomentATerminalIsListedIsHeard(t *testing.T) {
	m := NewManager()
	stop := make(chan struct{})
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			select {
			case <-stop:
				return
			default:
			}
			for _, i := range m.List() {
				if s := m.Get(i.ID); s != nil {
					s.SetForeground("vim")
				}
			}
		}
	}()
	for range 20 {
		spawn(t, m, `exit 0`)
	}
	close(stop)
	<-done
}

func TestATerminalUnderAHolderOutlivesItsPocketdAndIsReattachedWithItsScreen(t *testing.T) {
	dir := holderDir(t)
	s := spawn(t, held(t, dir), `echo before; read x; echo "after $x"; read y; exit 4`)
	waitScreen(t, s, "before")
	pid := s.Pid()
	s.sess.Close()
	<-s.Done()

	found := holder.Find(dir)
	if len(found) != 1 {
		t.Fatalf("%d holders", len(found))
	}
	again, err := held(t, dir).Reattach(found[0])
	if err != nil {
		t.Fatal(err)
	}
	if again.Info().ID != s.Info().ID || again.Pid() != pid {
		t.Fatalf("reattached %+v pid %d, was %s pid %d", again.Info(), again.Pid(), s.Info().ID, pid)
	}
	waitScreen(t, again, "before")
	again.Write([]byte("go\r"))
	waitScreen(t, again, "after go")
	again.Write([]byte("\r"))
	if code := again.ExitCode(); code != 4 {
		t.Fatalf("exit code %d", code)
	}
	waitHolders(t, dir, 0)
}

// waitHolders waits until n holders listen in dir.
func waitHolders(t *testing.T, dir string, n int) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		socks, _ := filepath.Glob(filepath.Join(dir, "*.sock"))
		if len(socks) == n {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("holders %v, want %d", socks, n)
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func TestATerminalUnderAHolderAnswersQueriesAndReportsItsForeground(t *testing.T) {
	s := spawn(t, held(t, holderDir(t)), `stty raw -echo; printf ready; head -c 1 >/dev/null; printf '\033]7501;?\033\\'; head -c 10; sleep 5`)
	waitScreen(t, s, "ready")
	if pg, err := s.Pgrp(); err != nil || pg <= 0 {
		t.Fatalf("pgrp %d, %v", pg, err)
	}
	events := make(chan Event, 16)
	_, detach, err := s.Attach(false, func(e Event) { events <- e })
	if err != nil {
		t.Fatal(err)
	}
	defer detach()
	s.Write([]byte("g"))
	var out strings.Builder
	timeout := time.After(5 * time.Second)
	for strings.Count(out.String(), "\x1b]7501;?\x1b\\") < 2 {
		select {
		case e := <-events:
			out.Write(e.Data)
		case <-timeout:
			t.Fatalf("output = %q", out.String())
		}
	}
}

func TestReplaceUnderAHolderEndsTheOldHolder(t *testing.T) {
	dir := holderDir(t)
	s := spawn(t, held(t, dir), `echo old; sleep 100`)
	waitScreen(t, s, "old")
	if err := s.Replace("sh", []string{"-c", "echo new; sleep 100"}, os.Environ(), ""); err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "new")
	waitHolders(t, dir, 1)
}

func TestWhatAHeldProcessPrintsBeforePocketdAttachesReachesTheScreen(t *testing.T) {
	m := held(t, holderDir(t))
	s, err := m.Spawn(Spec{Cmd: "sh", Args: []string{"-c", "echo early; sleep 30"}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	waitScreen(t, s, "early")
}

func TestAHeldTerminalResizedBeforePocketdRestartsComesBackWithTheSameScreen(t *testing.T) {
	dir := holderDir(t)
	m := held(t, dir)
	s, err := m.Spawn(Spec{Cmd: "sh", Args: []string{"-c", `trap 'printf "\033[2;30Hwide"' WINCH; printf "\033[2J\033[4;15Hnarrow\033[1;1Hready"; while :; do sleep 0.1; done`}, Cols: 20, Rows: 6})
	if err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "ready")
	s.Resize(40, 3)
	waitScreen(t, s, "wide")
	want := s.Screen()
	s.sess.Close()
	<-s.Done()

	found := holder.Find(dir)
	if len(found) != 1 {
		t.Fatalf("%d holders", len(found))
	}
	again, err := held(t, dir).Reattach(found[0])
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(again.Close)
	if got := again.Screen(); got != want {
		t.Fatalf("screen after the restart:\n%q\nbefore:\n%q", got, want)
	}
}
