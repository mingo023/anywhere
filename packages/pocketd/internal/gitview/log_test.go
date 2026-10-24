package gitview

import (
	"fmt"
	"reflect"
	"testing"
)

func TestTheLogPagesChildrenBeforeParentsAndSaysWhetherMoreRemain(t *testing.T) {
	repo := scratch(t)
	for i := range 4 {
		sh(t, repo, "commit", "-q", "--allow-empty", "-m", fmt.Sprintf("c%d", i))
	}
	s := service()
	tips, first, more, err := s.Log(repo, false, 0, 3)
	if err != nil || !more || len(first) != 3 || first[0].Subject != "c3" || tips.Head != first[0].Sha || tips.Branch != "main" {
		t.Fatalf("%+v %+v %v %v", tips, first, more, err)
	}
	if !reflect.DeepEqual(first[0].Parents, []string{first[1].Sha}) || first[0].Author != "t" || first[0].At == 0 {
		t.Fatalf("%+v", first[0])
	}
	_, rest, more, _ := s.Log(repo, false, 3, 3)
	if more || len(rest) != 2 || rest[1].Subject != "init" || len(rest[1].Parents) != 0 {
		t.Fatalf("%+v %v", rest, more)
	}
}

func TestTheGraphFollowsTheBranchABranchWasCreatedFromAndItsUpstream(t *testing.T) {
	repo := developAheadOfMaster(t)
	develop := sh(t, repo, "rev-parse", "origin/develop")
	sh(t, repo, "switch", "-qc", "feature", "--no-track", "origin/develop")
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "mine")
	sh(t, repo, "update-ref", "refs/remotes/origin/feature", "HEAD~0")
	sh(t, repo, "remote", "add", "origin", "/nowhere")
	sh(t, repo, "config", "branch.feature.remote", "origin")
	sh(t, repo, "config", "branch.feature.merge", "refs/heads/feature")
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "unpushed")
	tips, commits, _, _ := service().Log(repo, false, 0, 50)
	upstream := sh(t, repo, "rev-parse", "origin/feature")
	if tips.Base != develop || tips.Upstream != upstream || tips.Branch != "feature" || len(commits) != 4 {
		t.Fatalf("%+v %d", tips, len(commits))
	}
}

func TestAllBranchesBringInHistoryNoTipReaches(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "side")
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "side only")
	sh(t, repo, "checkout", "-q", "main")
	_, mine, _, _ := service().Log(repo, false, 0, 50)
	_, all, _, _ := service().Log(repo, true, 0, 50)
	if len(mine) != 1 || len(all) != 2 {
		t.Fatalf("%d %d", len(mine), len(all))
	}
}

func TestAnEmptyRepositoryHasNoHistory(t *testing.T) {
	dir := t.TempDir()
	sh(t, dir, "init", "-q")
	tips, commits, more, err := service().Log(dir, false, 0, 50)
	if err != nil || tips.Head != "" || len(commits) != 0 || more {
		t.Fatalf("%+v %v %v", tips, commits, err)
	}
}

func TestACommitListsItsFilesWithRenamesAndCounts(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "old name.txt", "one\ntwo\nthree\nfour\nfive\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "base")
	sh(t, repo, "mv", "old name.txt", "new name.txt")
	write(t, repo, "a", "a\nb\n")
	write(t, repo, "logo.png", "\x89PNG\x00")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "Rename and add", "-m", "Body line")
	c, err := service().Commit(repo, "HEAD")
	if err != nil {
		t.Fatal(err)
	}
	if c.Subject != "Rename and add" || c.Body != "Body line" || c.Author != "t" || len(c.Sha) != 40 || len(c.Files) != 3 {
		t.Fatalf("%+v", c)
	}
	byPath := map[string]string{}
	for _, f := range c.Files {
		counts := "-"
		if f.Added != nil {
			counts = fmt.Sprintf("+%d-%d", *f.Added, *f.Removed)
		}
		byPath[f.Path] = f.Status + " " + f.OldPath + " " + counts
	}
	want := map[string]string{"a": "M  +1-0", "logo.png": "A  -", "new name.txt": "R old name.txt +0-0"}
	if !reflect.DeepEqual(byPath, want) {
		t.Fatalf("%v", byPath)
	}
	root, _ := service().Commit(repo, "HEAD~2")
	if len(root.Files) != 1 || root.Files[0].Path != "a" || root.Files[0].Status != "A" {
		t.Fatalf("%+v", root)
	}
}
