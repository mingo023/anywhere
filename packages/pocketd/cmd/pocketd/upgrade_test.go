package main

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/daemon"
	"pocketd/internal/handoff"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/terminal"
)

// fakePocketd is a script that answers --version as pocketd v-bad and fails anything else.
func fakePocketd(t *testing.T) string {
	f := filepath.Join(t.TempDir(), "pocketd")
	if err := os.WriteFile(f, []byte("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocketd v-bad' && exit 0\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return f
}

func TestAFailedDryRunResumesTheTerminalsAndFlagsTheHost(t *testing.T) {
	d := &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New())}
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "sh", Args: []string{"-c", `read x; echo "got:$x"; sleep 5`}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	mon := host.NewMonitor()
	u := &upgrader{exe: fakePocketd(t), home: t.TempDir(), d: d, starting: func() bool { return false }, mon: mon}

	if err := u.run(); err == nil || !strings.Contains(err.Error(), "check") {
		t.Fatalf("err = %v", err)
	}
	if got := mon.State().UpgradeFailed; got != "v-bad" {
		t.Fatalf("upgradeFailed = %q", got)
	}
	if _, err := os.Stat(handoff.Path(u.home)); !os.IsNotExist(err) {
		t.Fatalf("the handoff file is left: %v", err)
	}
	term.Write([]byte("hi\r"))
	for deadline := time.Now().Add(5 * time.Second); !strings.Contains(term.Screen(), "got:hi"); time.Sleep(20 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatalf("the terminal stopped reading:\n%s", term.Screen())
		}
	}
}

func TestAnUpgradeWaitsForALaunchInFlight(t *testing.T) {
	u := &upgrader{exe: fakePocketd(t), starting: func() bool { return true }}
	if err := u.run(); !errors.Is(err, errBusy) {
		t.Fatalf("err = %v", err)
	}
}

func TestABinaryThatIsntPocketdHasNoVersion(t *testing.T) {
	if v, err := binaryVersion("/bin/echo"); err == nil {
		t.Fatalf("version = %q", v)
	}
}
