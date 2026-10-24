// Package fsview is what a phone sees of a tree's files: listings with git
// letters, previews classified as the desktop's Explorer does, go-to-file
// search, and image uploads for the chat.
package fsview

import (
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/worktree"
)

// Refusal is a request refused with one of protocol.md's codes.
type Refusal struct{ Code, Message string }

func (r *Refusal) Error() string { return r.Message }

var (
	errNotATree    = &Refusal{proto.CodeNotATree, "This folder isn't a project on your Mac"}
	errOutsideTree = &Refusal{proto.CodeOutsideTree, "That path is outside the project"}
)

// Tree is tree's real path when it is a folder that worktree.Containing
// places in a registered Project and its real path stays in that Project's
// folders, so a link inside a Project can't name a tree elsewhere.
func Tree(f registry.File, tree string) (string, error) {
	if !filepath.IsAbs(tree) || filepath.Clean(tree) != tree {
		return "", errNotATree
	}
	project, ok := worktree.Containing(f, tree)
	if !ok {
		return "", errNotATree
	}
	real, err := filepath.EvalSymlinks(tree)
	if err != nil {
		return "", errNotATree
	}
	if info, err := os.Stat(real); err != nil || !info.IsDir() {
		return "", errNotATree
	}
	home, _ := os.UserHomeDir()
	for _, root := range []string{project, worktree.Dir(f, project), filepath.Join(home, ".worktrees", f.Name(project))} {
		if r, err := filepath.EvalSymlinks(root); err == nil && within(real, r) {
			return real, nil
		}
	}
	return "", errNotATree
}

// Join is rel inside the real tree root, followed through links; outside_tree
// when rel is absolute, climbs with "..", or a link leads out. A path that
// doesn't exist is checked as far as it does.
func Join(root, rel string) (string, error) {
	if rel == "" {
		return root, nil
	}
	if filepath.IsAbs(rel) || strings.ContainsRune(rel, 0) {
		return "", errOutsideTree
	}
	for part := range strings.SplitSeq(filepath.ToSlash(rel), "/") {
		if part == ".." {
			return "", errOutsideTree
		}
	}
	real, err := existing(filepath.Join(root, rel))
	if err != nil || !within(real, root) {
		return "", errOutsideTree
	}
	return real, nil
}

// existing resolves the deepest part of p that exists and keeps the rest.
func existing(p string) (string, error) {
	dir, rest := p, ""
	for {
		real, err := filepath.EvalSymlinks(dir)
		if err == nil {
			return filepath.Join(real, rest), nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", err
		}
		rest = filepath.Join(filepath.Base(dir), rest)
		dir = parent
	}
}

func within(p, root string) bool {
	return p == root || strings.HasPrefix(p, strings.TrimSuffix(root, "/")+"/")
}
