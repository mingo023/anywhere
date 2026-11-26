// Package statedb is state/pocketd.db, where pocketd keeps what a user can't get back: projects, Tickets,
// Automations, Locals, hosts, devices, push registrations, config and the host id. Only the daemon writes it.
package statedb

import (
	"database/sql"
	"embed"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strconv"

	"pocketd/internal/sqlmigrate"

	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrations embed.FS

// walKept is what the WAL file is cut back to once a checkpoint empties it; SQLite never shrinks it otherwise.
const walKept = 16 << 20

// ErrNoDatabase is a home whose daemon never opened pocketd.db; a reader takes it as empty state.
var ErrNoDatabase = errors.New("pocketd.db doesn't exist yet")

func Path(home string) string { return filepath.Join(home, "state", "pocketd.db") }

// Open opens home/state/pocketd.db (0600), creating it, runs every migration it lacks, and imports the JSON
// files of a pocketd from before it, once. One connection makes it the only writer.
func Open(home string) (*sql.DB, error) {
	path := Path(home)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, err
	}
	f.Close()
	if err := os.Chmod(path, 0o600); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", path+"?_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)&_pragma=foreign_keys(on)&_pragma=journal_size_limit("+strconv.Itoa(walKept)+")")
	if err != nil {
		return nil, err
	}
	// One connection serialises writers, so a query or call into another store (or the registry) while a
	// transaction or open rows hold the connection deadlocks.
	db.SetMaxOpenConns(1)
	if err := sqlmigrate.Run(db, migrations); err != nil {
		db.Close()
		return nil, err
	}
	if err := importOnce(db, home); err != nil {
		db.Close()
		return nil, err
	}
	return db, nil
}

// OpenReadOnly is pocketd.db for a CLI process, beside a running daemon or without one. It migrates and
// imports nothing, so a database older than this binary reads as it is.
func OpenReadOnly(home string) (*sql.DB, error) {
	path := Path(home)
	if _, err := os.Stat(path); errors.Is(err, fs.ErrNotExist) {
		return nil, ErrNoDatabase
	} else if err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", "file:"+path+"?mode=ro&_pragma=busy_timeout(5000)")
	if err != nil {
		return nil, err
	}
	if err := db.Ping(); err != nil {
		db.Close()
		return nil, err
	}
	return db, nil
}

func importOnce(db *sql.DB, home string) error {
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	var at string
	switch err := tx.QueryRow(`SELECT value FROM meta WHERE key = 'imported_at'`).Scan(&at); {
	case err == nil:
		return nil
	case !errors.Is(err, sql.ErrNoRows):
		return err
	}
	if err := importHome(tx, home); err != nil {
		return err
	}
	return tx.Commit()
}
