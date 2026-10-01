package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"

	"pocketd/internal/lock"
	"pocketd/internal/ops"
)

func TestVersionPrintsWithoutAHome(t *testing.T) {
	home := filepath.Join(t.TempDir(), "never")
	out, code := pocketd(t, append(os.Environ(), "POCKET_HOME="+home), "--version")
	if code != 0 || !strings.HasPrefix(out, "pocketd ") {
		t.Fatalf("exit %d: %q", code, out)
	}
	if _, err := os.Stat(home); !os.IsNotExist(err) {
		t.Fatalf("--version touched %s: %v", home, err)
	}
}

func TestStatusExitsThreeWhenNotRunning(t *testing.T) {
	dir := t.TempDir()
	out, code := pocketd(t, append(os.Environ(), "POCKET_HOME="+dir, "POCKETD_SOCK="+filepath.Join(dir, "none.sock")), "status")
	if code != 3 || strings.TrimSpace(out) != "pocketd is not running" {
		t.Fatalf("exit %d: %q", code, out)
	}
}

func TestSecondServeExitsWithTheRunningPid(t *testing.T) {
	h := Start(t)
	pid, _ := os.ReadFile(filepath.Join(h.Home, "pocketd.lock"))
	out, code := h.Pocketd("serve")
	want := "pocketd is already running (pid " + strings.TrimSpace(string(pid)) + ") on " + h.Home + ". Stop it first, or use it."
	if code != 75 || strings.TrimSpace(out) != want {
		t.Fatalf("exit %d: %q, want %q", code, out, want)
	}
	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	if m, err := c.Recv(); err != nil || m.Ev != "terminals" {
		t.Fatalf("the first pocketd stopped answering: %+v %v", m, err)
	}
}

func TestStatusReportsTerminalsAndVersion(t *testing.T) {
	h := Start(t)
	h.Spawn("sh", "-c", "sleep 30")
	version, _ := h.Pocketd("--version")
	out, code := h.Pocketd("status", "--json")
	var st ops.Status
	if err := json.Unmarshal([]byte(out), &st); err != nil || code != 0 {
		t.Fatalf("exit %d %v: %s", code, err, out)
	}
	if "pocketd "+st.Version != strings.TrimSpace(version) || st.Terminals != 1 || st.Home != h.Home || st.Sock != h.Sock {
		t.Fatalf("%+v, version %q", st, version)
	}
	if text, _ := h.Pocketd("status"); !strings.Contains(text, "terminals  1\n") {
		t.Fatalf("%s", text)
	}
}

func TestSigtermStopsCleanlyAndStatsCountsTheRun(t *testing.T) {
	h := Start(t)
	raw, _ := os.ReadFile(filepath.Join(h.Home, "pocketd.lock"))
	pid, _ := strconv.Atoi(strings.TrimSpace(string(raw)))
	syscall.Kill(pid, syscall.SIGTERM)
	h.eventually("the lock to free", func() bool { _, held := lock.Holder(h.Home); return !held })
	events, _ := os.ReadFile(filepath.Join(h.Home, "events.jsonl"))
	if !strings.HasSuffix(string(events), `"kind":"stop","reason":"signal"}`+"\n") {
		t.Fatalf("%s", events)
	}
	out, code := h.Pocketd("stats", "--json")
	var rep struct{ Events int }
	if json.Unmarshal([]byte(out), &rep); code != 0 || rep.Events != 2 {
		t.Fatalf("exit %d: %s", code, out)
	}
}
