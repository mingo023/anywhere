//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/ops"
	"pocketd/internal/proto"
	"pocketd/internal/worktree"
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

func TestDeletingTheParentsWorktreeFromThePhoneTakesTheOneItsSubagentWorkedIn(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	h.WaitScreen(done.TerminalID, "fake claude ready")
	phone.Prompt(done.AgentID, `mcp delegate_task {"task":"hello","title":"Notes"}`)
	phone.WaitFor("the child done", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == done.AgentID && m.Agent.Status == "done"
	})
	var parentTree, childTree string
	for _, line := range strings.Split(gitOut(t, h.Repo, "worktree", "list", "--porcelain"), "\n") {
		path, ok := strings.CutPrefix(line, "worktree ")
		switch {
		case !ok || path == h.Repo:
		case strings.HasSuffix(path, "calm-otter"):
			parentTree = path
		default:
			childTree = path
		}
	}
	if parentTree == "" || childTree == "" {
		t.Fatalf("worktrees %q and %q, want the parent's and the child's", parentTree, childTree)
	}

	workspace := h.Owner(proto.CapWorkspace)
	workspace.Send(map[string]any{"type": "worktree.delete", "id": "wd", "path": parentTree, "deleteBranch": true, "force": true})
	if m := workspace.WaitFor("the delete's answer", func(m Message) bool { return m.ID == "wd" }); m.Type != "ack" {
		t.Fatalf("got %s", m.Raw)
	}
	for _, tree := range []string{parentTree, childTree} {
		if _, err := os.Stat(tree); !os.IsNotExist(err) {
			t.Errorf("%s stayed: %v", tree, err)
		}
	}
	// The child's branch may have been renamed from its prompt by now.
	if branches := gitOut(t, h.Repo, "branch", "--format=%(refname:short)"); branches != "main" {
		t.Errorf("branches %q, want main alone", branches)
	}
}

func TestAWorktreesSubagentWorktreesAreListedForADesktopOnceItsSessionsClosed(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	h.WaitScreen(done.TerminalID, "fake claude ready")
	phone.Prompt(done.AgentID, `mcp delegate_task {"task":"hello","title":"Notes"}`)
	phone.WaitFor("the child done", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == done.AgentID && m.Agent.Status == "done"
	})
	var parentTree, childTree string
	for _, line := range strings.Split(gitOut(t, h.Repo, "worktree", "list", "--porcelain"), "\n") {
		path, ok := strings.CutPrefix(line, "worktree ")
		switch {
		case !ok || path == h.Repo:
		case strings.HasSuffix(path, "calm-otter"):
			parentTree = path
		default:
			childTree = path
		}
	}
	child := childOf(phone, done.AgentID).Agent
	h.Ops().Send(ops.Msg{Op: "close", ID: done.TerminalID})
	h.eventually("the parent's and child's terminals to end", func() bool { return !h.lists(done.TerminalID) && !h.lists(child.TerminalID) })

	c := h.Ops()
	c.Send(ops.Msg{Op: "worktree.subagents", ID: "s", Path: parentTree})
	if r, err := c.Recv(); err != nil || r.Ev != "worktree.subagents" || len(r.Trees) != 1 || r.Trees[0].Path != childTree || r.Trees[0].Since != worktree.MadeAt(childTree) || r.Trees[0].Since == 0 || r.Trees[0].Host != "" {
		t.Fatalf("got %+v %v, want [%s]", r, err, childTree)
	}
	c.Send(ops.Msg{Op: "worktree.subagents", ID: "c", Path: childTree})
	if r, err := c.Recv(); err != nil || r.Ev != "worktree.subagents" || len(r.Trees) != 0 {
		t.Fatalf("got %+v %v, want none below the child's", r, err)
	}
}
