package team

import (
	"database/sql"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"

	_ "modernc.org/sqlite"

	"pocketd/internal/proto"
)

const schema = `CREATE TABLE IF NOT EXISTS teams (
	lead TEXT PRIMARY KEY,
	team TEXT NOT NULL
)`

// Store keeps each lead's team, so teams outlive a pocketd restart.
type Store struct{ db *sql.DB }

// OpenStore opens home/state/team.db, creating it.
func OpenStore(home string) (*Store, error) {
	path := filepath.Join(home, "state", "team.db")
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

// Get is lead's team; false when it has none.
func (s *Store) Get(lead string) (proto.Team, bool, error) {
	var raw string
	err := s.db.QueryRow(`SELECT team FROM teams WHERE lead = ?`, lead).Scan(&raw)
	if errors.Is(err, sql.ErrNoRows) {
		return proto.Team{}, false, nil
	}
	if err != nil {
		return proto.Team{}, false, err
	}
	var t proto.Team
	if err := json.Unmarshal([]byte(raw), &t); err != nil {
		return proto.Team{}, false, err
	}
	return t, true, nil
}

// Put keeps t as its lead's team, in place of any it had.
func (s *Store) Put(t proto.Team) error {
	raw, err := json.Marshal(t)
	if err != nil {
		return err
	}
	_, err = s.db.Exec(`INSERT INTO teams (lead, team) VALUES (?, ?) ON CONFLICT (lead) DO UPDATE SET team = excluded.team`, t.Lead, string(raw))
	return err
}

// List is every lead's team, by lead id.
func (s *Store) List() ([]proto.Team, error) {
	rows, err := s.db.Query(`SELECT team FROM teams ORDER BY lead`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []proto.Team
	for rows.Next() {
		var raw string
		if err := rows.Scan(&raw); err != nil {
			return nil, err
		}
		var t proto.Team
		if err := json.Unmarshal([]byte(raw), &t); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}
