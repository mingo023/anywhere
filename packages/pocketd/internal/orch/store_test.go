package orch

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/proto"
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

func seedItems(t *testing.T, s *Store, n int) {
	t.Helper()
	execAll(t, s,
		`WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < `+itoa(int64(n))+`)
		INSERT INTO items (session_id, item_id, seq, epoch, kind, text, body, ts) SELECT 's' || (i % 80), 'i' || i, i, 1, 'assistant', 'words about the code ' || i, zeroblob(3000), i FROM k`,
		`INSERT INTO items_fts (rowid, session_id, item_id, text) SELECT rowid, session_id, item_id, text FROM items`,
	)
}

func writeItems(s *Store) time.Duration {
	k := NewSink(s)
	defer k.Close()
	start := time.Now()
	for i := range 200 {
		k.Item("live", 1, proto.Item{ID: fmt.Sprintf("n%d", i%20), Seq: int64(i), Kind: "assistant", Text: fmt.Sprintf("streaming %d", i)})
	}
	k.Flush()
	return time.Since(start)
}

func TestAnItemWriteCostsNoMoreInABigStore(t *testing.T) {
	small := writeItems(openTest(t))
	s := openTest(t)
	seedItems(t, s, 20000)
	if big := writeItems(s); big > 4*small+100*time.Millisecond {
		t.Fatalf("200 item writes took %v beside 20000 items, %v in an empty store", big, small)
	}
}

func TestAStoreFromBeforeFindsARewrittenItemOnce(t *testing.T) {
	s := openTest(t)
	execAll(t, s,
		`DROP TABLE items_fts`,
		`CREATE VIRTUAL TABLE items_fts USING fts5 (session_id UNINDEXED, item_id UNINDEXED, text)`,
		`DELETE FROM schema_version WHERE version > 5`,
		`INSERT INTO items (session_id, item_id, seq, epoch, kind, text, body, ts) VALUES ('a', 'i1', 1, 1, 'user', 'first draft', '{}', 1), ('a', 'i2', 2, 1, 'user', 'other words', '{}', 2)`,
		`INSERT INTO items_fts (rowid, session_id, item_id, text) VALUES (2, 'a', 'i1', 'first draft'), (1, 'a', 'i2', 'other words')`,
	)
	if err := s.migrate(); err != nil {
		t.Fatal(err)
	}
	k := NewSink(s)
	k.Item("a", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "second draft"})
	k.Close()
	for q, want := range map[string]int{"first": 0, "second": 1, "draft": 1, "other": 1} {
		if n := countRows(s, `SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH '`+q+`'`); n != want {
			t.Errorf("%q finds %d, want %d", q, n, want)
		}
	}
}

func TestPruneDropsTheSearchRowsOfASessionItDrops(t *testing.T) {
	s := openTest(t)
	day := int64(24 * time.Hour / time.Millisecond)
	k := NewSink(s)
	k.Item("gone", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "gone words"})
	k.Item("kept", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "kept words"})
	k.Close()
	execAll(t, s, `UPDATE sessions SET closed_at = `+itoa(time.Now().UnixMilli()-16*day)+` WHERE id = 'gone'`)
	if err := s.Prune(time.Now()); err != nil {
		t.Fatal(err)
	}
	if n := countRows(s, `SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'words'`); n != 1 {
		t.Fatalf("search rows = %d, want only the kept session's", n)
	}
}

func TestTheWALShrinksBackAfterABigWrite(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	execAll(t, s, `WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 12000)
		INSERT INTO items (session_id, item_id, seq, epoch, kind, body, ts) SELECT 'a', 'i' || i, i, 1, 'assistant', randomblob(3000), i FROM k`)
	execAll(t, s, `INSERT INTO items (session_id, item_id, seq, epoch, kind, body, ts) VALUES ('a', 'last', 0, 1, 'user', '{}', 0)`)
	st, err := os.Stat(filepath.Join(dir, "state", "orchestrator.db-wal"))
	if err != nil {
		t.Fatal(err)
	}
	if st.Size() > walKept {
		t.Fatalf("WAL is %d bytes after the write that followed a big one", st.Size())
	}
}

func TestAStoreFromBeforeKeepsTheNewestSearchRowOfAnItemWrittenTwice(t *testing.T) {
	s := openTest(t)
	execAll(t, s,
		`DROP TABLE items_fts`,
		`CREATE VIRTUAL TABLE items_fts USING fts5 (session_id UNINDEXED, item_id UNINDEXED, text)`,
		`DELETE FROM schema_version WHERE version > 5`,
		`INSERT INTO items (session_id, item_id, seq, epoch, kind, text, body, ts) VALUES ('a', 'i1', 1, 1, 'user', 'newer', '{}', 1)`,
		`INSERT INTO items_fts (rowid, session_id, item_id, text) VALUES (1, 'a', 'i1', 'older'), (2, 'a', 'i1', 'newer')`,
	)
	if err := s.migrate(); err != nil {
		t.Fatal(err)
	}
	if n := countRows(s, `SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'newer'`); n != 1 {
		t.Fatalf("newer finds %d", n)
	}
}
