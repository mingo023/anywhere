package worktree

import (
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

func TestAMovedHomesWorktreesAreListedAtTheirNewPathsAndAGoneOneIsSkipped(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd never moves its home on Windows")
	}
	p := repo(t)
	user := filepath.Dir(p)
	old, now := filepath.Join(user, ".anywhere"), filepath.Join(user, ".zoes")
	for _, name := range []string{"a", "b"} {
		git(t, p, "worktree", "add", "-q", filepath.Join(old, "worktrees", "pocket", name), "-b", name)
	}
	git(t, p, "worktree", "add", "-q", filepath.Join(old, "worktrees", "gone", "c"), "-b", "c")
	orphan := repo(t)
	git(t, orphan, "worktree", "add", "-q", filepath.Join(old, "worktrees", "orphan", "d"), "-b", "d")
	os.RemoveAll(filepath.Join(old, "worktrees", "gone"))
	os.RemoveAll(orphan)
	if err := os.Rename(old, now); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(now, old); err != nil {
		t.Fatal(err)
	}
	if err := RepairMoved(filepath.Join(now, "worktrees"), old); err != nil {
		t.Fatal(err)
	}
	var listed []string
	for _, l := range strings.Split(git(t, p, "worktree", "list", "--porcelain"), "\n") {
		if path, ok := strings.CutPrefix(l, "worktree "); ok {
			listed = append(listed, path)
		}
	}
	want := []string{p, filepath.Join(now, "worktrees", "pocket", "a"), filepath.Join(now, "worktrees", "pocket", "b"), filepath.Join(old, "worktrees", "gone", "c")}
	slices.Sort(listed)
	slices.Sort(want)
	if !slices.Equal(listed, want) {
		t.Fatalf("%q", listed)
	}
	if err := RepairMoved(filepath.Join(now, "worktrees"), old); err != nil {
		t.Fatal("a second run", err)
	}
}
