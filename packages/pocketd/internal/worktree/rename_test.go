package worktree

import (
	"path/filepath"
	"testing"
)

func placeholder(t *testing.T) (project, tree string) {
	p := repo(t)
	w := filepath.Join(p, "..", "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", w)
	return p, w
}

func TestABranchNamedFromThePromptReplacesThePlaceholder(t *testing.T) {
	p, w := placeholder(t)
	got, err := RenameBranch(p, w, "calm-otter", "fix-login")
	if err != nil || got != "fix-login" || git(t, w, "branch", "--show-current") != "fix-login" {
		t.Fatalf("%q %v", got, err)
	}
}

func TestATakenBranchNameGetsTheNextFreeNumber(t *testing.T) {
	p, w := placeholder(t)
	git(t, p, "branch", "fix-login")
	git(t, p, "update-ref", "refs/remotes/origin/fix-login-2", "HEAD")
	if got, err := RenameBranch(p, w, "calm-otter", "fix-login"); err != nil || got != "fix-login-3" {
		t.Fatalf("%q %v", got, err)
	}
}

func TestAMovedOrPushedBranchKeepsItsName(t *testing.T) {
	p, w := placeholder(t)
	git(t, p, "update-ref", "refs/remotes/origin/calm-otter", "HEAD")
	if _, err := RenameBranch(p, w, "calm-otter", "fix-login"); err == nil || git(t, w, "branch", "--show-current") != "calm-otter" {
		t.Fatal("renamed a pushed branch")
	}
	p, w = placeholder(t)
	git(t, w, "switch", "-q", "-c", "other")
	if _, err := RenameBranch(p, w, "calm-otter", "fix-login"); err == nil || git(t, w, "branch", "--show-current") != "other" {
		t.Fatal("renamed after the worktree moved")
	}
}
