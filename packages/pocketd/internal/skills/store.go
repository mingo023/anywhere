package skills

import (
	"database/sql"
	"os"
	"path/filepath"
	"slices"

	_ "modernc.org/sqlite"

	"pocketd/internal/proto"
)

const schema = `CREATE TABLE IF NOT EXISTS commands (
	provider TEXT NOT NULL,
	config TEXT NOT NULL,
	cwd TEXT NOT NULL,
	pos INTEGER NOT NULL,
	name TEXT NOT NULL,
	description TEXT NOT NULL,
	argument_hint TEXT NOT NULL,
	PRIMARY KEY (provider, config, cwd, pos)
)`

// Key is a scan: provider's commands in cwd on the account whose config dir is Config.
type Key struct{ Provider, Config, Cwd string }

// Store keeps each scan's commands, so a menu shows them before the next scan ends.
type Store struct{ db *sql.DB }

// Open opens home/state/skills.db, creating it.
func Open(home string) (*Store, error) {
	path := filepath.Join(home, "state", "skills.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	// One writer at a time; SQLite answers a second with SQLITE_BUSY.
	db.SetMaxOpenConns(1)
	if _, err := db.Exec(schema); err != nil {
		db.Close()
		return nil, err
	}
	return &Store{db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

// Get is k's commands as last kept; none before its first scan.
func (s *Store) Get(k Key) ([]proto.Command, error) {
	rows, err := s.db.Query(`SELECT name, description, argument_hint FROM commands WHERE provider = ? AND config = ? AND cwd = ? ORDER BY pos`, k.Provider, k.Config, k.Cwd)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []proto.Command
	for rows.Next() {
		var c proto.Command
		if err := rows.Scan(&c.Name, &c.Description, &c.ArgumentHint); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, rows.Err()
}

// Put keeps list as k's commands, reporting whether they differ from those kept.
func (s *Store) Put(k Key, list []proto.Command) (bool, error) {
	kept, err := s.Get(k)
	if err != nil || slices.Equal(kept, list) {
		return false, err
	}
	tx, err := s.db.Begin()
	if err != nil {
		return false, err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`DELETE FROM commands WHERE provider = ? AND config = ? AND cwd = ?`, k.Provider, k.Config, k.Cwd); err != nil {
		return false, err
	}
	for i, c := range list {
		if _, err := tx.Exec(`INSERT INTO commands VALUES (?, ?, ?, ?, ?, ?, ?)`, k.Provider, k.Config, k.Cwd, i, c.Name, c.Description, c.ArgumentHint); err != nil {
			return false, err
		}
	}
	return true, tx.Commit()
}

// List sends k's commands as kept, then scans for them again under home and sends those too if they changed.
func (s *Store) List(k Key, home string, send func([]proto.Command)) error {
	kept, err := s.Get(k)
	if err != nil {
		return err
	}
	send(kept)
	found := Scan(k.Provider, k.Config, home, k.Cwd)
	changed, err := s.Put(k, found)
	if changed {
		send(found)
	}
	return err
}
