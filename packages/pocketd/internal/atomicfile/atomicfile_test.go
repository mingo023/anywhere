package atomicfile

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
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
	if st, _ := os.Stat(path); runtime.GOOS != "windows" && st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	if entries, _ := os.ReadDir(dir); len(entries) != 1 {
		t.Fatalf("%v", entries)
	}
}

func TestAFailedWriteKeepsTheOldFile(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows ignores a folder's mode bits, so chmod can't make it read-only")
	}
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

func TestAWriteWaitsOutAReaderHoldingTheFileOpen(t *testing.T) {
	path := filepath.Join(t.TempDir(), "state.json")
	if err := Write(path, []byte("old"), 0o600); err != nil {
		t.Fatal(err)
	}
	r, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	time.AfterFunc(100*time.Millisecond, func() { r.Close() })
	if err := Write(path, []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}
	if raw, _ := os.ReadFile(path); string(raw) != "new" {
		t.Fatalf("got %q", raw)
	}
}
