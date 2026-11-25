package state

import (
	"database/sql"
	"path/filepath"
	"testing"

	"pocketd/internal/statedb/dbtest"
)

func newDB(t *testing.T) *sql.DB {
	t.Helper()
	return dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
}

func TestHostIDIsMadeOnceAndKept(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	id, err := HostID(dbtest.Open(t, path))
	if err != nil || len(id) != 32 {
		t.Fatalf("%q %v", id, err)
	}
	again, err := HostID(dbtest.Open(t, path))
	if err != nil || again != id {
		t.Fatalf("after a restart %q %v, want %q", again, err, id)
	}
}

func TestHostIDKeepsTheOneAnImportPlaced(t *testing.T) {
	db := newDB(t)
	db.Exec(`INSERT INTO meta (key, value) VALUES ('host_id', 'imported')`)
	if id, err := HostID(db); err != nil || id != "imported" {
		t.Fatalf("%q %v", id, err)
	}
}

func TestHostIDsDifferBetweenDatabases(t *testing.T) {
	a, _ := HostID(newDB(t))
	b, _ := HostID(newDB(t))
	if a == "" || a == b {
		t.Fatalf("both %q", a)
	}
}
