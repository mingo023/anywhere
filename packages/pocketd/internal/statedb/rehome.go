package statedb

import (
	"database/sql"
	"fmt"
	"unicode/utf8"

	"pocketd/internal/config"
)

// Rehome points each path stored under a moved home at the same path under where it went. A path whose new one
// another row already holds keeps the old one, which still leads there.
func Rehome(db *sql.DB, moves []config.Move) error {
	if len(moves) == 0 {
		return nil
	}
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for _, c := range pathColumns {
		q := fmt.Sprintf(`UPDATE OR IGNORE %[1]q SET %[2]q = ? || substr(%[2]q, ?) WHERE %[2]q = ? OR substr(%[2]q, 1, ?) = ?`, c[0], c[1])
		for _, m := range moves {
			n := utf8.RuneCountInString(m.From)
			if _, err := tx.Exec(q, m.To, n+1, m.From, n+1, m.From+"/"); err != nil {
				return fmt.Errorf("%s.%s: %w", c[0], c[1], err)
			}
		}
	}
	return tx.Commit()
}

// pathColumns holds every column of paths. Others hold text a user wrote, where a path means what it said.
var pathColumns = [][2]string{
	{"projects", "path"}, {"worktrees", "path"}, {"names", "path"}, {"shared_settings", "worktree_root"},
	{"tickets", "tree"}, {"tickets", "worktree"}, {"automations", "tree"}, {"runs", "worktree"},
}
