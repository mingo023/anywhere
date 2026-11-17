//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestAWorktreeChildStartsFromTheParentsHeadAndTheResultSaysTheParentIsDirty(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	git := func(args ...string) string {
		return gitOut(t, h.Repo, append([]string{"-c", "user.name=e2e", "-c", "user.email=e2e@x", "-c", "commit.gpgsign=false"}, args...)...)
	}
	git("switch", "-q", "-c", "feature")
	file := filepath.Join(h.Repo, "notes.txt")
	os.WriteFile(file, []byte("committed\n"), 0o644)
	git("add", "notes.txt")
	git("commit", "-q", "-m", "notes")
	os.WriteFile(file, []byte("edited, not committed\n"), 0o644)
	head := git("rev-parse", "HEAD")

	phone.Prompt(parent, `mcp delegate_task {"task":"read the notes","title":"Notes"}`)

	phone.WaitFor("the warning", saysAbout(parent, "uncommitted changes", "starts from your HEAD commit"))
	child := childOf(phone, parent).Agent
	if child.Branch == "main" || child.Branch == "feature" {
		t.Fatalf("the child's branch is %q, want one of its own", child.Branch)
	}
	var worktree string
	for _, line := range strings.Split(git("worktree", "list", "--porcelain"), "\n") {
		if path, ok := strings.CutPrefix(line, "worktree "); ok && path != h.Repo {
			worktree = path
		}
	}
	if worktree == "" {
		t.Fatal("no worktree for the child")
	}
	if got := gitOut(t, worktree, "rev-parse", "HEAD"); got != head {
		t.Errorf("the child's worktree is at %s, want the parent's HEAD %s", got, head)
	}
	if got, _ := os.ReadFile(filepath.Join(worktree, "notes.txt")); string(got) != "committed\n" {
		t.Errorf("the child's notes.txt = %q, want the committed one", got)
	}
}

func TestTwoWorktreeChildrenWithTheSameTitleGetBranchesOfTheirOwn(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	for range 2 {
		phone.Prompt(parent, `mcp delegate_task {"task":"read the notes","title":"Notes"}`)
		phone.WaitStatus(parent, "done")
	}
	var ts []proto.Delegation
	h.eventually("two tasks started or failed", func() bool {
		ts = delegationsOf(phone, parent)
		return len(ts) == 2 && ts[0].Status != "queued" && ts[0].Status != "starting" && ts[1].Status != "queued" && ts[1].Status != "starting"
	})
	for _, d := range ts {
		if d.Status == "failed" {
			t.Fatalf("a task failed: %+v", d)
		}
	}
	branches := map[string]bool{}
	for _, line := range strings.Split(gitOut(t, h.Repo, "worktree", "list", "--porcelain"), "\n") {
		if b, ok := strings.CutPrefix(line, "branch "); ok {
			branches[b] = true
		}
	}
	if len(branches) != 3 {
		t.Fatalf("branches %v, want the repo's and one for each child", branches)
	}
}
