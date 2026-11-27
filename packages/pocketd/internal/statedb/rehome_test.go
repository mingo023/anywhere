package statedb

import (
	"slices"
	"testing"

	"pocketd/internal/config"
)

func TestRehomeMovesEveryPathUnderTheOldHomeAndNothingElse(t *testing.T) {
	db, err := Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	for _, q := range []string{
		`INSERT INTO projects (id, path, position) VALUES ('a', '/Users/mình/.anywhere/sessions/calm', 0), ('b', '/Users/mình/.anywhere-x/app', 1), ('c', '/w/app', 2)`,
		`INSERT INTO worktrees (path, kind, position) VALUES ('/Users/mình/.anywhere/worktrees/app/x', 'tracked', 0), ('/Users/mình/.anywhere', 'agent', 1)`,
		`INSERT INTO names (path, title) VALUES ('/Users/mình/.anywhere/worktrees/app/y', 'old'), ('/Users/mình/.zoes/worktrees/app/y', 'new')`,
		`INSERT INTO shared_settings (id, worktree_root) VALUES (1, '/Users/mình/.anywhere/worktrees')`,
		`INSERT INTO tickets (id, project_id, tree, title, prompt, provider, state, worktree, position) VALUES ('t', 'c', '/Users/mình/.anywhere/worktrees/app/x', 't', '/Users/mình/.anywhere/notes.md says hi', 'claude', 'open', '/Users/mình/.anywhere/worktrees/app/x', 0)`,
	} {
		if _, err := db.Exec(q); err != nil {
			t.Fatal(err)
		}
	}
	moves := []config.Move{{From: "/Users/mình/.anywhere", To: "/Users/mình/.zoes"}}
	for range 2 {
		if err := Rehome(db, moves); err != nil {
			t.Fatal(err)
		}
	}
	for q, want := range map[string][]string{
		`SELECT path FROM projects ORDER BY position`:                  {"/Users/mình/.zoes/sessions/calm", "/Users/mình/.anywhere-x/app", "/w/app"},
		`SELECT path FROM worktrees ORDER BY position`:                 {"/Users/mình/.zoes/worktrees/app/x", "/Users/mình/.zoes"},
		`SELECT path || '=' || title FROM names ORDER BY title`:        {"/Users/mình/.zoes/worktrees/app/y=new", "/Users/mình/.anywhere/worktrees/app/y=old"},
		`SELECT worktree_root FROM shared_settings`:                    {"/Users/mình/.zoes/worktrees"},
		`SELECT tree || '|' || worktree || '|' || prompt FROM tickets`: {"/Users/mình/.zoes/worktrees/app/x|/Users/mình/.zoes/worktrees/app/x|/Users/mình/.anywhere/notes.md says hi"},
	} {
		rows, err := db.Query(q)
		if err != nil {
			t.Fatal(err)
		}
		var got []string
		for rows.Next() {
			var s string
			rows.Scan(&s)
			got = append(got, s)
		}
		rows.Close()
		if !slices.Equal(got, want) {
			t.Errorf("%s: %q", q, got)
		}
	}
}
