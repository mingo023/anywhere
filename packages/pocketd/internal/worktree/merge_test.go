package worktree

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"testing"
)

// commitFile writes body to name in dir and commits it.
func commitFile(t *testing.T, dir, name, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	git(t, dir, "add", name)
	git(t, dir, "commit", "-q", "-m", name)
}

func TestHeadIsTheCommitTheCheckoutIsOn(t *testing.T) {
	p := repo(t)
	if got, err := Head(p); err != nil || got != git(t, p, "rev-parse", "HEAD") {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestCommitTakesEveryChangeWithoutRunningHooks(t *testing.T) {
	p := repo(t)
	commitFile(t, p, "kept.txt", "a\n")
	os.WriteFile(filepath.Join(p, "kept.txt"), []byte("b\n"), 0o644)
	os.WriteFile(filepath.Join(p, "new.txt"), []byte("n\n"), 0o644)
	os.WriteFile(filepath.Join(p, ".git", "hooks", "pre-commit"), []byte("#!/bin/sh\nexit 1\n"), 0o755)
	committed, err := Commit(p, "team t1: Add new")
	if err != nil || !committed {
		t.Fatalf("committed %t, %v", committed, err)
	}
	if got := git(t, p, "show", "--name-only", "--format=%s", "HEAD"); got != "team t1: Add new\n\nkept.txt\nnew.txt" {
		t.Fatalf("got %q", got)
	}
}

func TestCommitSkipsACleanCheckout(t *testing.T) {
	p := repo(t)
	before := git(t, p, "rev-parse", "HEAD")
	committed, err := Commit(p, "nothing")
	if err != nil || committed || git(t, p, "rev-parse", "HEAD") != before {
		t.Fatalf("committed %t, %v", committed, err)
	}
}

func TestStatIsWhatTheBranchChangedSinceItLeftItsBase(t *testing.T) {
	p := repo(t)
	git(t, p, "checkout", "-q", "-b", "work")
	commitFile(t, p, "a.txt", "one\ntwo\n")
	git(t, p, "checkout", "-q", "main")
	commitFile(t, p, "later.txt", "x\n")
	git(t, p, "checkout", "-q", "work")
	got, err := Stat(p, "main")
	if want := " a.txt | 2 ++\n 1 file changed, 2 insertions(+)"; err != nil || got != want {
		t.Fatalf("got %q, %v", got, err)
	}
}

// branchWith makes branch off main in p holding name=body, and leaves p on main.
func branchWith(t *testing.T, p, branch, name, body string) {
	t.Helper()
	git(t, p, "checkout", "-q", "-b", branch, "main")
	commitFile(t, p, name, body)
	git(t, p, "checkout", "-q", "main")
}

func TestMergeBranchesMakesABranchHoldingEachOfThemWithoutACheckout(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	branchWith(t, p, "t2", "b.txt", "b\n")
	if err := MergeBranches(p, "t3-base", []string{"t1", "t2"}); err != nil {
		t.Fatal(err)
	}
	if got := git(t, p, "ls-tree", "--name-only", "t3-base"); got != "a.txt\nb.txt" {
		t.Fatalf("tree %q", got)
	}
	if got := git(t, p, "log", "-1", "--format=%s", "t3-base"); got != "Merge t2" {
		t.Fatalf("subject %q", got)
	}
	if branch, status := git(t, p, "branch", "--show-current"), git(t, p, "status", "--porcelain"); branch != "main" || status != "" {
		t.Fatalf("checkout moved: %q %q", branch, status)
	}
}

func TestMergeBranchesNamesTheFilesThatConflictAndMakesNoBranch(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "same.txt", "one\n")
	branchWith(t, p, "t2", "same.txt", "two\n")
	err := MergeBranches(p, "t3-base", []string{"t1", "t2"})
	if c, ok := errors.AsType[*Conflict](err); !ok || !reflect.DeepEqual(*c, Conflict{Branch: "t2", Files: []string{"same.txt"}}) {
		t.Fatalf("got %v", err)
	}
	if got := git(t, p, "branch", "--list", "t3-base"); got != "" {
		t.Fatalf("made %q", got)
	}
}

func TestMergeMergesEachBranchWithItsOwnMergeCommitInOrder(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	git(t, p, "checkout", "-q", "-b", "t2", "t1")
	commitFile(t, p, "b.txt", "b\n")
	git(t, p, "checkout", "-q", "main")
	merged, err := Merge(p, []string{"t1", "t2"})
	if err != nil || !reflect.DeepEqual(merged, []string{"t1", "t2"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
	if got := git(t, p, "log", "--merges", "--format=%s"); got != "Merge t2\nMerge t1" {
		t.Fatalf("merges %q", got)
	}
}

func TestMergeRefusesUncommittedChangesButNotUntrackedFiles(t *testing.T) {
	p := repo(t)
	commitFile(t, p, "a.txt", "a\n")
	branchWith(t, p, "t1", "b.txt", "b\n")
	os.WriteFile(filepath.Join(p, "a.txt"), []byte("edited\n"), 0o644)
	if merged, err := Merge(p, []string{"t1"}); err != ErrDirty || merged != nil {
		t.Fatalf("merged %v, %v", merged, err)
	}
	os.WriteFile(filepath.Join(p, "a.txt"), []byte("a\n"), 0o644)
	os.WriteFile(filepath.Join(p, "scratch.txt"), []byte("x\n"), 0o644)
	if merged, err := Merge(p, []string{"t1"}); err != nil || !reflect.DeepEqual(merged, []string{"t1"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
}

func TestAConflictAbortsThatMergeAndKeepsTheOnesBefore(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	branchWith(t, p, "t2", "same.txt", "two\n")
	commitFile(t, p, "same.txt", "main\n")
	merged, err := Merge(p, []string{"t1", "t2"})
	c, ok := errors.AsType[*Conflict](err)
	if !ok || !reflect.DeepEqual(*c, Conflict{Branch: "t2", Files: []string{"same.txt"}}) || !reflect.DeepEqual(merged, []string{"t1"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
	if got := git(t, p, "status", "--porcelain"); got != "" {
		t.Fatalf("merge left in progress: %q", got)
	}
	if got := git(t, p, "log", "--merges", "--format=%s"); got != "Merge t1" {
		t.Fatalf("merges %q", got)
	}
}

func TestAMergeInProgressIsFoundAndAborted(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "same.txt", "one\n")
	commitFile(t, p, "same.txt", "main\n")
	if busy, err := MergeInProgress(p); err != nil || busy {
		t.Fatalf("before the merge: %t, %v", busy, err)
	}
	if out, err := exec.Command("git", "-C", p, "merge", "--no-edit", "t1").CombinedOutput(); err == nil {
		t.Fatalf("the merge didn't conflict:\n%s", out)
	}
	if busy, err := MergeInProgress(p); err != nil || !busy {
		t.Fatalf("during the merge: %t, %v", busy, err)
	}
	if err := AbortMerge(p); err != nil {
		t.Fatal(err)
	}
	if busy, _ := MergeInProgress(p); busy || git(t, p, "status", "--porcelain") != "" {
		t.Fatal("the merge is still in progress")
	}
}
