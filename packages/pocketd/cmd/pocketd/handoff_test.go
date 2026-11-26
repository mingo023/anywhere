package main

import (
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/handoff"
	"pocketd/internal/lock"
	"pocketd/internal/state"
)

func TestTheDryRunRefusesAFileItCantRead(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	os.WriteFile(path, []byte(`{"format":99,"terminals":[]}`), 0o600)
	if err := handoffCheck(path); err == nil {
		t.Fatal("a newer format passed the dry run")
	}
}

func TestTheDryRunAcceptsTheLockItKeepsAcrossExec(t *testing.T) {
	skipHandoffOnWindows(t)
	home := t.TempDir()
	held, err := lock.Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	defer held.Close()
	fd, err := held.Keep()
	if err != nil {
		t.Fatal(err)
	}
	defer handoff.Release(fd)
	path := filepath.Join(t.TempDir(), "handoff.json")
	f := handoff.File{Format: handoff.Format, LockFD: fd, Terminals: []handoff.Terminal{{Saved: state.Terminal{TerminalID: "t-1", Cols: 40, Rows: 5}}}}
	if err := handoff.Write(path, f); err != nil {
		t.Fatal(err)
	}
	if err := handoffCheck(path); err != nil {
		t.Fatal(err)
	}
}
