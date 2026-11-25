package main

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/names"
	"pocketd/internal/orch"
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

// claimAgentWorktrees fills the registry's agent worktrees the first time a pocketd that keeps them starts in home,
// from the worktree delegations' children the store keeps and the subagents' folders desktop.json remembers, then
// cleans up each whose sessions all closed. Never again: every agent worktree since is recorded as it's made.
func claimAgentWorktrees(home string, reg *registry.Registry, isolated []orch.Session, reap func(tree string)) {
	done := filepath.Join(home, "state", "agent-worktrees-claimed")
	if _, err := os.Stat(done); err == nil {
		return
	}
	uses := subtreeUses(filepath.Join(home, "desktop.json"))
	for _, x := range isolated {
		uses = append(uses, worktree.Use{Cwd: x.Cwd, Started: x.CreatedAt})
	}
	trees := worktree.AgentMade(reg.Load(), uses)
	err := reg.AgentWorktrees(func(paths []string) []string {
		for _, t := range trees {
			if !slices.Contains(paths, t) {
				paths = append(paths, t)
			}
		}
		return paths
	})
	if err != nil {
		log.Printf("worktrees: %v", err)
		return
	}
	log.Printf("worktrees: %d made for agents' sessions", len(trees))
	for _, t := range trees {
		reap(t)
	}
	err = os.MkdirAll(filepath.Dir(done), 0o700)
	if err == nil {
		err = os.WriteFile(done, nil, 0o600)
	}
	if err != nil {
		log.Printf("worktrees: %v", err)
	}
}

// subtreeUses is the subagents' folders on this Mac that desktop.json's subtrees remember, with when each started.
func subtreeUses(path string) []worktree.Use {
	var d struct {
		Subtrees map[string]struct {
			Started int64 `json:"started"`
		} `json:"subtrees"`
	}
	raw, err := os.ReadFile(path)
	if err == nil {
		err = json.Unmarshal(raw, &d)
	}
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		log.Printf("worktrees: %s: %v", path, err)
	}
	var uses []worktree.Use
	for folder, s := range d.Subtrees {
		if !strings.HasPrefix(folder, "//") {
			uses = append(uses, worktree.Use{Cwd: folder, Started: s.Started})
		}
	}
	return uses
}
