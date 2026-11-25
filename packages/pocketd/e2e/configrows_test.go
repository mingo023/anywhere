//go:build unix

package e2e

import (
	"database/sql"
	"testing"

	"pocketd/internal/statedb"
)

// configRow is the JSON value of a config key in pocketd.db, read as the CLI reads it, beside the running daemon.
func configRow(t *testing.T, h *Harness, key string) string {
	t.Helper()
	db, err := statedb.OpenReadOnly(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	var value string
	if err := db.QueryRow(`SELECT value FROM config WHERE key = ?`, key).Scan(&value); err != nil && err != sql.ErrNoRows {
		t.Fatal(err)
	}
	return value
}

// setConfigRow writes a config key while no pocketd runs on the home, as a harness does between two starts.
func setConfigRow(t *testing.T, h *Harness, key, value string) {
	t.Helper()
	db, err := sql.Open("sqlite", "file:"+statedb.Path(h.Home)+"?_pragma=busy_timeout(5000)")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if _, err := db.Exec(`INSERT INTO config (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value`, key, value); err != nil {
		t.Fatal(err)
	}
}
