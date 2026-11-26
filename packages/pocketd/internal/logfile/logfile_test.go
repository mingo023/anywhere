package logfile

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func read(t *testing.T, path string) string {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func TestWriterRotatesPastTheLimitKeepingTheNewest(t *testing.T) {
	path := filepath.Join(t.TempDir(), "logs", "x.log")
	f, err := Open(path, 8, 2)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	for _, line := range []string{"aaaa\n", "bbbb\n", "cccc\n", "dddd\n"} {
		if _, err := f.Write([]byte(line)); err != nil {
			t.Fatal(err)
		}
	}
	if got := [3]string{read(t, path), read(t, path+".1"), read(t, path+".2")}; got != [3]string{"dddd\n", "cccc\n", "bbbb\n"} {
		t.Fatalf("%q", got)
	}
	if _, err := os.Stat(path + ".3"); !os.IsNotExist(err) {
		t.Fatalf("kept a third old file: %v", err)
	}
}

func TestReopenedFileAppendsAndIsPrivate(t *testing.T) {
	path := filepath.Join(t.TempDir(), "x.log")
	for _, line := range []string{"one\n", "two\n"} {
		f, err := Open(path, 1<<20, 1)
		if err != nil {
			t.Fatal(err)
		}
		f.Write([]byte(line))
		f.Close()
	}
	if got := read(t, path); got != "one\ntwo\n" {
		t.Fatalf("%q", got)
	}
	if st, _ := os.Stat(path); runtime.GOOS != "windows" && st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
}

func TestALogHeldOpenElsewhereKeepsItsRotatedFilesAndDoesntRotateOnEveryWrite(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("only Windows refuses to rename a file another process holds open")
	}
	path := filepath.Join(t.TempDir(), "x.log")
	if err := os.WriteFile(path+".1", []byte("old\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	f, err := Open(path, 8, 3)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if _, err := f.Write([]byte("aaaaaa\n")); err != nil {
		t.Fatal(err)
	}
	reader, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, line := range []string{"bb\n", "c\n"} {
		if _, err := f.Write([]byte(line)); err != nil {
			t.Fatal(err)
		}
	}
	reader.Close()
	if got := read(t, path+".1"); got != "old\n" {
		t.Fatalf(".1 holds %q", got)
	}
	if _, err := os.Stat(path + ".2"); !os.IsNotExist(err) {
		t.Fatalf("shifted .1 to .2 though the live log stayed: %v", err)
	}
	if got := read(t, path); got != "aaaaaa\nbb\nc\n" {
		t.Fatalf("live log %q", got)
	}
}

func TestALogLeftMidRotationByACrashBecomesTheNewestRotatedFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "x.log")
	os.WriteFile(path+".1", []byte("older\n"), 0o600)
	os.WriteFile(path+".rotating", []byte("before the crash\n"), 0o600)
	f, err := Open(path, 8, 3)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if got := read(t, path+".1"); got != "before the crash\n" {
		t.Fatalf(".1 holds %q", got)
	}
	if got := read(t, path+".2"); got != "older\n" {
		t.Fatalf(".2 holds %q", got)
	}
	if _, err := os.Stat(path + ".rotating"); !os.IsNotExist(err) {
		t.Fatalf(".rotating is left: %v", err)
	}
}

func TestARotatedFileHeldOpenElsewhereLeavesTheLiveLogWhole(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("only Windows refuses to rename a file another process holds open")
	}
	path := filepath.Join(t.TempDir(), "x.log")
	os.WriteFile(path+".1", []byte("old\n"), 0o600)
	reader, err := os.Open(path + ".1")
	if err != nil {
		t.Fatal(err)
	}
	defer reader.Close()
	f, err := Open(path, 8, 2)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	for _, line := range []string{"aaaaaa\n", "bb\n"} {
		if _, err := f.Write([]byte(line)); err != nil {
			t.Fatal(err)
		}
	}
	if got := read(t, path); got != "aaaaaa\nbb\n" {
		t.Fatalf("live log %q", got)
	}
	if _, err := os.Stat(path + ".rotating"); !os.IsNotExist(err) {
		t.Fatalf(".rotating is left: %v", err)
	}
}
