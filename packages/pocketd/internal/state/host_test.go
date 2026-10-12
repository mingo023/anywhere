package state

import (
	"os"
	"path/filepath"
	"testing"
)

func TestHostIDIsMadeOnceAndKept(t *testing.T) {
	home := t.TempDir()
	id, err := HostID(home)
	if err != nil || len(id) != 32 {
		t.Fatalf("%q %v", id, err)
	}
	again, err := HostID(home)
	if err != nil || again != id {
		t.Fatalf("second read %q %v, want %q", again, err, id)
	}
	if st, err := os.Stat(filepath.Join(home, "state", "host.json")); err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("%v %v", st, err)
	}
}

func TestHostIDsDifferBetweenHomes(t *testing.T) {
	a, _ := HostID(t.TempDir())
	b, _ := HostID(t.TempDir())
	if a == b {
		t.Fatalf("both %q", a)
	}
}
