// Package dbtest opens a pocketd.db with statedb's schema and nothing imported, for the tests of packages
// statedb imports, which can't import it back.
package dbtest

import (
	"database/sql"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"pocketd/internal/sqlmigrate"

	_ "modernc.org/sqlite"
)

// Open opens the database at path, creating and migrating it, as statedb.Open does, and closes it when t ends.
// The migrations are read from the source tree, beside this file.
func Open(t testing.TB, path string) *sql.DB {
	t.Helper()
	_, self, _, _ := runtime.Caller(0)
	db, err := sql.Open("sqlite", path+"?_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)&_pragma=foreign_keys(on)")
	if err != nil {
		t.Fatal(err)
	}
	db.SetMaxOpenConns(1)
	t.Cleanup(func() { db.Close() })
	if err := sqlmigrate.Run(db, os.DirFS(filepath.Join(filepath.Dir(self), ".."))); err != nil {
		t.Fatal(err)
	}
	return db
}
