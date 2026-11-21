//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

// legacyHome runs pocketd the way a host service set up before the move to ~/.anywhere does: HOME is the scratch
// folder, POCKET_HOME names the legacy home in it, and clients dial the socket there.
func legacyHome(h *Harness) {
	legacy := filepath.Join(h.Home, ".coding-pocket-dev")
	if err := os.MkdirAll(filepath.Join(legacy, "state"), 0o700); err != nil {
		h.t.Fatal(err)
	}
	os.WriteFile(filepath.Join(legacy, "state", "names.json"), []byte("{}"), 0o600)
	h.Home, h.Sock = legacy, filepath.Join(legacy, "pocketd.sock")
	h.Env = append(h.Env, "POCKET_HOME="+legacy, "POCKETD_SOCK=")
}

func (h *Harness) moved() {
	h.t.Helper()
	home := filepath.Join(h.UserHome(), ".anywhere", "dev")
	if to, err := os.Readlink(h.Home); err != nil || to != home {
		h.t.Fatalf("the legacy home is %q %v", to, err)
	}
	if _, err := os.Stat(filepath.Join(home, "state", "names.json")); err != nil {
		h.t.Fatal(err)
	}
	if s := h.Status(); s.Home != home || s.Sock != filepath.Join(home, "pocketd.sock") {
		h.t.Fatalf("status says home %s, socket %s", s.Home, s.Sock)
	}
}

func TestAPocketdSetUpBeforeTheMoveMovesItsHomeAndItsOldPathsKeepWorking(t *testing.T) {
	h := Start(t, legacyHome)
	h.moved()
	id, _ := counting(h)
	h.countsPast(id, 2)
}

// The release before the move, built from origin/main, upgrading in place to this one with a Terminal running.
func TestAnUpgradeFromABuildBeforeTheMoveMovesTheHomeAndKeepsTheShell(t *testing.T) {
	old := os.Getenv("POCKETD_OLD")
	if old == "" {
		t.Skip("POCKETD_OLD names no pocketd built before the move")
	}
	h := Start(t, legacyHome, func(h *Harness) {
		h.exe = filepath.Join(h.Home, "bin", "pocketd")
		copyFile(h.t, old, h.exe)
	})
	id, shell := counting(h)
	last := h.countsPast(id, 0)
	pid := h.Status().PID

	h.replaceBinary(filepath.Join(binDir, "pocketd-next"))

	h.eventually("pocketd e2e-next", func() bool { return h.Status().Version == "e2e-next" })
	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	h.moved()
	h.countsPast(id, last[len(last)-1]+5)
}
