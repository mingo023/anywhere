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
	"time"

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

// MadeAt is when Anywhere made the Worktree at tree, in Unix milliseconds; 0
// when it didn't.
func MadeAt(tree string) int64 {
	info, err := os.Stat(markerPath(tree))
	if err != nil {
		return 0
	}
	return info.ModTime().UnixMilli()
}

// InMade is whether path is in a Worktree Anywhere made that's still there.
func InMade(path string) bool {
	for d := filepath.Clean(path); ; d = filepath.Dir(d) {
		if MadeAt(d) > 0 {
			return true
		}
		if d == filepath.Dir(d) {
			return false
		}
	}
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
		dir := resolved(Dir(f, p)) + string(filepath.Separator)
		for _, w := range ws {
			holds := func(u string) bool { return u == w.Path || strings.HasPrefix(u, w.Path+string(filepath.Separator)) }
			if !w.Main && !w.Made && strings.HasPrefix(resolved(w.Path), dir) && slices.ContainsFunc(used, holds) {
				claim(w.Path)
			}
		}
	}
}

// claim marks tree dated when git made it, so MadeAt still tells it from a
// Worktree made at its folder since.
func claim(tree string) {
	mark(tree)
	made, err := os.Stat(filepath.Join(filepath.Dir(markerPath(tree)), "commondir"))
	if err != nil {
		return
	}
	if err := os.Chtimes(markerPath(tree), made.ModTime(), made.ModTime()); err != nil {
		log.Printf("worktree: dating %s: %v", tree, err)
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
				w.Path = filepath.FromSlash(p)
				w.Name = filepath.Base(w.Path)
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
		ps = append(ps, proto.Project{ID: f.Repos[p].ID, Path: p, Name: f.Name(p), Worktrees: Listed(f, ws)})
	}
	return ps
}

// Listed is the worktrees of a project the sidebar lists: the main one, those a person made in Anywhere, and those
// tracked. One pocketd made for an agent's session shows only once tracked.
func Listed(f registry.File, ws []Worktree) []proto.Worktree {
	out := []proto.Worktree{}
	for _, w := range ws {
		tracked := slices.Contains(f.Tracked, w.Path)
		if w.Main || w.Made && !slices.Contains(f.AgentWorktrees, w.Path) || tracked {
			out = append(out, proto.Worktree{Name: w.Name, Path: w.Path, Branch: w.Branch, IsMain: w.Main, CreatedAt: born(w.Path) / 1e6})
		}
	}
	return out
}

// Use is a folder a session an agent started worked in, and when that session started, in Unix milliseconds.
type Use struct {
	Cwd     string
	Started int64
}

// madeFor bounds how long before a session started its worktree was made: the setup runs in between.
const madeFor = 15 * time.Minute

// AgentMade is the worktrees of f's projects that Anywhere made for the sessions that used them: each a linked one
// Anywhere marked, not tracked, whose folder a use is in and that was made in the madeFor before that session
// started. One made long before, as a person's worktree a session was sent into, or since, isn't theirs.
func AgentMade(f registry.File, uses []Use) []string {
	var out []string
	listed := map[string][]Worktree{}
	for _, u := range uses {
		project, ok := Project(f, u.Cwd)
		if !ok {
			continue
		}
		if _, ok := listed[project]; !ok {
			listed[project], _ = List(project)
		}
		cwd := resolved(u.Cwd)
		for _, w := range listed[project] {
			at := MadeAt(w.Path)
			theirs := cwd == w.Path || strings.HasPrefix(cwd, w.Path+string(filepath.Separator))
			if theirs && !w.Main && w.Made && at <= u.Started && at >= u.Started-madeFor.Milliseconds() &&
				!slices.Contains(f.Tracked, w.Path) && !slices.Contains(out, w.Path) {
				out = append(out, w.Path)
			}
		}
	}
	return out
}
