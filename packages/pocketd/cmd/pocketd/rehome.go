package main

import (
	"database/sql"
	"errors"
	"os"
	"path/filepath"

	"pocketd/internal/claude"
	"pocketd/internal/config"
	"pocketd/internal/statedb"
	"pocketd/internal/worktree"
)

// rehome points what still names a moved home at where it went: pocketd.db's paths, the worktrees in the shared
// home, and Claude's transcript dirs, named by cwd. Every step finds its own work, so each start retries what failed.
func rehome(db *sql.DB, moves []config.Move) error {
	moves = withRealPaths(moves)
	errs := []error{statedb.Rehome(db, moves)}
	user, _ := os.UserHomeDir()
	accounts, _ := filepath.Glob(filepath.Join(config.AccountsDir(), "claude", "*", "projects"))
	projects := append([]string{filepath.Join(user, ".claude", "projects")}, accounts...)
	if dir := os.Getenv("CLAUDE_CONFIG_DIR"); dir != "" {
		projects = append(projects, filepath.Join(dir, "projects"))
	}
	for _, m := range moves {
		errs = append(errs, worktree.RepairMoved(filepath.Join(config.SharedHome(), "worktrees"), m.From))
		for _, p := range projects {
			errs = append(errs, claude.LinkMoved(p, m.From, m.To))
		}
	}
	return errors.Join(errs...)
}

// withRealPaths adds each move as seen through the user folder's real path, where that differs: git and agents
// record a worktree by its real path.
func withRealPaths(moves []config.Move) []config.Move {
	out := moves
	for _, m := range moves {
		dir := filepath.Dir(m.From)
		real, err := filepath.EvalSymlinks(dir)
		rel, rerr := filepath.Rel(dir, m.To)
		if err == nil && rerr == nil && real != dir {
			out = append(out, config.Move{From: filepath.Join(real, filepath.Base(m.From)), To: filepath.Join(real, rel)})
		}
	}
	return out
}
