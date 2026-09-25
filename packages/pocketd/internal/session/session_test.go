package session

import (
	"os"
	"strings"
	"testing"
	"time"
)

func spawn(t *testing.T, m *Manager, script string) *Session {
	t.Helper()
	s, err := m.Spawn(Spec{Cmd: "sh", Args: []string{"-c", script}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	return s
}

func waitScreen(t *testing.T, s *Session, want string) {
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

func TestExitRemovesSessionAndKeepsCode(t *testing.T) {
	m := NewManager()
	s := spawn(t, m, "exit 3")
	if code := s.ExitCode(); code != 3 {
		t.Fatalf("code = %d", code)
	}
	if m.Get(s.Info().ID) != nil || len(m.List()) != 0 {
		t.Fatal("session still listed after exit")
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
