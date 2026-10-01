package logfile

import (
	"os"
	"path/filepath"
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
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
}
