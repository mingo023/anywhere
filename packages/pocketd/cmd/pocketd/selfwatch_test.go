package main

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// writePocketd puts a script at path, by rename as Sparkle does.
func writePocketd(t *testing.T, path, script string) {
	t.Helper()
	if err := os.WriteFile(path+".tmp", []byte("#!/bin/sh\n"+script+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(path+".tmp", path); err != nil {
		t.Fatal(err)
	}
}

func watching(t *testing.T) *selfWatch {
	exe := filepath.Join(t.TempDir(), "pocketd")
	writePocketd(t, exe, "echo 'pocketd abc-dirty'")
	st, err := os.Stat(exe)
	if err != nil {
		t.Fatal(err)
	}
	sum, err := fileSum(exe)
	if err != nil {
		t.Fatal(err)
	}
	return &selfWatch{exe: exe, sum: sum, seen: st}
}

func TestARebuiltBinaryUpgradesEvenWithTheSameVersion(t *testing.T) {
	w := watching(t)
	calls := 0
	upgrade := func() error { calls++; return nil }
	w.check(upgrade)
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty' # rebuilt")
	w.check(upgrade)
	w.check(upgrade)
	if calls != 1 {
		t.Fatalf("%d upgrades, want 1", calls)
	}
}

func TestATouchedOrIdenticalBinaryDoesNot(t *testing.T) {
	w := watching(t)
	later := time.Now().Add(time.Hour)
	os.Chtimes(w.exe, later, later)
	calls := 0
	upgrade := func() error { calls++; return nil }
	w.check(upgrade)
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty'")
	w.check(upgrade)
	if calls != 0 {
		t.Fatalf("%d upgrades, want 0", calls)
	}
}

func TestABusyUpgradeIsRetriedNextTick(t *testing.T) {
	w := watching(t)
	writePocketd(t, w.exe, "echo 'pocketd 1.1.0'")
	results := []error{errBusy, nil}
	calls := 0
	upgrade := func() error { calls++; return results[calls-1] }
	w.check(upgrade)
	w.check(upgrade)
	w.check(upgrade)
	if calls != 2 {
		t.Fatalf("%d upgrades, want 2", calls)
	}
}

func TestAFailedUpgradeWaitsForTheNextChange(t *testing.T) {
	w := watching(t)
	writePocketd(t, w.exe, "exit 1")
	results := []error{errors.New("dry run failed"), nil}
	calls := 0
	upgrade := func() error { calls++; return results[calls-1] }
	w.check(upgrade)
	w.check(upgrade)
	if calls != 1 {
		t.Fatalf("%d upgrades before the fix, want 1", calls)
	}
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty' # fixed")
	w.check(upgrade)
	if calls != 2 {
		t.Fatalf("%d upgrades after the fix, want 2", calls)
	}
}

func TestABinaryItCantReadYetIsCheckedAgain(t *testing.T) {
	w := watching(t)
	writePocketd(t, w.exe, "echo 'pocketd 1.1.0'")
	if err := os.Chmod(w.exe, 0); err != nil {
		t.Fatal(err)
	}
	calls := 0
	upgrade := func() error { calls++; return nil }
	w.check(upgrade)
	if err := os.Chmod(w.exe, 0o755); err != nil {
		t.Fatal(err)
	}
	w.check(upgrade)
	if calls != 1 {
		t.Fatalf("%d upgrades, want 1", calls)
	}
}
