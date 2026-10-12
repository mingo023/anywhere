//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/ops"
)

func TestStatusNamesTheHostAndKeepsItsIDAcrossRestarts(t *testing.T) {
	h := Start(t)
	st := h.Status()
	if len(st.HostID) != 32 || st.OS != runtime.GOOS || st.Arch != runtime.GOARCH || st.UserHome != h.Home ||
		!filepath.IsAbs(st.Shell) || st.Name == "" || !slices.Contains(st.Caps, "exec.v1") || !slices.Contains(st.Caps, "fs.v1") {
		t.Fatalf("%+v", st)
	}
	h.Restart()
	if again := h.Status().HostID; again != st.HostID {
		t.Fatalf("host id %q after restart, was %q", again, st.HostID)
	}
}

func TestALoginSpawnRunsArgvInHomeThenTheShell(t *testing.T) {
	h := Start(t, launchReady(""))
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Login: true, Cwd: "~", Argv: []string{"sh", "-c", `echo "cwd=$(pwd -P)"`}, Cols: 100, Rows: 30})
	m, err := c.Recv()
	if err != nil || m.Ev != "spawned" {
		t.Fatalf("%+v %v", m, err)
	}
	home, _ := filepath.EvalSymlinks(h.Home)
	h.WaitScreen(m.ID, "cwd="+home)
	h.Ops().Send(ops.Msg{Op: "input", ID: m.ID, Data: []byte("echo back\"\"in-shell\r")})
	h.WaitScreen(m.ID, "backin-shell")
}

func TestExecAndFsWorkOnTheHostsFiles(t *testing.T) {
	h := Start(t, launchReady(""))
	c := h.Ops()
	ask := func(m ops.Msg) ops.Msg {
		t.Helper()
		c.Send(m)
		r, err := c.Recv()
		if err != nil || r.Ev != m.Op {
			t.Fatalf("%s: %+v %v", m.Op, r, err)
		}
		return r
	}
	if r := ask(ops.Msg{Op: "exec", Cwd: h.Repo, Argv: []string{"git", "rev-parse", "--show-toplevel"}, TimeoutMs: 10000}); r.Code != 0 || strings.TrimSpace(string(r.Stdout)) != h.Repo {
		t.Fatalf("exec: %+v", r)
	}
	os.WriteFile(filepath.Join(h.Repo, ".gitignore"), []byte("*.log\n"), 0o644)
	ask(ops.Msg{Op: "fs.write", Path: filepath.Join(h.Repo, "debug.log"), Data: []byte("x"), Create: true})
	ignored := map[string]bool{}
	for _, e := range ask(ops.Msg{Op: "fs.list", Path: h.Repo}).Entries {
		ignored[e.Name] = e.Ignored
	}
	if !ignored["debug.log"] || ignored[".gitignore"] {
		t.Fatalf("list: %v", ignored)
	}
	if r := ask(ops.Msg{Op: "fs.read", Path: filepath.Join(h.Repo, ".gitignore")}); string(r.Data) != "*.log\n" || r.Truncated {
		t.Fatalf("read: %+v", r)
	}
	if r := ask(ops.Msg{Op: "fs.trash", Path: filepath.Join(h.Repo, "debug.log")}); r.Trashed == "" {
		t.Fatalf("trash: %+v", r)
	} else if _, err := os.Stat(r.Trashed); err != nil {
		t.Fatalf("trashed: %v", err)
	}
	if r := ask(ops.Msg{Op: "fs.stat", Path: filepath.Join(h.Repo, "debug.log")}); r.Exists {
		t.Fatalf("stat: %+v", r)
	}
}
