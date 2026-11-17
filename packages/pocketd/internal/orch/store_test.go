package orch

import (
	"fmt"
	"testing"
	"time"
)

func TestPruneDropsEventsOlderThanADayButKeepsEachSessionsNewest(t *testing.T) {
	s := openTest(t)
	now := time.Now()
	ms := now.UnixMilli()
	hour := int64(time.Hour / time.Millisecond)
	execAll(t, s,
		`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', `+itoa(ms-26*hour)+`)`,
		`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', `+itoa(ms-25*hour)+`)`,
		`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', `+itoa(ms-23*hour)+`)`,
		`INSERT INTO events (session_id, kind, payload, at) VALUES ('b', 'x', '{}', `+itoa(ms-30*hour)+`)`,
	)
	if err := s.Prune(now); err != nil {
		t.Fatal(err)
	}
	if n := countRows(s, `SELECT COUNT(*) FROM events WHERE session_id = 'a'`); n != 1 {
		t.Errorf("events of a = %d, want only the one under a day old", n)
	}
	if n := countRows(s, `SELECT COUNT(*) FROM events WHERE session_id = 'b'`); n != 1 {
		t.Errorf("events of b = %d, want its newest kept", n)
	}
}

func TestPruneDropsReceiptsOlderThanAWeek(t *testing.T) {
	s := openTest(t)
	now := time.Now()
	ms := now.UnixMilli()
	day := int64(24 * time.Hour / time.Millisecond)
	execAll(t, s,
		`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES ('old', 'h', '{}', `+itoa(ms-7*day-1000)+`)`,
		`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES ('new', 'h', '{}', `+itoa(ms-7*day+60000)+`)`,
	)
	if err := s.Prune(now); err != nil {
		t.Fatal(err)
	}
	if n := countRows(s, `SELECT COUNT(*) FROM receipts WHERE command_id = 'new'`); n != 1 {
		t.Errorf("a receipt just under a week old was dropped")
	}
	if n := countRows(s, `SELECT COUNT(*) FROM receipts WHERE command_id = 'old'`); n != 0 {
		t.Errorf("a receipt just over a week old was kept")
	}
}

func TestPruneDropsWhatAClosedSessionOwnsFifteenDaysAfterItClosed(t *testing.T) {
	day := int64(24 * time.Hour / time.Millisecond)
	for _, c := range []struct{ table, owner, insert string }{
		{"turns", "session_id", `INSERT INTO turns (session_id, state, started_at) VALUES ('%[1]s', 'completed', 1)`},
		{"queue", "session_id", `INSERT INTO queue (id, session_id, text, ord, created_at) VALUES ('q%[1]s', '%[1]s', 'hi', 1, 1)`},
		{"requests", "session_id", `INSERT INTO requests (id, session_id, kind, payload, created_at) VALUES ('r%[1]s', '%[1]s', 'question', '{}', 1)`},
		{"delegations", "parent_id", `INSERT INTO delegations (task_id, parent_id, child_id, created_at) VALUES ('t%[1]s', '%[1]s', 'other', 1)`},
		{"transfers", "source_id", `INSERT INTO transfers (id, source_id, target_id, kind, state, created_at) VALUES ('x%[1]s', '%[1]s', 'other', 'fork', 'done', 1)`},
		{"tokens", "session_id", `INSERT INTO tokens (hash, session_id, agent, issued_at, last_alive) VALUES ('h%[1]s', '%[1]s', 'claude', 1, 1)`},
		{"held", "session_id", `INSERT INTO held (session_id, task_id, until_at) VALUES ('%[1]s', 'task', 1)`},
	} {
		t.Run(c.table, func(t *testing.T) {
			s := openTest(t)
			now := time.Now()
			ms := now.UnixMilli()
			execAll(t, s,
				`INSERT INTO sessions (id, closed_at) VALUES ('gone', `+itoa(ms-16*day)+`)`,
				`INSERT INTO sessions (id, closed_at) VALUES ('kept', `+itoa(ms-14*day)+`)`,
				`INSERT INTO sessions (id, closed_at) VALUES ('open', 0)`,
				fmt.Sprintf(c.insert, "gone"), fmt.Sprintf(c.insert, "kept"), fmt.Sprintf(c.insert, "open"),
			)
			if err := s.Prune(now); err != nil {
				t.Fatal(err)
			}
			if n := countRows(s, `SELECT COUNT(*) FROM `+c.table); n != 2 {
				t.Errorf("%s rows = %d, want the two of the sessions closed under 15 days ago or open", c.table, n)
			}
			if n := countRows(s, `SELECT COUNT(*) FROM `+c.table+` WHERE `+c.owner+` = 'gone'`); n != 0 {
				t.Errorf("%s rows of a session closed 16 days ago = %d", c.table, n)
			}
		})
	}
}

func execAll(t *testing.T, s *Store, stmts ...string) {
	t.Helper()
	if err := s.Tx(func(tx *Tx) error {
		for _, q := range stmts {
			if _, err := tx.Exec(q); err != nil {
				return fmt.Errorf("%s: %w", q, err)
			}
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
}

func countRows(s *Store, q string) int {
	var n int
	s.db.QueryRow(q).Scan(&n)
	return n
}
