package main

import (
	"log"
	"os"
	"path/filepath"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/names"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// usedFolders is every folder Anywhere has on record: the Worktrees it named,
// the ones its tickets and automation runs made, and where its Terminals run.
func usedFolders(named *names.Names, terminals *terminal.Manager, tickets *board.Store, autos *automation.Store) []string {
	var used []string
	for tree := range named.Titles() {
		used = append(used, tree)
	}
	for _, t := range terminals.List() {
		used = append(used, t.Cwd)
	}
	ts, _ := tickets.Snapshot()
	for _, t := range ts {
		used = append(used, t.Worktree)
	}
	_, runs := autos.Snapshot()
	for _, r := range runs {
		used = append(used, r.Worktree)
	}
	return used
}

// claimWorktrees runs worktree.Claim the first time a pocketd that marks its
// Worktrees starts in home. Never again: a Worktree in a worktrees folder that
// has no mark after that was made by hand.
func claimWorktrees(home string, f registry.File, used []string) {
	done := filepath.Join(home, "state", "worktrees-claimed")
	if _, err := os.Stat(done); err == nil {
		return
	}
	worktree.Claim(f, used)
	err := os.MkdirAll(filepath.Dir(done), 0o700)
	if err == nil {
		err = os.WriteFile(done, nil, 0o600)
	}
	if err != nil {
		log.Printf("worktrees: %v", err)
	}
}
