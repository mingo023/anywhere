package ops

import (
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/terminal"
)

func TestExecRunsWithTheLoginEnv(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager(), LoginEnv: func() []string { return append(os.Environ(), "FROM_LOGIN=yes") }})
	c.Send(Msg{Op: "exec", ID: "1", Cwd: t.TempDir(), Argv: []string{"sh", "-c", `cat; printf "$FROM_LOGIN" >&2; exit 2`}, Stdin: []byte("in")})
	m := recv(t, c, "exec")
	if m.ID != "1" || m.Code != 2 || string(m.Stdout) != "in" || string(m.Stderr) != "yes" {
		t.Fatalf("%+v", m)
	}
}

func TestExecGivesMinusOneOnTimeout(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "exec", ID: "1", Cwd: t.TempDir(), Argv: []string{"sleep", "10"}, TimeoutMs: 100})
	if m := recv(t, c, "exec"); m.Code != -1 {
		t.Fatalf("%+v", m)
	}
}

func TestFsOpsRoundTrip(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager(), TrashDir: t.TempDir()})
	dir := t.TempDir()
	f := filepath.Join(dir, "sub", "f.txt")
	for _, m := range []Msg{
		{Op: "fs.mkdir", Path: filepath.Join(dir, "sub"), Parents: true},
		{Op: "fs.write", Path: f, Data: []byte("hello"), Create: true},
	} {
		c.Send(m)
		if r := recv(t, c, m.Op); r.Error != "" {
			t.Fatalf("%s: %+v", m.Op, r)
		}
	}
	c.Send(Msg{Op: "fs.write", ID: "again", Path: f, Data: []byte("x"), Create: true})
	if r := recv(t, c, "error"); r.ID != "again" {
		t.Fatalf("%+v", r)
	}
	c.Send(Msg{Op: "fs.read", Path: f, Max: 3})
	if r := recv(t, c, "fs.read"); string(r.Data) != "hel" || r.Size != 5 || !r.Truncated {
		t.Fatalf("%+v", r)
	}
	c.Send(Msg{Op: "fs.list", Path: filepath.Join(dir, "sub")})
	if r := recv(t, c, "fs.list"); len(r.Entries) != 1 || r.Entries[0].Name != "f.txt" || r.Entries[0].Size != 5 {
		t.Fatalf("%+v", r)
	}
	moved := filepath.Join(dir, "g.txt")
	c.Send(Msg{Op: "fs.rename", From: f, To: moved})
	recv(t, c, "fs.rename")
	c.Send(Msg{Op: "fs.stat", Path: moved})
	if r := recv(t, c, "fs.stat"); !r.Exists || r.Dir || r.Size != 5 || r.Mtime == 0 {
		t.Fatalf("%+v", r)
	}
	c.Send(Msg{Op: "fs.remove", Path: filepath.Join(dir, "sub"), Recursive: true})
	recv(t, c, "fs.remove")
	c.Send(Msg{Op: "fs.stat", Path: filepath.Join(dir, "sub")})
	if r := recv(t, c, "fs.stat"); r.Exists {
		t.Fatalf("%+v", r)
	}
}

func TestFsTrashSaysWhereItWent(t *testing.T) {
	trash := t.TempDir()
	t.Setenv("HOME", trash)
	t.Setenv("XDG_DATA_HOME", trash)
	c := start(t, &Server{Terminals: terminal.NewManager(), TrashDir: trash})
	f := filepath.Join(t.TempDir(), "gone.txt")
	os.WriteFile(f, []byte("x"), 0o644)
	c.Send(Msg{Op: "fs.trash", Path: f})
	r := recv(t, c, "fs.trash")
	if _, err := os.Stat(r.Trashed); err != nil || filepath.Base(r.Trashed) != "gone.txt" {
		t.Fatalf("%+v %v", r, err)
	}
	if _, err := os.Stat(f); err == nil {
		t.Fatal("still there")
	}
}

func TestHostOpsRefuseRelativePaths(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	for _, m := range []Msg{
		{Op: "exec", Cwd: "rel", Argv: []string{"true"}},
		{Op: "fs.list", Path: "rel"},
		{Op: "fs.read", Path: ""},
		{Op: "fs.rename", From: "/a", To: "b"},
	} {
		c.Send(m)
		if r := recv(t, c, "error"); r.Error != "paths must be absolute" {
			t.Fatalf("%s: %+v", m.Op, r)
		}
	}
}
