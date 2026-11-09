// Package mailbox keeps what pocketd sends a chat session until a turn that
// showed it ends normally. It survives a pocketd restart.
package mailbox

import (
	"database/sql"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

const schema = `CREATE TABLE IF NOT EXISTS mail (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	agent TEXT NOT NULL,
	text TEXT NOT NULL,
	state TEXT NOT NULL,
	ref TEXT NOT NULL,
	created INTEGER NOT NULL
)`

const index = `CREATE INDEX IF NOT EXISTS mail_agent_state ON mail (agent, state)`

type Mail struct {
	ID      int64
	Agent   string
	Text    string
	State   string // queued | shown | acked
	Ref     string // Claude: the prompt's uuid. Codex: the turn id
	Created time.Time
}

type Store struct{ db *sql.DB }

// Open opens home/state/mail.db, creating it. Mail shown before goes back in
// the queue: the turn that showed it ended with the pocketd that sent it.
func Open(home string) (*Store, error) {
	path := filepath.Join(home, "state", "mail.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	// Another process may hold the file a moment, as the e2e tests do: wait, don't fail with SQLITE_BUSY.
	db, err := sql.Open("sqlite", path+"?_pragma=busy_timeout(5000)")
	if err != nil {
		return nil, err
	}
	// One writer at a time; SQLite answers a second with SQLITE_BUSY.
	db.SetMaxOpenConns(1)
	for _, q := range []string{schema, index, `UPDATE mail SET state = 'queued', ref = '' WHERE state = 'shown'`} {
		if _, err := db.Exec(q); err != nil {
			db.Close()
			return nil, err
		}
	}
	return &Store{db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

func (s *Store) Send(agent, text string) (int64, error) {
	r, err := s.db.Exec(`INSERT INTO mail (agent, text, state, ref, created) VALUES (?, ?, 'queued', '', ?)`, agent, text, time.Now().UnixMilli())
	if err != nil {
		return 0, err
	}
	return r.LastInsertId()
}

// Queued is agent's mail waiting to be shown, oldest first.
func (s *Store) Queued(agent string) ([]Mail, error) {
	rows, err := s.db.Query(`SELECT id, agent, text, state, ref, created FROM mail WHERE agent = ? AND state = 'queued' ORDER BY id`, agent)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Mail
	for rows.Next() {
		var m Mail
		var created int64
		if err := rows.Scan(&m.ID, &m.Agent, &m.Text, &m.State, &m.Ref, &created); err != nil {
			return nil, err
		}
		m.Created = time.UnixMilli(created)
		out = append(out, m)
	}
	return out, rows.Err()
}

// Shown marks mail id delivered under ref.
func (s *Store) Shown(id int64, ref string) error {
	_, err := s.db.Exec(`UPDATE mail SET state = 'shown', ref = ? WHERE id = ? AND state = 'queued'`, ref, id)
	return err
}

// Settle acks agent's mail shown under ref, or puts it back in the queue.
// Ref "" settles all of agent's shown mail, as when its session exits.
func (s *Store) Settle(agent, ref string, ok bool) error {
	q := `UPDATE mail SET state = 'queued', ref = '' WHERE agent = ? AND state = 'shown' AND (? = '' OR ref = ?)`
	if ok {
		q = `UPDATE mail SET state = 'acked' WHERE agent = ? AND state = 'shown' AND (? = '' OR ref = ?)`
	}
	_, err := s.db.Exec(q, agent, ref, ref)
	return err
}
