package worktree

import (
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/registry"
)

// Location is the checkout a folder sits in. Common is the repo's shared .git,
// the same for every Worktree of one repo.
type Location struct {
	Root, Common, Branch string
	Main                 bool
}

// Locate finds the checkout holding cwd from files alone: it runs on every
// agent start and turn end, where a git process per call would add up.
func Locate(cwd string) (Location, bool) {
	for root := cwd; ; root = filepath.Dir(root) {
		dotgit := filepath.Join(root, ".git")
		if fi, err := os.Stat(dotgit); err == nil {
			if fi.IsDir() {
				return location(root, dotgit, dotgit, true)
			}
			raw, err := os.ReadFile(dotgit)
			gitdir, ok := strings.CutPrefix(strings.TrimSpace(string(raw)), "gitdir: ")
			if err != nil || !ok {
				return Location{}, false
			}
			if !filepath.IsAbs(gitdir) {
				gitdir = filepath.Join(root, gitdir)
			}
			common, err := os.ReadFile(filepath.Join(gitdir, "commondir"))
			if err != nil {
				return location(root, gitdir, gitdir, true)
			}
			c := strings.TrimSpace(string(common))
			if !filepath.IsAbs(c) {
				c = filepath.Join(gitdir, c)
			}
			return location(root, gitdir, c, false)
		}
		if root == filepath.Dir(root) {
			return Location{}, false
		}
	}
}

func location(root, gitdir, common string, main bool) (Location, bool) {
	if real, err := filepath.EvalSymlinks(common); err == nil {
		common = real
	}
	head, _ := os.ReadFile(filepath.Join(gitdir, "HEAD"))
	branch, ok := strings.CutPrefix(strings.TrimSpace(string(head)), "ref: refs/heads/")
	if !ok {
		branch = ""
	}
	return Location{Root: root, Common: common, Branch: branch, Main: main}, true
}

// Place is where an agent works, as clients name it.
type Place struct {
	Project, Worktree, Branch string
	Main                      bool
}

// Find places cwd in a registered Project: the one sharing its repo, or, for
// a folder that isn't a repo, the one holding it.
func Find(f registry.File, cwd string) (Place, bool) {
	if !filepath.IsAbs(cwd) {
		return Place{}, false
	}
	at, inRepo := Locate(cwd)
	for _, p := range f.Projects {
		l, isRepo := Locate(p)
		switch {
		case isRepo && inRepo && l.Common == at.Common:
			return Place{Project: f.Name(p), Worktree: filepath.Base(at.Root), Branch: at.Branch, Main: at.Main}, true
		case !isRepo && (cwd == p || strings.HasPrefix(cwd, p+"/")):
			return Place{Project: f.Name(p), Worktree: filepath.Base(p), Main: true}, true
		}
	}
	return Place{}, false
}
