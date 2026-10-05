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

// Open is an existing branch about to be checked out in a new Worktree, or
// the Worktree already on it.
type Open struct {
	Project, Path, Branch string
	// Adopt is a Worktree already on Branch; nothing is added.
	Adopt string
	Note  string
	// created: this Open made Branch at oid, so a failed Add deletes it.
	created bool
	oid     string
	env     []string
}

// Folder is branch with / as -, then -2, -3… until no folder has it.
func Folder(branch string, folders []string) string {
	base := strings.ReplaceAll(branch, "/", "-")
	name := base
	for n := 2; slices.ContainsFunc(folders, func(f string) bool { return strings.EqualFold(f, name) }); n++ {
		name = fmt.Sprintf("%s-%d", base, n)
	}
	return name
}

// PrepareBranch readies branch, local or on origin, for a new Worktree named
// name, or after branch when name is empty.
func PrepareBranch(f registry.File, project, name, branch string, env []string) (*Open, error) {
	o, err := prepare(f, project, name, branch, env)
	if err != nil || o.adopt() {
		return o, err
	}
	local := o.at("refs/heads/" + branch)
	remote := ""
	_, fetchErr := o.git("fetch", "-q", "--no-tags", "origin", "+refs/heads/"+branch+":refs/remotes/origin/"+branch)
	if fetchErr == nil {
		remote = o.at("refs/remotes/origin/" + branch)
	}
	switch {
	case local == "" && remote == "":
		return nil, o.missing(fetchErr)
	case local == "":
		if _, err := o.git("branch", "--track", branch, "refs/remotes/origin/"+branch); err != nil {
			return nil, err
		}
		o.created, o.oid = true, remote
	case remote == "" || local == remote:
	case ancestor(project, local, remote):
		if _, err := o.git("update-ref", "refs/heads/"+branch, remote, local); err != nil {
			return nil, err
		}
		o.note("Fast-forwarded " + branch)
	default:
		o.note(branch + " differs from origin/" + branch)
	}
	return o, nil
}

func prepare(f registry.File, project, name, branch string, env []string) (*Open, error) {
	if !slices.Contains(f.Projects, project) {
		return nil, fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	o := &Open{Project: project, Branch: branch, env: env}
	// A Worktree whose folder was deleted still holds its branch until pruned.
	if _, err := o.git("worktree", "prune"); err != nil {
		return nil, err
	}
	if strings.HasPrefix(branch, "-") || exec.Command("git", "check-ref-format", "refs/heads/"+branch).Run() != nil {
		return nil, failed("No branch %s", branch)
	}
	dir := Dir(f, project)
	var folders []string
	entries, _ := os.ReadDir(dir)
	for _, e := range entries {
		folders = append(folders, e.Name())
	}
	if name == "" {
		name = Folder(branch, folders)
	} else if err := Validate(name, folders); err != nil {
		return nil, err
	}
	o.Path = filepath.Join(dir, name)
	return o, nil
}

// adopt takes the Worktree already on o.Branch, the main one included.
func (o *Open) adopt() bool {
	ws, _ := List(o.Project)
	i := slices.IndexFunc(ws, func(w Worktree) bool { return w.Branch == o.Branch })
	if i < 0 {
		return false
	}
	o.Adopt, o.Path = ws[i].Path, ws[i].Path
	o.note("Already open in " + ws[i].Name)
	return true
}

// Add checks Branch out in a new Worktree at Path, the real path as List
// reports it. On failure a branch this Open made is deleted.
func (o *Open) Add() (Created, error) {
	if _, err := o.git("worktree", "add", "-q", "--", o.Path, o.Branch); err != nil {
		o.rollback()
		return Created{}, err
	}
	path := o.Path
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	return Created{Path: path, Branch: o.Branch}, nil
}

// rollback deletes Branch if this Open made it and nothing moved it since.
func (o *Open) rollback() {
	if o.created && o.at("refs/heads/"+o.Branch) == o.oid {
		o.git("branch", "-D", o.Branch)
	}
}

// missing says why Branch is neither local nor fetched: no such branch, or
// an origin that couldn't be reached.
func (o *Open) missing(fetchErr error) error {
	if fetchErr == nil {
		return failed("No branch %s", o.Branch)
	}
	if _, err := o.git("remote", "get-url", "origin"); err != nil {
		return failed("No branch %s", o.Branch)
	}
	// fetch also fails when origin lacks the branch; ls-remote tells that
	// apart without reading git's localized stderr.
	if out, err := o.git("ls-remote", "origin", "refs/heads/"+o.Branch); err == nil && out == "" {
		return failed("No branch %s", o.Branch)
	}
	line, _, _ := strings.Cut(fetchErr.Error(), "\n")
	if line = strings.TrimPrefix(line, "fatal: "); line == "" {
		return failed("Couldn't fetch %s from origin", o.Branch)
	}
	return failed("Couldn't fetch %s from origin: %s", o.Branch, line)
}

func (o *Open) note(s string) {
	if o.Note != "" {
		o.Note += "; "
	}
	o.Note += s
}

// at is ref's commit, or "" when there is no ref.
func (o *Open) at(ref string) string {
	sha, _ := o.git("rev-parse", "-q", "--verify", ref+"^{commit}")
	return sha
}

// git runs in o.Project with the login env, never prompts, and gives up
// after 15s, as Plan.Fetch does.
func (o *Open) git(args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "git", append([]string{"-C", o.Project}, args...)...)
	cmd.Env = append(slices.Clip(o.env), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.Output()
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return "", errors.New(strings.TrimSpace(string(exit.Stderr)))
	}
	return strings.TrimSpace(string(out)), err
}

// failed is a failure worded for the person opening, sent as spawn_failed.
func failed(format string, a ...any) error {
	return coded("spawn_failed", fmt.Sprintf(format, a...))
}
