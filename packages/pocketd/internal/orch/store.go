// Package orch keeps every session's events, projections, command receipts
// and effect outbox in one SQLite store, and runs commands against it.
package orch

import (
	"database/sql"
	"embed"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"pocketd/internal/sqlmigrate"

	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrations embed.FS

const (
	EventsKept   = 24 * time.Hour
	ReceiptsKept = 7 * 24 * time.Hour
	ClosedKept   = 15 * 24 * time.Hour
)

// walKept is what the WAL file is cut back to once a checkpoint empties it; SQLite never shrinks it otherwise.
const walKept = 16 << 20

// Store is state/orchestrator.db. One connection makes it the only writer.
type Store struct{ db *sql.DB }

// Open opens home/state/orchestrator.db (0600), creating it, and runs every migration it lacks.
func Open(home string) (*Store, error) {
	return OpenPath(filepath.Join(home, "state", "orchestrator.db"))
}

func OpenPath(path string) (*Store, error) {
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
	db.SetMaxOpenConns(1)
	s := &Store{db}
	if err := sqlmigrate.Run(db, migrations); err != nil {
		db.Close()
		return nil, err
	}
	return s, nil
}

func (s *Store) Close() error { return s.db.Close() }

type Tx struct{ *sql.Tx }

// Tx is the only write path: f's writes commit together or not at all.
func (s *Store) Tx(f func(*Tx) error) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	if err := f(&Tx{tx}); err != nil {
		tx.Rollback()
		return err
	}
	return tx.Commit()
}

// DB is for reads.
func (s *Store) DB() *sql.DB { return s.db }

// Prune drops what retention no longer keeps: events after a day (each
// session's newest stays), receipts after a week, and a closed session with
// everything it owns 15 days after it closed.
func (s *Store) Prune(now time.Time) error {
	ms := now.UnixMilli()
	return s.Tx(func(tx *Tx) error {
		if _, err := tx.Exec(`DELETE FROM events WHERE at < ? AND seq NOT IN (SELECT MAX(seq) FROM events GROUP BY session_id)`, ms-EventsKept.Milliseconds()); err != nil {
			return err
		}
		if _, err := tx.Exec(`DELETE FROM receipts WHERE at < ?`, ms-ReceiptsKept.Milliseconds()); err != nil {
			return err
		}
		// A client's create runs on a lane of its own, so its row goes with its receipt.
		if _, err := tx.Exec(`DELETE FROM outbox WHERE kind = ? AND state NOT IN ('pending', 'running') AND created_at < ?`, EffCreate, ms-ReceiptsKept.Milliseconds()); err != nil {
			return err
		}
		cut := ms - ClosedKept.Milliseconds()
		gone := `SELECT id FROM sessions WHERE closed_at > 0 AND closed_at < ?`
		for _, q := range []string{
			`DELETE FROM items_fts WHERE rowid IN (SELECT rowid FROM items WHERE session_id IN (` + gone + `))`,
			`DELETE FROM items WHERE session_id IN (` + gone + `)`,
			`DELETE FROM turns WHERE session_id IN (` + gone + `)`,
			`DELETE FROM queue WHERE session_id IN (` + gone + `)`,
			`DELETE FROM requests WHERE session_id IN (` + gone + `)`,
			`DELETE FROM delegations WHERE child_id IN (` + gone + `) OR parent_id IN (` + gone + `)`,
			`DELETE FROM transfers WHERE source_id IN (` + gone + `) OR target_id IN (` + gone + `)`,
			`DELETE FROM events WHERE session_id IN (` + gone + `)`,
			`DELETE FROM outbox WHERE session_id IN (` + gone + `)`,
			// A launch runs on its own lane, so it goes with the session it made or the one that asked.
			`DELETE FROM outbox WHERE kind IN ('` + EffLaunch + `', '` + EffForkLaunch + `') AND state NOT IN ('pending', 'running') AND (json_extract(payload, '$.parent') IN (` + gone + `)
				OR CASE WHEN json_valid(result) THEN json_extract(result, '$.childId') END IN (` + gone + `))`,
			`DELETE FROM tokens WHERE session_id IN (` + gone + `)`,
			`DELETE FROM held WHERE session_id IN (` + gone + `)`,
			`DELETE FROM sessions WHERE closed_at > 0 AND closed_at < ?`,
		} {
			args := []any{cut}
			if strings.Count(q, "?") == 2 {
				args = append(args, cut)
			}
			if _, err := tx.Exec(q, args...); err != nil {
				return err
			}
		}
		return nil
	})
}
