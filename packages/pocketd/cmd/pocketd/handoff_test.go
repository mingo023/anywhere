package main

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestTheDryRunRefusesAFileItCantRead(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	os.WriteFile(path, []byte(`{"format":99,"terminals":[]}`), 0o600)
	if err := handoffCheck(path); err == nil {
		t.Fatal("a newer format passed the dry run")
	}
}

func TestTheDryRunAcceptsAPausedTerminalsFile(t *testing.T) {
	term, err := terminal.NewManager().Spawn(terminal.Spec{Cmd: "sh", Args: []string{"-c", "echo hi; sleep 5"}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	if err := term.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	defer term.Resume()
	h, err := term.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "handoff.json")
	f := handoff.File{Format: handoff.Format, Terminals: []handoff.Terminal{{Saved: state.Terminal{TerminalID: "t-1", Cols: 40, Rows: 5}, FD: h.FD, Pid: h.Pid, Screen: h.Screen}}}
	if err := handoff.Write(path, f); err != nil {
		t.Fatal(err)
	}
	if err := handoffCheck(path); err != nil {
		t.Fatal(err)
	}
}
