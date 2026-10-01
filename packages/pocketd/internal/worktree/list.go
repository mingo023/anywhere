// Package worktree lists, creates and removes a Project's Worktrees.
package worktree

import (
	"cmp"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"syscall"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

type Worktree struct {
	Name, Path, Branch string
	Main               bool
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

func born(path string) int64 {
	var st syscall.Stat_t
	if syscall.Stat(path, &st) != nil {
		return 0
	}
	return st.Birthtimespec.Nano()
}

// Projects is the registry as clients see it. A folder that is gone is left out.
func Projects(f registry.File) []proto.Project {
	ps := []proto.Project{}
	for _, p := range f.Projects {
		ws, err := List(p)
		if err != nil {
			continue
		}
		pp := proto.Project{Path: p, Name: f.Name(p), Worktrees: make([]proto.Worktree, len(ws))}
		for i, w := range ws {
			pp.Worktrees[i] = proto.Worktree{Name: w.Name, Path: w.Path, Branch: w.Branch, IsMain: w.Main}
		}
		ps = append(ps, pp)
	}
	return ps
}
