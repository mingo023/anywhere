package hostop

import (
	"context"
	"errors"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func write(t *testing.T, path, body string) {
	t.Helper()
	os.MkdirAll(filepath.Dir(path), 0o755)
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func byName(entries []Entry) map[string]Entry {
	m := map[string]Entry{}
	for _, e := range entries {
		m[e.Name] = e
	}
	return m
}

func TestListDescribesEachEntry(t *testing.T) {
	dir := t.TempDir()
	write(t, filepath.Join(dir, "a.txt"), "hello")
	os.Mkdir(filepath.Join(dir, "sub"), 0o755)
	os.Symlink(filepath.Join(dir, "sub"), filepath.Join(dir, "link"))
	entries, err := List(context.Background(), os.Environ(), dir)
	if err != nil || len(entries) != 3 {
		t.Fatalf("%+v %v", entries, err)
	}
	got := byName(entries)
	if a := got["a.txt"]; a.Dir || a.Link || a.Size != 5 || a.Mtime == 0 || a.Ignored {
		t.Fatalf("a.txt %+v", a)
	}
	if s := got["sub"]; !s.Dir || s.Link {
		t.Fatalf("sub %+v", s)
	}
	if l := got["link"]; !l.Dir || !l.Link {
		t.Fatalf("link %+v", l)
	}
}

func TestListMarksWhatGitIgnores(t *testing.T) {
	dir := t.TempDir()
	if out, err := exec.Command("git", "-C", dir, "init", "-q").CombinedOutput(); err != nil {
		t.Fatalf("%s", out)
	}
	write(t, filepath.Join(dir, ".gitignore"), "build/\n*.log\n")
	write(t, filepath.Join(dir, "build", "x"), "")
	write(t, filepath.Join(dir, "a.log"), "")
	write(t, filepath.Join(dir, "main.go"), "")
	entries, err := List(context.Background(), os.Environ(), dir)
	if err != nil {
		t.Fatal(err)
	}
	got := byName(entries)
	if !got["build"].Ignored || !got["a.log"].Ignored || got["main.go"].Ignored || got[".gitignore"].Ignored {
		t.Fatalf("%+v", got)
	}
}

func TestStatReportsAMissingPathWithoutAnError(t *testing.T) {
	dir := t.TempDir()
	if info, err := Stat(filepath.Join(dir, "nope")); err != nil || info.Exists {
		t.Fatalf("%+v %v", info, err)
	}
	write(t, filepath.Join(dir, "f"), "abc")
	if info, err := Stat(filepath.Join(dir, "f")); err != nil || !info.Exists || info.Dir || info.Size != 3 || info.Mtime == 0 {
		t.Fatalf("%+v %v", info, err)
	}
	if info, _ := Stat(dir); !info.Dir {
		t.Fatalf("%+v", info)
	}
}

func TestReadStopsAtMax(t *testing.T) {
	path := filepath.Join(t.TempDir(), "f")
	write(t, path, "0123456789")
	data, size, truncated, err := Read(path, 4)
	if err != nil || string(data) != "0123" || size != 10 || !truncated {
		t.Fatalf("%q %d %v %v", data, size, truncated, err)
	}
	data, _, truncated, _ = Read(path, 0)
	if string(data) != "0123456789" || truncated {
		t.Fatalf("default max: %q %v", data, truncated)
	}
}

func TestWriteWithCreateRefusesAnExistingFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "f")
	if err := Write(path, []byte("one"), true); err != nil {
		t.Fatal(err)
	}
	if err := Write(path, []byte("two"), true); !errors.Is(err, fs.ErrExist) {
		t.Fatalf("%v", err)
	}
	if err := Write(path, []byte("three"), false); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(path); string(b) != "three" {
		t.Fatalf("%q", b)
	}
}

func TestMkdirAndRemove(t *testing.T) {
	dir := t.TempDir()
	deep := filepath.Join(dir, "a", "b")
	if err := Mkdir(deep, false); err == nil {
		t.Fatal("made a dir without its parent")
	}
	if err := Mkdir(deep, true); err != nil {
		t.Fatal(err)
	}
	write(t, filepath.Join(deep, "f"), "x")
	if err := Remove(filepath.Join(dir, "a"), false); err == nil {
		t.Fatal("removed a full dir without recursive")
	}
	if err := Remove(filepath.Join(dir, "a"), true); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "a")); !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("%v", err)
	}
}

func TestTrashIntoKeepsBothWhenANameIsTaken(t *testing.T) {
	dir, trash := t.TempDir(), t.TempDir()
	write(t, filepath.Join(dir, "a.txt"), "1")
	first, err := trashInto(filepath.Join(dir, "a.txt"), trash)
	if err != nil || first != filepath.Join(trash, "a.txt") {
		t.Fatalf("%q %v", first, err)
	}
	write(t, filepath.Join(dir, "a.txt"), "2")
	second, err := trashInto(filepath.Join(dir, "a.txt"), trash)
	if err != nil || second != filepath.Join(trash, "a 2.txt") {
		t.Fatalf("%q %v", second, err)
	}
	if b, _ := os.ReadFile(first); string(b) != "1" {
		t.Fatalf("first trashed file now %q", b)
	}
}

func TestTrashXDGWritesTheTrashInfo(t *testing.T) {
	dir, data := t.TempDir(), t.TempDir()
	path := filepath.Join(dir, "my file")
	write(t, path, "x")
	got, err := trashXDG(path, data)
	if err != nil || got != filepath.Join(data, "Trash", "files", "my file") {
		t.Fatalf("%q %v", got, err)
	}
	info, _ := os.ReadFile(filepath.Join(data, "Trash", "info", "my file.trashinfo"))
	if !strings.HasPrefix(string(info), "[Trash Info]\nPath="+strings.ReplaceAll(path, " ", "%20")+"\nDeletionDate=") {
		t.Fatalf("%q", info)
	}
	write(t, path, "y")
	if got, err := trashXDG(path, data); err != nil || filepath.Base(got) != "my file 2" {
		t.Fatalf("%q %v", got, err)
	}
}
