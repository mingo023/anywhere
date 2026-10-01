package atomicfile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestWriteReplacesTheFileAndLeavesNothingElse(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "devices.json")
	for _, data := range []string{"old", "new"} {
		if err := Write(path, []byte(data), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	if got, _ := os.ReadFile(path); string(got) != "new" {
		t.Fatalf("%q", got)
	}
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	if entries, _ := os.ReadDir(dir); len(entries) != 1 {
		t.Fatalf("%v", entries)
	}
}

func TestAFailedWriteKeepsTheOldFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "devices.json")
	Write(path, []byte("old"), 0o600)
	os.Chmod(dir, 0o500)
	t.Cleanup(func() { os.Chmod(dir, 0o700) })
	if err := Write(path, []byte("new"), 0o600); err == nil {
		t.Fatal("wrote into a read-only folder")
	}
	if got, _ := os.ReadFile(path); string(got) != "old" {
		t.Fatalf("%q", got)
	}
}
