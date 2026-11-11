// Package worktree lists, creates and removes a Project's Worktrees.
package worktree

import (
	"cmp"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

type Worktree struct {
	Name, Path, Branch string
	Main               bool
	// Made: Anywhere made it, whatever folder it is in.
	Made bool
}

// Marker is the file Anywhere leaves in the git folder of a Worktree it makes,
// which git deletes with the Worktree. The desktop reads it too (git::worktrees in crates/git).
const Marker = "anywhere"

// markerPath is where tree's Marker goes; "" when tree isn't a linked worktree.
func markerPath(tree string) string {
	raw, _ := os.ReadFile(filepath.Join(tree, ".git"))
	dir, linked := strings.CutPrefix(strings.TrimSpace(string(raw)), "gitdir: ")
	if !linked {
		return ""
	}
	if !filepath.IsAbs(dir) {
		dir = filepath.Join(tree, dir)
	}
	return filepath.Join(dir, Marker)
}

func mark(tree string) {
	if err := os.WriteFile(markerPath(tree), nil, 0o644); err != nil {
		log.Printf("worktree: marking %s: %v", tree, err)
	}
}

func marked(tree string) bool {
	_, err := os.Stat(markerPath(tree))
	return err == nil
}

// Claim marks the Worktrees in a project's worktrees folder that hold one of
// used, the folders Anywhere has on record: the ones it made before it marked
// them, which only their folder told from the ones made there by hand.
func Claim(f registry.File, used []string) {
	for _, p := range f.Projects {
		ws, _ := List(p)
		dir := resolved(Dir(f, p)) + "/"
		for _, w := range ws {
			holds := func(u string) bool { return u == w.Path || strings.HasPrefix(u, w.Path+"/") }
			if !w.Main && !w.Made && strings.HasPrefix(resolved(w.Path), dir) && slices.ContainsFunc(used, holds) {
				mark(w.Path)
			}
		}
	}
}

// List is project's checkouts: the main one, then the rest oldest folder
// first, as the desktop shows them (git::worktrees in crates/git). A folder that
// isn't a repo is its own main Worktree.
func List(project string) ([]Worktree, error) {
	if _, err := os.Stat(project); err != nil {
		return nil, err
	}
	out, err := exec.Command("git", "-C", project, "worktree", "list", "--porcelain").Output()
	if err != nil {
		return []Worktree{{Name: filepath.Base(project), Path: project, Main: true}}, nil
	}
	ws := parse(string(out))
	if len(ws) > 1 {
		for i := 1; i < len(ws); i++ {
			ws[i].Made = marked(ws[i].Path)
		}
		slices.SortStableFunc(ws[1:], func(a, b Worktree) int { return cmp.Compare(born(a.Path), born(b.Path)) })
	}
	return ws, nil
}

func parse(porcelain string) []Worktree {
	var ws []Worktree
	for _, block := range strings.Split(strings.TrimSpace(porcelain), "\n\n") {
		var w Worktree
		for _, l := range strings.Split(block, "\n") {
			if p, ok := strings.CutPrefix(l, "worktree "); ok {
				w.Path, w.Name = p, filepath.Base(p)
			} else if b, ok := strings.CutPrefix(l, "branch "); ok {
				w.Branch = strings.TrimPrefix(b, "refs/heads/")
			}
		}
		if w.Path != "" {
			ws = append(ws, w)
		}
	}
	if len(ws) > 0 {
		ws[0].Main = true
	}
	return ws
}

// Projects is the registry as clients see it. A folder that is gone is left out.
func Projects(f registry.File) []proto.Project {
	ps := []proto.Project{}
	for _, p := range f.Projects {
		ws, err := List(p)
		if err != nil {
			continue
		}
		ps = append(ps, proto.Project{Path: p, Name: f.Name(p), Worktrees: Listed(f, ws)})
	}
	return ps
}

// Listed is the worktrees of a project the desktop's sidebar lists (`project::listed` over `Store::lists`): the
// main one, those Anywhere made, those the desktop tracks, and all while it lists those made elsewhere.
func Listed(f registry.File, ws []Worktree) []proto.Worktree {
	out := []proto.Worktree{}
	for _, w := range ws {
		if w.Main || w.Made || f.Sidebar.ExternalWorktrees || slices.Contains(f.Tracked, w.Path) {
			out = append(out, proto.Worktree{Name: w.Name, Path: w.Path, Branch: w.Branch, IsMain: w.Main})
		}
	}
	return out
}
