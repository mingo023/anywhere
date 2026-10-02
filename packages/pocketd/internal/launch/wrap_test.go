package launch

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// stub stands in for pocketd: it appends its arguments to log.
func stub(t *testing.T) (exe, log string) {
	dir := t.TempDir()
	exe, log = filepath.Join(dir, "pocketd"), filepath.Join(dir, "log")
	if err := os.WriteFile(exe, []byte("#!/bin/sh\necho \"$@\" >> '"+log+"'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return exe, log
}

func runWrapped(t *testing.T, shell, setup string, argv []string) string {
	exe, log := stub(t)
	cmd := exec.Command(shell, Wrap(shell, exe, setup, argv)...)
	cmd.Env = []string{"HOME=" + t.TempDir(), "PATH=/usr/bin:/bin"}
	cmd.Run()
	got, _ := os.ReadFile(log)
	return string(got)
}

func TestWrapRecordsTheExitBeforeTheShellTakesOver(t *testing.T) {
	shells := []string{"/bin/sh"}
	if fish, err := exec.LookPath("fish"); err == nil {
		shells = append(shells, fish)
	}
	for _, shell := range shells {
		if got := runWrapped(t, shell, "", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit agent 3\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestAFailedSetupStopsBeforeTheAgent(t *testing.T) {
	shells := []string{"/bin/sh"}
	if fish, err := exec.LookPath("fish"); err == nil {
		shells = append(shells, fish)
	}
	for _, shell := range shells {
		if got := runWrapped(t, shell, "echo setting up\nfalse", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit setup 1\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestASetupThatCallsExitIsReportedAsSetup(t *testing.T) {
	shells := []string{"/bin/sh"}
	if fish, err := exec.LookPath("fish"); err == nil {
		shells = append(shells, fish)
	}
	for _, shell := range shells {
		if got := runWrapped(t, shell, "exit 4", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit setup 4\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestAPassingSetupIsReportedThenRunsTheAgent(t *testing.T) {
	shells := []string{"/bin/sh"}
	if fish, err := exec.LookPath("fish"); err == nil {
		shells = append(shells, fish)
	}
	for _, shell := range shells {
		if got := runWrapped(t, shell, "true", []string{"/bin/sh", "-c", "exit 0"}); got != "hook exit setup 0\nhook exit agent 0\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}
