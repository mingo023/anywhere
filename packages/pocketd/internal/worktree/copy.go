package worktree

import (
	"errors"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/registry"
)

// CopyInto copies project's copy set into the new Worktree at path. It never
// overwrites, and skips symlinks, folders and anything outside the project.
// Skipped entries read "{rel}: {reason}".
func CopyInto(f registry.File, project, path string) (copied, skipped []string) {
	for _, rel := range copySet(f, project) {
		if reason := copyFile(project, path, rel); reason != "" {
			skipped = append(skipped, rel+": "+reason)
		} else {
			copied = append(copied, rel)
		}
	}
	return copied, skipped
}

// copySet is the repo's copy list, else its root .env* files, plus the lines
// of .worktreeinclude.
func copySet(f registry.File, project string) []string {
	set := slices.Clone(f.Repos[project].Copy)
	if len(set) == 0 {
		entries, _ := os.ReadDir(project)
		for _, e := range entries {
			if strings.HasPrefix(e.Name(), ".env") {
				set = append(set, e.Name())
			}
		}
	}
	if raw, err := os.ReadFile(filepath.Join(project, ".worktreeinclude")); err == nil {
		for _, l := range strings.Split(string(raw), "\n") {
			if l = strings.TrimSpace(l); l != "" && !strings.HasPrefix(l, "#") {
				set = append(set, l)
			}
		}
	}
	var out []string
	for _, rel := range set {
		if rel = filepath.Clean(rel); !slices.Contains(out, rel) {
			out = append(out, rel)
		}
	}
	return out
}

func copyFile(project, path, rel string) string {
	src, dst := filepath.Join(project, rel), filepath.Join(path, rel)
	if filepath.IsAbs(rel) || rel == ".." || strings.HasPrefix(rel, "../") {
		return "outside the project"
	}
	fi, err := os.Lstat(src)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return "missing"
	case err != nil:
		return err.Error()
	case !fi.Mode().IsRegular():
		return "not a regular file"
	case !inside(project, filepath.Dir(src)):
		return "outside the project"
	}
	if !inside(path, existing(filepath.Dir(dst))) {
		return "outside the project"
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err.Error()
	}
	in, err := os.Open(src)
	if err != nil {
		return err.Error()
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if errors.Is(err, fs.ErrExist) {
		return "exists"
	}
	if err != nil {
		return err.Error()
	}
	_, err = io.Copy(out, in)
	if err == nil {
		err = out.Chmod(fi.Mode().Perm())
	}
	if cerr := out.Close(); err == nil {
		err = cerr
	}
	if err != nil {
		return err.Error()
	}
	return ""
}

// inside reports whether dir, symlinks resolved, is root or below it: a
// checked-in symlinked folder must not lead a copy out of either tree.
func inside(root, dir string) bool {
	r, err1 := filepath.EvalSymlinks(root)
	d, err2 := filepath.EvalSymlinks(dir)
	return err1 == nil && err2 == nil && (d == r || strings.HasPrefix(d, r+"/"))
}

func existing(dir string) string {
	for {
		if _, err := os.Lstat(dir); err == nil || dir == filepath.Dir(dir) {
			return dir
		}
		dir = filepath.Dir(dir)
	}
}
