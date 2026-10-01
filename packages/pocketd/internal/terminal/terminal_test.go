package terminal

import (
	"os"
	"slices"
	"strings"
	"sync"
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
