//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

// Unix socket paths are capped at 104 bytes on macOS; t.TempDir() is too long.
const tempRoot = "/tmp"

const exeSuffix = ""

// platformBins are built beside pocketd for tests that run only on unix.
var platformBins = [][2]string{{"redteam", "./redteam"}}

// rebootSignal stops pocketd as launchd does, and pocketd exits 0.
var rebootSignal os.Signal = syscall.SIGTERM

const rebootExit = 0

// harnessEnv is what Start adds to the env beyond the home and PATH.
func harnessEnv(home string) []string {
	return []string{"POCKETD_RESTORE_SHELL=/bin/sh"}
}

// buildPlatformFakes links anywhere to pocketd and builds the fakes whose tests run only on unix.
func buildPlatformFakes(dir string) {
	if err := os.Symlink(filepath.Join(dir, "pocketd"), filepath.Join(dir, "anywhere")); err != nil {
		panic(err)
	}
	buildFakeACP(filepath.Join(dir, "fake"))
	buildFakePi(filepath.Join(dir, "fake"))
}

// ownBinary serves pocketd from a copy in h.Home, which a test may replace.
func ownBinary(h *Harness) {
	h.exe = filepath.Join(h.Home, "bin", "pocketd")
	copyFile(h.t, filepath.Join(binDir, "pocketd"), h.exe)
}

// replaceBinary swaps h.exe for src by rename, as Sparkle does. Writing over
// a running binary gets it killed on macOS.
func (h *Harness) replaceBinary(src string) {
	h.t.Helper()
	copyFile(h.t, src, h.exe+".new")
	if err := os.Rename(h.exe+".new", h.exe); err != nil {
		h.t.Fatal(err)
	}
}

func copyFile(t *testing.T, src, dst string) {
	t.Helper()
	b, err := os.ReadFile(src)
	if err == nil {
		err = os.MkdirAll(filepath.Dir(dst), 0o755)
	}
	if err == nil {
		err = os.WriteFile(dst, b, 0o755)
	}
	if err != nil {
		t.Fatal(err)
	}
}

// workedThenDone is agentID's turn ending, counted only once it has started.
func workedThenDone(agentID string) func(Message) bool {
	working := false
	return func(m Message) bool {
		working = working || statusOf(agentID, "working")(m)
		return working && statusOf(agentID, "done")(m)
	}
}
