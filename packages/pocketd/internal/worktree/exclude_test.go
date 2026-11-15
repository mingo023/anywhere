package worktree

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestAnExcludedPathIsNeitherListedNorCommittedInALinkedWorktree(t *testing.T) {
	main := repo(t)
	tree := filepath.Join(filepath.Dir(main), "linked")
	git(t, main, "worktree", "add", "-q", "-b", "linked", tree)
	skill := filepath.Join(tree, ".claude", "skills", "review")
	os.MkdirAll(skill, 0o755)
	os.WriteFile(filepath.Join(skill, "SKILL.md"), []byte("# Review\n"), 0o644)
	if out := git(t, tree, "status", "--porcelain"); !strings.Contains(out, ".claude/") {
		t.Fatalf("the skill isn't untracked before the exclude: %q", out)
	}
	if err := Exclude(tree, []string{"/.claude/skills/review/"}); err != nil {
		t.Fatal(err)
	}
	if out := git(t, tree, "status", "--porcelain"); out != "" {
		t.Fatalf("git still lists %q", out)
	}
	if committed, err := Commit(tree, "handback"); err != nil || committed {
		t.Fatalf("a handback committed the skill: %t %v", committed, err)
	}
}

func TestExcludingAPathTwiceWritesItOnce(t *testing.T) {
	main := repo(t)
	for range 2 {
		if err := Exclude(main, []string{"/.claude/skills/review/", "/.agents/skills/review/"}); err != nil {
			t.Fatal(err)
		}
	}
	loc, _ := Locate(main)
	text, _ := os.ReadFile(filepath.Join(loc.Common, "info", "exclude"))
	if strings.Count(string(text), "/.claude/skills/review/\n") != 1 || strings.Count(string(text), "/.agents/skills/review/\n") != 1 {
		t.Fatalf("info/exclude:\n%s", text)
	}
}

func TestExcludeRefusesAFolderOutsideAnyCheckout(t *testing.T) {
	if err := Exclude(t.TempDir(), []string{"/x/"}); err == nil {
		t.Fatal("no error for a folder with no repository")
	}
}
