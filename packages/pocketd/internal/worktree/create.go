package worktree

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/registry"
)

type codedError struct{ code, message string }

func (e *codedError) Error() string { return e.message }

func coded(code, message string) error { return &codedError{code, message} }

var (
	ErrUnknownProject  = coded("unknown_project", "Unknown project")
	ErrUnknownWorktree = coded("unknown_worktree", "Not a worktree of this project")
	ErrExists          = coded("worktree_exists", "A worktree or branch with this name already exists")
	ErrInvalidName     = coded("invalid_name", "Use letters, digits, - _ or .")
	ErrMain            = coded("main_worktree", "The main worktree can't be removed")
)

// Code is err's wire code, or "" when it has none.
func Code(err error) string {
	var c *codedError
	if errors.As(err, &c) {
		return c.code
	}
	return ""
}

// Validate is the desktop's name rule (name_problem in pocket/src/modals/new_session.rs).
// Taken is checked first, case-insensitively as APFS is; a branch a/b owns a.
func Validate(name string, taken []string) error {
	for _, t := range taken {
		if first, _, _ := strings.Cut(t, "/"); strings.EqualFold(first, name) {
			return ErrExists
		}
	}
	safe := func(r rune) bool {
		return 'a' <= r && r <= 'z' || 'A' <= r && r <= 'Z' || '0' <= r && r <= '9' || strings.ContainsRune("-_.", r)
	}
	if name == "" || name == "HEAD" || strings.ContainsFunc(name, func(r rune) bool { return !safe(r) }) ||
		strings.HasPrefix(name, "-") || strings.HasPrefix(name, ".") || strings.HasSuffix(name, ".") ||
		strings.HasSuffix(name, ".lock") || strings.Contains(name, "..") {
		return ErrInvalidName
	}
	return nil
}

// Taken is every name a new Worktree of project can't have: its branches and
// the folders already in dir.
func Taken(project, dir string) ([]string, error) {
	taken, err := branches(project)
	if err != nil {
		return nil, err
	}
	entries, _ := os.ReadDir(dir)
	for _, e := range entries {
		taken = append(taken, e.Name())
	}
	return taken, nil
}

// branches is project's local branches, most recently committed first.
func branches(project string) ([]string, error) {
	out, err := exec.Command("git", "-C", project, "for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads").Output()
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return nil, errors.New(strings.TrimSpace(string(exit.Stderr)))
	}
	return strings.Fields(string(out)), err
}

// Dir is the folder project's new Worktrees go in.
func Dir(f registry.File, project string) string {
	if d := f.Repos[project].Worktrees; d != "" {
		return d
	}
	if r := f.Worktree.Root; r != "" {
		return filepath.Join(r, f.Name(project))
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".worktrees", f.Name(project))
}

// Containing is the registered Project whose folder or worktrees folder holds
// dir; the deepest wins.
func Containing(f registry.File, dir string) (string, bool) {
	best, depth := "", 0
	for _, p := range f.Projects {
		for _, root := range []string{p, Dir(f, p)} {
			if (dir == root || strings.HasPrefix(dir, root+"/")) && len(root) > depth {
				best, depth = p, len(root)
			}
		}
	}
	return best, depth > 0
}

type Created struct {
	Path, Branch, Base string
	Copied, Skipped    []string
}

// Create adds Worktree name on a new branch of the same name, then copies the
// copy set into it. Copy failures are Skipped, never fatal.
func Create(f registry.File, project, name, base string) (Created, error) {
	c, err := Add(f, project, name, base)
	if err != nil {
		return c, err
	}
	c.Copied, c.Skipped = CopyInto(f, project, c.Path)
	return c, nil
}

// Add is Create without the copy.
func Add(f registry.File, project, name, base string) (Created, error) {
	p, err := Prepare(f, project, name, base)
	if err != nil {
		return Created{}, err
	}
	return p.Add()
}

// Plan is a new Worktree whose name is free and valid, before git makes it.
type Plan struct {
	Project, Path, Branch, Base string
	// from is the commit the branch starts at: Base, or origin's Base once Fetch finds it newer.
	from string
}

// Prepare checks name against project's branches and folders and resolves base.
func Prepare(f registry.File, project, name, base string) (Plan, error) {
	if !slices.Contains(f.Projects, project) {
		return Plan{}, fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	if fi, err := os.Stat(project); err != nil || !fi.IsDir() {
		return Plan{}, fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	dir := Dir(f, project)
	taken, err := Taken(project, dir)
	if err != nil {
		return Plan{}, err
	}
	if err := Validate(name, taken); err != nil {
		return Plan{}, err
	}
	if base == "" {
		base = defaultBase(project, f.Repos[project].Base)
	}
	return Plan{Project: project, Path: filepath.Join(dir, name), Branch: name, Base: base, from: base}, nil
}

// Fetch updates origin's Base, so the Worktree starts from whichever of it and
// the local Base holds the other. git runs in env. The note says why it starts
// from the local Base anyway, when that isn't because the local Base is newest.
func (p *Plan) Fetch(env []string) (note string) {
	all, _ := branches(p.Project)
	if !slices.Contains(all, p.Base) {
		return ""
	}
	local := "using local " + p.Base
	remote := "origin/" + p.Base
	if exec.Command("git", "-C", p.Project, "remote", "get-url", "origin").Run() != nil {
		return "No remote, " + local
	}
	if exec.Command("git", "-C", p.Project, "rev-parse", "-q", "--verify", "refs/remotes/"+remote).Run() != nil {
		return "No " + remote + ", " + local
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	fetch := exec.CommandContext(ctx, "git", "-C", p.Project, "fetch", "-q", "--no-tags", "origin", "+refs/heads/"+p.Base+":refs/remotes/origin/"+p.Base)
	fetch.Env = append(slices.Clip(env), "GIT_TERMINAL_PROMPT=0")
	if fetch.Run() != nil {
		return "Couldn't fetch, " + local
	}
	switch {
	case ancestor(p.Project, remote, p.Base):
		return ""
	case ancestor(p.Project, p.Base, remote):
		p.from = remote
		return ""
	}
	return p.Base + " has diverged from origin, " + local
}

func ancestor(project, a, b string) bool {
	return exec.Command("git", "-C", project, "merge-base", "--is-ancestor", a, b).Run() == nil
}

// Add makes the Worktree on its new branch. The branch tracks nothing, even
// when it starts from origin. Its Path is the real one, as List reports it.
func (p Plan) Add() (Created, error) {
	if out, err := exec.Command("git", "-C", p.Project, "worktree", "add", "-q", "--no-track", "-b", p.Branch, "--", p.Path, p.from).CombinedOutput(); err != nil {
		return Created{}, errors.New(strings.TrimSpace(string(out)))
	}
	path := p.Path
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	return Created{Path: path, Branch: p.Branch, Base: p.Base}, nil
}

// defaultBase is the desktop's pick (default_base in pocket/src/modals/form.rs):
// the repo's base, main, master, the current branch, the newest branch, else
// HEAD when the repo has no branch at all.
func defaultBase(project, preferred string) string {
	all, _ := branches(project)
	current, _ := exec.Command("git", "-C", project, "branch", "--show-current").Output()
	for _, b := range []string{preferred, "main", "master", strings.TrimSpace(string(current))} {
		if b != "" && slices.Contains(all, b) {
			return b
		}
	}
	if len(all) > 0 {
		return all[0]
	}
	return "HEAD"
}

// Remove deletes a linked Worktree's folder, uncommitted changes included, as
// the desktop does; its branch stays.
func Remove(f registry.File, project, path string) error {
	if !slices.Contains(f.Projects, project) {
		return fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	ws, err := List(project)
	if err != nil {
		return fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	abs, _ := filepath.Abs(path)
	if real, err := filepath.EvalSymlinks(abs); err == nil {
		abs = real
	}
	i := slices.IndexFunc(ws, func(w Worktree) bool { return w.Path == abs })
	switch {
	case i < 0:
		return fmt.Errorf("%w: %s", ErrUnknownWorktree, path)
	case ws[i].Main:
		return ErrMain
	}
	if out, err := exec.Command("git", "-C", project, "worktree", "remove", "--force", abs).CombinedOutput(); err != nil {
		return errors.New(strings.TrimSpace(string(out)))
	}
	return nil
}
