package wsserver

import (
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"testing"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// agentTree adds a worktree named name as pocketd makes one for an agent's session.
func agentTree(t *testing.T, b *bench, name string) string {
	t.Helper()
	c, err := worktree.Add(b.s.Registry.Load(), b.project, name, "")
	if err != nil {
		t.Fatal(err)
	}
	if err := b.s.Registry.AgentWorktree(c.Path, true); err != nil {
		t.Fatal(err)
	}
	return c.Path
}

func gone(t *testing.T, path string) bool {
	t.Helper()
	_, err := os.Stat(path)
	return os.IsNotExist(err)
}

func onSidebar(b *bench, path string) bool {
	return slices.ContainsFunc(worktree.Projects(b.s.Registry.Load())[0].Worktrees, func(w proto.Worktree) bool { return w.Path == path })
}

func TestAClosedAgentsCleanWorktreeGoesWithItsBranchUnlessTheBranchHoldsCommitsOfItsOwn(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	plain, committed := agentTree(t, &b, "plain"), agentTree(t, &b, "committed")
	gitOut(t, committed, "commit", "-q", "--allow-empty", "-m", "the child's own")
	own, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: plain})
	if onSidebar(&b, plain) {
		t.Fatal("the sidebar lists an agent's worktree")
	}
	b.s.AgentClosed("s1", filepath.Join(plain, "src"), own.Info().ID)
	b.s.AgentClosed("s2", committed, "")
	select {
	case <-own.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("the closed session's terminal is still open")
	}
	if !gone(t, plain) || !gone(t, committed) {
		t.Fatal("a clean agent's worktree stayed")
	}
	if gitOut(t, b.project, "branch", "--list", "plain") != "" {
		t.Fatal("the branch holding nothing of its own stayed")
	}
	if gitOut(t, b.project, "branch", "--list", "committed") == "" {
		t.Fatal("the branch holding the child's commit went, which its parent may still merge")
	}
	if got := b.s.Registry.Load().AgentWorktrees; len(got) != 0 {
		t.Fatalf("still recorded: %v", got)
	}
}

func TestAClosedAgentsWorktreeWithUncommittedWorkStaysAndComesOnTheSidebar(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	tree := agentTree(t, &b, "unfinished")
	os.WriteFile(filepath.Join(tree, "notes.md"), []byte("half done"), 0o644)
	b.s.AgentClosed("s1", tree, "")
	if gone(t, tree) {
		t.Fatal("the worktree holding untracked work went")
	}
	if !onSidebar(&b, tree) || slices.Contains(b.s.Registry.Load().AgentWorktrees, tree) {
		t.Fatal("the unfinished work isn't on the sidebar")
	}
}

func TestAClosedAgentsWorktreeSomeoneElseWorksInStaysOffTheSidebar(t *testing.T) {
	var b bench
	var open []orch.Session
	local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.OpenSessions = func() ([]orch.Session, error) { return open, nil }
	})
	shelled, shared := agentTree(t, &b, "shelled"), agentTree(t, &b, "shared")
	shell, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: shelled})
	open = []orch.Session{{ID: "other", Cwd: shared}}
	b.s.AgentClosed("s1", shelled, "")
	b.s.AgentClosed("s2", shared, "")
	if gone(t, shelled) || gone(t, shared) {
		t.Fatal("a worktree someone else works in went")
	}
	select {
	case <-shell.Done():
		t.Fatal("someone else's shell was closed")
	default:
	}
	if onSidebar(&b, shelled) || onSidebar(&b, shared) {
		t.Fatal("a worktree still in use came on the sidebar")
	}
}

func TestASessionClosingInAWorktreeAPersonMadeLeavesItAlone(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	c, err := worktree.Add(b.s.Registry.Load(), b.project, "persons", "")
	if err != nil {
		t.Fatal(err)
	}
	b.s.AgentClosed("s1", c.Path, "")
	if gone(t, c.Path) || !onSidebar(&b, c.Path) {
		t.Fatal("a person's worktree went or left the sidebar")
	}
}

func TestAnAgentsWorktreeGoesAfterItsProjectsTeardownRuns(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	script := `touch "$ANYWHERE_ROOT_PATH/torn-$ANYWHERE_WORKSPACE_NAME"`
	if runtime.GOOS == "windows" {
		script = `New-Item -ItemType File "$env:ANYWHERE_ROOT_PATH/torn-$env:ANYWHERE_WORKSPACE_NAME"`
	}
	if err := b.s.Registry.SetRepo(b.project, registry.Repo{Teardown: script}); err != nil {
		t.Fatal(err)
	}
	tree := agentTree(t, &b, "child")
	b.s.AgentClosed("s1", tree, "")
	if _, err := os.Stat(filepath.Join(b.project, "torn-child")); err != nil {
		t.Fatalf("the teardown didn't run: %v", err)
	}
	if !gone(t, tree) {
		t.Fatal("the worktree stayed")
	}
}

func TestAFailedTeardownDeletesNothingUntilTheDeleteSkipsIt(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	script := "echo containers still up; exit 4"
	if runtime.GOOS == "windows" {
		// PowerShell's echo prints each argument on a line of its own.
		script = "echo 'containers still up'; exit 4"
	}
	if err := b.s.Registry.SetRepo(b.project, registry.Repo{Teardown: script}); err != nil {
		t.Fatal(err)
	}
	tree := agentTree(t, &b, "calm-otter")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":` + q(tree) + `}`)
	if m := p.reply("wd"); m["code"] != proto.CodeTeardownFailed || m["path"] != tree || m["detail"] != "containers still up" {
		t.Fatalf("%v", m)
	}
	if gone(t, tree) {
		t.Fatal("the folder went though its teardown failed")
	}
	p.send(`{"type":"worktree.delete","id":"wf","path":` + q(tree) + `,"teardown":false}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if !gone(t, tree) || len(b.s.Registry.Load().AgentWorktrees) != 0 {
		t.Fatal("the worktree stayed, or stayed recorded as an agent's")
	}
}

func TestADeleteToTheTrashMovesTheFolderThere(t *testing.T) {
	var b bench
	home := t.TempDir()
	trash := filepath.Join(home, ".Trash")
	switch runtime.GOOS {
	case "linux":
		trash = filepath.Join(home, ".local", "share", "Trash", "files")
	case "windows":
		trash = filepath.Join(home, "trash")
	}
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) { s.Workspace.TrashDir = trash })
	t.Setenv("HOME", home)
	t.Setenv("XDG_DATA_HOME", filepath.Join(home, ".local", "share"))
	c, err := worktree.Add(b.s.Registry.Load(), b.project, "calm-otter", "")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(c.Path, "keep.txt"), []byte("mine"), 0o644)
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":` + q(c.Path) + `,"trash":true,"teardownSecs":5}`)
	if m := p.reply("wd"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if raw, err := os.ReadFile(filepath.Join(trash, "calm-otter", "keep.txt")); err != nil || string(raw) != "mine" || !gone(t, c.Path) {
		t.Fatalf("the folder isn't in the Trash: %v", err)
	}
	if out := gitOut(t, b.project, "worktree", "list"); filepath.Base(out) == "calm-otter" {
		t.Fatalf("git still lists it: %s", out)
	}
}

func TestInspectFailsRatherThanCallATreeGitCantReadClean(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	c, err := worktree.Add(b.s.Registry.Load(), b.project, "broken", "")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(gitOut(t, c.Path, "rev-parse", "--absolute-git-dir"), "index"), []byte("garbage"), 0o644)
	helloWith(p, proto.CapWorkspace, proto.CapWorktrees)
	p.send(`{"type":"worktree.inspect","id":"wi","path":` + q(c.Path) + `}`)
	if m := p.reply("wi"); m["type"] != "error" {
		t.Fatalf("%v", m)
	}
}

func TestInspectSaysWhatADeleteWouldCloseAndLoseForTheWorktreeAndItsSubagents(t *testing.T) {
	var b bench
	tree := func(name string) string { return filepath.Join(b.root, "pocket", name) }
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.Workspace.SubagentTrees = func(string, func(string, int64) string) (orch.Subtrees, error) {
			return orch.Subtrees{Local: []string{tree("child"), tree("detached")}}, nil
		}
	})
	for _, name := range []string{"parent", "child", "detached"} {
		if _, err := worktree.Add(b.s.Registry.Load(), b.project, name, ""); err != nil {
			t.Fatal(err)
		}
	}
	os.WriteFile(filepath.Join(tree("parent"), "a"), nil, 0o644)
	os.WriteFile(filepath.Join(tree("parent"), "b"), nil, 0o644)
	gitOut(t, tree("parent"), "commit", "-q", "--allow-empty", "-m", "parent's")
	gitOut(t, tree("child"), "commit", "-q", "--allow-empty", "-m", "child's")
	gitOut(t, tree("detached"), "checkout", "-q", "--detach")
	gitOut(t, tree("detached"), "commit", "-q", "--allow-empty", "-m", "detached's")
	b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: tree("parent")})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.inspect","id":"wi","path":` + q(tree("parent")) + `}`)
	if m := p.reply("wi"); m["type"] != "error" || m["message"] != "Malformed message" {
		t.Fatalf("without worktrees.v1: %v", m)
	}
	helloWith(p, proto.CapWorkspace, proto.CapWorktrees)
	p.send(`{"type":"worktree.inspect","id":"wi2","path":` + q(tree("parent")) + `}`)
	m := p.reply("wi2")
	trees, _ := m["trees"].([]any)
	if len(trees) != 3 || m["lost"] != 2.0 || m["lostAnyway"] != 1.0 {
		t.Fatalf("%v", m)
	}
	first := trees[0].(map[string]any)
	if first["path"] != tree("parent") || first["dirty"] != 2.0 || first["terminals"] != 1.0 || first["branch"] != "parent" {
		t.Fatalf("the worktree itself: %v", first)
	}
	if last := trees[2].(map[string]any); last["path"] != tree("detached") || last["branch"] != "" {
		t.Fatalf("the detached subagent's: %v", last)
	}
}

func TestAFailedTeardownLeavesTheSubagentsWorktreesToo(t *testing.T) {
	var b bench
	tree := func(name string) string { return filepath.Join(b.root, "pocket", name) }
	_, _, p := local(t, peer.OwnerOf(1), withWorkspace(t, &b), func(s *Server) {
		s.Workspace.SubagentTrees = func(string, func(string, int64) string) (orch.Subtrees, error) {
			return orch.Subtrees{Local: []string{tree("child")}}, nil
		}
	})
	for _, name := range []string{"parent", "child"} {
		if _, err := worktree.Add(b.s.Registry.Load(), b.project, name, ""); err != nil {
			t.Fatal(err)
		}
	}
	os.MkdirAll(filepath.Join(tree("parent"), filepath.Dir(worktree.ConfigFile)), 0o755)
	os.WriteFile(filepath.Join(tree("parent"), worktree.ConfigFile), []byte(`{"teardown":["exit 4"]}`), 0o644)
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":` + q(tree("parent")) + `,"deleteBranch":true,"force":true}`)
	if m := p.reply("wd"); m["code"] != proto.CodeTeardownFailed {
		t.Fatalf("%v", m)
	}
	if gone(t, tree("child")) || gitOut(t, b.project, "branch", "--list", "child") == "" {
		t.Fatal("the subagent's worktree went though the delete failed")
	}
}

func TestAPhonesDeleteRunsOnlyTheTeardownTheProjectsOwnCheckoutNames(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	c, err := worktree.Add(b.s.Registry.Load(), b.project, "picked", "")
	if err != nil {
		t.Fatal(err)
	}
	os.MkdirAll(filepath.Join(c.Path, filepath.Dir(worktree.ConfigFile)), 0o755)
	script := "touch " + filepath.Join(b.root, "ran")
	if runtime.GOOS == "windows" {
		script = "New-Item -ItemType File '" + filepath.Join(b.root, "ran") + "'"
	}
	os.WriteFile(filepath.Join(c.Path, worktree.ConfigFile), []byte(`{"teardown":[`+q(script)+`]}`), 0o644)
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":` + q(c.Path) + `,"deleteBranch":true,"force":true}`)
	if m := p.reply("wd"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if _, err := os.Stat(filepath.Join(b.root, "ran")); err == nil {
		t.Fatal("a phone's delete ran the script the worktree itself names")
	}
}

func TestADeleteKeepsABranchTheWorktreeSwitchedToAfterTheUserConfirmed(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	c, err := worktree.Add(b.s.Registry.Load(), b.project, "confirmed", "")
	if err != nil {
		t.Fatal(err)
	}
	gitOut(t, c.Path, "switch", "-c", "since")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":` + q(c.Path) + `,"deleteBranch":true,"force":true,"branch":"` + c.Branch + `"}`)
	if m := p.reply("wd"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if !gone(t, c.Path) || gitOut(t, b.project, "branch", "--list", "since") == "" {
		t.Fatal("the worktree stayed, or the branch it switched to went")
	}
}

func TestAClosedAgentsWorktreeGoesOnceTheLastShellInItCloses(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	tree := agentTree(t, &b, "shelled")
	shell, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: tree})
	b.s.AgentClosed("s1", tree, "")
	if gone(t, tree) {
		t.Fatal("the worktree went while a shell was open in it")
	}
	shell.Close()
	deadline := time.Now().Add(5 * time.Second)
	// The branch goes after the folder; waiting for it keeps git out of the repo the test's cleanup removes.
	for !gone(t, tree) || gitOut(t, b.project, "branch", "--list", "shelled") != "" {
		if time.Now().After(deadline) {
			t.Fatal("the worktree stayed once its last shell closed")
		}
		time.Sleep(50 * time.Millisecond)
	}
}
