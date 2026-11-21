package wsserver

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/devices"
	"pocketd/internal/hosts"
	"pocketd/internal/launch"
	"pocketd/internal/locals"
	"pocketd/internal/names"
	"pocketd/internal/orch"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/workspace"
	"pocketd/internal/worktree"
)

var (
	aPhone  = peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	aLegacy = peer.FromDevice(devices.Device{ID: devices.LegacyID, Scopes: devices.LegacyScopes})
	aPTY    = peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}
)

func gitOut(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

// bench is a Mac with one project, a git repo on main, whose new worktrees go under root.
type bench struct {
	project, root string
	s             *Server
	terms         *terminal.Manager
}

func withWorkspace(t *testing.T, b *bench) func(*Server) {
	t.Setenv("GIT_CONFIG_GLOBAL", "/dev/null")
	t.Setenv("GIT_CONFIG_NOSYSTEM", "1")
	t.Setenv("GIT_AUTHOR_NAME", "t")
	t.Setenv("GIT_AUTHOR_EMAIL", "t@t")
	t.Setenv("GIT_COMMITTER_NAME", "t")
	t.Setenv("GIT_COMMITTER_EMAIL", "t@t")
	base, _ := filepath.EvalSymlinks(t.TempDir())
	b.project, b.root = filepath.Join(base, "pocket"), filepath.Join(base, "trees")
	os.Mkdir(b.project, 0o755)
	gitOut(t, b.project, "init", "-q", "-b", "main")
	gitOut(t, b.project, "commit", "-q", "--allow-empty", "-m", "init")
	home := t.TempDir()
	raw, _ := json.Marshal(registry.File{Projects: []string{b.project}, Repos: map[string]registry.Repo{}, Worktree: registry.Worktree{Root: b.root}})
	path := filepath.Join(home, "desktop.json")
	os.WriteFile(path, raw, 0o600)
	reg := registry.New(path)
	b.terms = terminal.NewManager()
	t.Cleanup(func() { b.terms.CloseAll(0) })
	d := &daemon.Daemon{Terminals: b.terms}
	l := launch.New(d, reg, config.NewSettings(home), nil)
	projects := workspace.NewProjects(reg)
	projects.Scan()
	return func(s *Server) {
		b.s = s
		s.Registry, s.Terminals, s.Launch, s.Names, s.Locals = reg, b.terms, l, names.Open(home), locals.Open(home)
		s.Workspace = WorkspaceService{Projects: projects, Accounts: &workspace.AccountList{Store: &accounts.Store{Root: t.TempDir(), Home: t.TempDir()},
			Running: func(string, string) int { return 0 }}, Switch: d.Switch}
	}
}

// untilType reads messages until one of type, returning those before it too.
func (p *phone) untilType(typ string) (map[string]any, []map[string]any) {
	p.t.Helper()
	var before []map[string]any
	for {
		m := p.recv()
		if m["type"] == typ {
			return m, before
		}
		before = append(before, m)
	}
}

func treePaths(m map[string]any) []string {
	var out []string
	for _, p := range m["projects"].([]any) {
		for _, w := range p.(map[string]any)["worktrees"].([]any) {
			out = append(out, w.(map[string]any)["path"].(string))
		}
	}
	return out
}

func TestAWorkspacePhoneGetsTheProjectListAfterHelloThenAPushForEachChange(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	m := p.recv()
	if m["type"] != "project.list" || m["id"] != nil || !reflect.DeepEqual(treePaths(m), []string{b.project}) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"worktree.create","id":"wc","requestId":"r1","project":"` + b.project + `","name":"calm-otter","setup":false}`)
	created, before := p.untilType("worktree.created")
	wt := filepath.Join(b.root, "pocket", "calm-otter")
	if created["id"] != "wc" || created["requestId"] != "r1" || created["path"] != wt || created["branch"] != "calm-otter" {
		t.Fatalf("%v", created)
	}
	var steps []string
	var push map[string]any
	for _, m := range before {
		switch m["type"] {
		case "worktree.progress":
			if m["id"] != "wc" || m["requestId"] != "r1" {
				t.Fatalf("%v", m)
			}
			if len(steps) == 0 || steps[len(steps)-1] != m["step"] {
				steps = append(steps, m["step"].(string))
			}
		case "project.list":
			push = m
		}
	}
	if !reflect.DeepEqual(steps, []string{"prepare", "verify", "fetch", "worktree", "copy"}) {
		t.Fatalf("steps %v", steps)
	}
	if push == nil || !reflect.DeepEqual(treePaths(push), []string{b.project, wt}) {
		t.Fatalf("the list wasn't pushed before worktree.created: %v", before)
	}
	p.send(`{"type":"project.list","id":"pl"}`)
	if m := p.reply("pl"); len(treePaths(m)) != 2 {
		t.Fatalf("%v", m)
	}
}

func TestAWorktreeCreateSentAgainAfterTheLinkDropsGetsTheFirstWorktreeNotASecond(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	create := func(id, name string) string {
		return `{"type":"worktree.create","id":"` + id + `","requestId":"r1","project":"` + b.project + `","name":"` + name + `","setup":false}`
	}
	p.send(create("w1", "calm-otter"))
	p.ws.CloseNow()
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	q := &phone{t: t, ws: ws, url: p.url}
	helloWith(q, proto.CapWorkspace)
	wt := filepath.Join(b.root, "pocket", "calm-otter")
	for _, id := range []string{"w2", "w3"} {
		q.send(create(id, "calm-otter"))
		if m, _ := q.untilType("worktree.created"); m["id"] != id || m["path"] != wt || m["branch"] != "calm-otter" {
			t.Fatalf("%v", m)
		}
	}
	q.send(create("w4", "bold-fox"))
	if m, _ := q.untilType("error"); m["id"] != "w4" || m["code"] != "duplicate" {
		t.Fatalf("%v", m)
	}
	if n := strings.Count(gitOut(t, b.project, "worktree", "list", "--porcelain"), "worktree "); n != 2 {
		t.Fatalf("%d worktrees", n)
	}
}

func TestANewWorktreeRunsTheProjectsSetupInATerminalOfItsOwn(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	os.MkdirAll(filepath.Join(b.project, ".anywhere"), 0o755)
	os.WriteFile(filepath.Join(b.project, ".anywhere", "config.json"), []byte(`{"setup":["touch set-up"]}`), 0o644)
	helloWith(p, proto.CapWorkspace)
	p.recv()
	p.send(`{"type":"worktree.create","id":"wc","requestId":"r1","project":"` + b.project + `","name":"with-setup"}`)
	created, before := p.untilType("worktree.created")
	if !strings.Contains(stepsOf(before), "setup") {
		t.Fatalf("no setup step: %v", before)
	}
	wt := created["path"].(string)
	deadline := time.Now().Add(10 * time.Second)
	for {
		if _, err := os.Stat(filepath.Join(wt, "set-up")); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("the setup never ran")
		}
		time.Sleep(50 * time.Millisecond)
	}
	if n := len(b.terms.All()); n != 1 || b.terms.All()[0].Info().Cwd != wt {
		t.Fatalf("%d terminals", n)
	}
}

func stepsOf(msgs []map[string]any) string {
	var steps []string
	for _, m := range msgs {
		if m["type"] == "worktree.progress" {
			steps = append(steps, m["step"].(string))
		}
	}
	return strings.Join(steps, " ")
}

func TestANewWorktreeNeedsAFreeNameInAKnownProject(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	p.recv()
	for raw, code := range map[string]string{
		`{"type":"worktree.create","id":"x","requestId":"r1","project":"` + b.project + `","name":"main"}`:     "worktree_exists",
		`{"type":"worktree.create","id":"x","requestId":"r2","project":"` + b.project + `","name":"no good"}`:  "invalid_name",
		`{"type":"worktree.create","id":"x","requestId":"r3","project":"/elsewhere","name":"calm-otter"}`:      "unknown_project",
		`{"type":"worktree.create","id":"x","requestId":"r4","project":"` + b.project + `","name":"","pr":12}`: "",
	} {
		p.send(raw)
		m, _ := p.untilType("error")
		if code != "" && m["code"] != code || m["id"] != "x" {
			t.Errorf("%s: %v", raw, m)
		}
	}
}

func TestDeletingAWorktreeClosesItsTerminalsForgetsItsNameAndMayDeleteItsBranch(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "calm-otter")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "calm-otter", ""); err != nil {
		t.Fatal(err)
	}
	b.s.Names.Rename(wt, "Fix login")
	in, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: filepath.Join(wt)})
	out, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: b.project})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + wt + `","deleteBranch":true}`)
	if m, _ := p.untilType("ack"); m["id"] != "wd" {
		t.Fatalf("%v", m)
	}
	select {
	case <-in.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("the worktree's terminal is still open")
	}
	select {
	case <-out.Done():
		t.Fatal("a terminal outside the worktree closed")
	default:
	}
	if _, err := os.Stat(wt); !os.IsNotExist(err) {
		t.Fatalf("the folder stayed: %v", err)
	}
	if gitOut(t, b.project, "branch", "--list", "calm-otter") != "" {
		t.Fatal("the branch stayed")
	}
	if _, ok := b.s.Names.Titles()[wt]; ok {
		t.Fatal("the name stayed")
	}
	if list, _ := b.s.Workspace.Projects.List(); len(list[0].Worktrees) != 1 {
		t.Fatalf("the cache still lists it: %+v", list)
	}
}

func TestDeletingAWorktreeTakesTheOnesAnywhereMadeForItsSessionsSubagents(t *testing.T) {
	var b bench
	tree := func(name string) string { return filepath.Join(b.root, "pocket", name) }
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.Workspace.SubagentTrees = func(parent string, treeOf func(string, int64) string) (orch.Subtrees, error) {
			now := time.Now().UnixMilli()
			if parent != tree("parent") || treeOf(filepath.Join(tree("child"), "src"), now) != tree("child") || treeOf(b.project, 0) != b.project {
				t.Error("the parent's worktree or treeOf is wrong")
			}
			if treeOf(tree("child"), 1) != "" {
				t.Error("a session that closed before the child's worktree was made counts as its")
			}
			return orch.Subtrees{Local: []string{tree("child"), tree("unmade"), tree("gone"), b.project}}, nil
		}
	})
	for _, name := range []string{"parent", "child"} {
		if _, err := worktree.Add(b.s.Registry.Load(), b.project, name, ""); err != nil {
			t.Fatal(err)
		}
	}
	gitOut(t, b.project, "worktree", "add", "-q", "-b", "unmade", tree("unmade"))
	gitOut(t, tree("child"), "commit", "-q", "--allow-empty", "-m", "the child's own")
	in, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: tree("child")})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + tree("parent") + `","deleteBranch":true}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != proto.CodeUnpushedCommits || m["count"] != 1.0 {
		t.Fatalf("the child's commit didn't refuse the delete: %v", m)
	}
	if _, err := os.Stat(tree("parent")); err != nil {
		t.Fatalf("the parent's folder went: %v", err)
	}
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + tree("parent") + `","deleteBranch":true,"force":true}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	for _, name := range []string{"parent", "child"} {
		if _, err := os.Stat(tree(name)); !os.IsNotExist(err) {
			t.Errorf("%s's folder stayed: %v", name, err)
		}
		if gitOut(t, b.project, "branch", "--list", name) != "" {
			t.Errorf("%s's branch stayed", name)
		}
	}
	select {
	case <-in.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("the child's terminal is still open")
	}
	if _, err := os.Stat(tree("unmade")); err != nil {
		t.Fatalf("a worktree Anywhere didn't make went: %v", err)
	}
}

func TestABranchRenamedSinceItWasCheckedGoesOnlyWhenItLosesNothingOrIsForced(t *testing.T) {
	var b bench
	local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "notes")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "notes", ""); err != nil {
		t.Fatal(err)
	}
	if got := branchToDelete(b.project, wt, "notes", false); got != "notes" {
		t.Fatalf("the checked branch, still there: %q", got)
	}
	gitOut(t, wt, "branch", "-m", "say-hello")
	if got := branchToDelete(b.project, wt, "notes", false); got != "say-hello" {
		t.Fatalf("a rename that loses nothing: %q", got)
	}
	gitOut(t, wt, "commit", "-q", "--allow-empty", "-m", "unchecked")
	if got := branchToDelete(b.project, wt, "notes", false); got != "" {
		t.Fatalf("a rename holding an unchecked commit, unforced: %q", got)
	}
	if got := branchToDelete(b.project, wt, "notes", true); got != "say-hello" {
		t.Fatalf("forced: %q", got)
	}
}

func TestAHostThatCannotDeleteASubagentsWorktreeRefusesTheDeleteUnlessForced(t *testing.T) {
	var b bench
	var asked []string
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.Workspace.SubagentTrees = func(string, func(string, int64) string) (orch.Subtrees, error) {
			return orch.Subtrees{Hosts: []orch.HostTree{{Host: "devbox", Path: "/home/dev/wt/notes", Sessions: []string{"hc"}}}}, nil
		}
		s.Workspace.HostDelete = func(_ context.Context, host, path string, subagents []string, _, force bool) error {
			asked = append(asked, fmt.Sprintf("%s %s %v %v", host, path, subagents, force))
			return hosts.Refusal{Reply: proto.Error{Type: "error", Message: "2 commits", Code: proto.CodeUnpushedCommits, Count: 2}}
		}
	})
	tree := filepath.Join(b.root, "pocket", "parent")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "parent", ""); err != nil {
		t.Fatal(err)
	}
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + tree + `"}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != proto.CodeUnpushedCommits || m["count"] != 2.0 {
		t.Fatalf("the host's refusal didn't reach the phone whole: %v", m)
	}
	if _, err := os.Stat(tree); err != nil {
		t.Fatalf("the parent's folder went: %v", err)
	}
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + tree + `","force":true}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if want := []string{"devbox /home/dev/wt/notes [hc] false", "devbox /home/dev/wt/notes [hc] true"}; !slices.Equal(asked, want) {
		t.Fatalf("asked the host %q, want %q", asked, want)
	}
}

func TestAWorktreeMadeAfterItsSubagentStartedIsSomeoneElsesAndStays(t *testing.T) {
	var b bench
	var started int64
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.Workspace.Session = func(id string) (*orch.Session, error) {
			// Its close came after the newer worktree, as a restart closing what it didn't bring back stamps it.
			return &orch.Session{ID: id, Cwd: filepath.Join(b.root, "pocket", "notes"), Status: "closed", CreatedAt: started, ClosedAt: started + 1e6}, nil
		}
	})
	tree := filepath.Join(b.root, "pocket", "notes")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "notes", ""); err != nil {
		t.Fatal(err)
	}
	started = worktree.MadeAt(tree) - 1
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + tree + `","subagents":["sub"]}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != "unknown_worktree" {
		t.Fatalf("a newer worktree at the path wasn't refused: %v", m)
	}
	if _, err := os.Stat(tree); err != nil {
		t.Fatalf("the newer worktree went: %v", err)
	}
	started++
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + tree + `","subagents":["sub"]}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if _, err := os.Stat(tree); !os.IsNotExist(err) {
		t.Fatalf("the worktree made by then stayed: %v", err)
	}
}

func TestASubagentsWorktreeSomeoneElseWorksInStaysWhenItsParentsMacDeletesIt(t *testing.T) {
	var b bench
	var mine string
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.OpenSessions = func() ([]orch.Session, error) {
			return []orch.Session{{ID: "sub"}, {ID: "grand", ParentID: "sub", Relationship: "subagent", Terminal: mine}}, nil
		}
	})
	tree := filepath.Join(b.root, "pocket", "notes")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "notes", ""); err != nil {
		t.Fatal(err)
	}
	theirs, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: tree})
	ours, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: tree})
	mine = ours.Info().ID
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + tree + `","subagents":["sub"]}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != "worktree_in_use" {
		t.Fatalf("another's terminal didn't keep it: %v", m)
	}
	if _, err := os.Stat(tree); err != nil {
		t.Fatalf("the worktree went: %v", err)
	}
	theirs.Close()
	<-theirs.Done()
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + tree + `","subagents":["sub"]}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("its subagent's own terminal kept it: %v", m)
	}
	if _, err := os.Stat(tree); !os.IsNotExist(err) {
		t.Fatalf("the worktree stayed: %v", err)
	}
}

func TestACommitOnlyASubagentsDetachedHeadHoldsRefusesTheDeleteUnlessForced(t *testing.T) {
	var b bench
	tree := func(name string) string { return filepath.Join(b.root, "pocket", name) }
	_, _, p := local(t, aPhone, withWorkspace(t, &b), func(s *Server) {
		s.Workspace.SubagentTrees = func(string, func(string, int64) string) (orch.Subtrees, error) {
			return orch.Subtrees{Local: []string{tree("child")}}, nil
		}
	})
	for _, name := range []string{"parent", "child"} {
		if _, err := worktree.Add(b.s.Registry.Load(), b.project, name, ""); err != nil {
			t.Fatal(err)
		}
	}
	gitOut(t, tree("child"), "checkout", "-q", "--detach")
	gitOut(t, tree("child"), "commit", "-q", "--allow-empty", "-m", "on the detached head alone")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + tree("parent") + `"}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != proto.CodeUnpushedCommits || m["count"] != 1.0 {
		t.Fatalf("the detached commit didn't refuse the delete: %v", m)
	}
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + tree("parent") + `","force":true}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if _, err := os.Stat(tree("child")); !os.IsNotExist(err) {
		t.Fatalf("the child's folder stayed: %v", err)
	}
}

func TestDeletingABranchWithCommitsNowhereElseIsRefusedUnlessForced(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "lone")
	gitOut(t, b.project, "worktree", "add", "-q", "-b", "lone", wt)
	gitOut(t, wt, "commit", "-q", "--allow-empty", "-m", "one")
	gitOut(t, wt, "commit", "-q", "--allow-empty", "-m", "two")
	// A branch holding the first commit keeps it; only the second is on lone alone.
	gitOut(t, b.project, "branch", "keeper", "lone~1")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + wt + `","deleteBranch":true}`)
	if m := p.reply("wd"); m["type"] != "error" || m["code"] != proto.CodeUnpushedCommits || m["count"] != 1.0 {
		t.Fatalf("%v", m)
	}
	if _, err := os.Stat(wt); err != nil {
		t.Fatalf("the folder went: %v", err)
	}
	p.send(`{"type":"worktree.delete","id":"wf","path":"` + wt + `","deleteBranch":true,"force":true}`)
	if m := p.reply("wf"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if gitOut(t, b.project, "branch", "--list", "lone") != "" {
		t.Fatal("the branch stayed")
	}
}

func TestADeletedWorktreeKeepsItsBranchUnlessAsked(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "keep")
	gitOut(t, b.project, "worktree", "add", "-q", "-b", "keep", wt)
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + wt + `"}`)
	p.untilType("ack")
	if gitOut(t, b.project, "branch", "--list", "keep") == "" {
		t.Fatal("the branch went")
	}
}

func TestTheMainWorktreeIsNeverDeletedAndItsTerminalsStayOpen(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	term, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: b.project})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + b.project + `"}`)
	if m, _ := p.untilType("error"); m["code"] != proto.CodeMainWorktree || m["id"] != "wd" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"worktree.delete","id":"wd","path":"/nowhere/else"}`)
	if m, _ := p.untilType("error"); m["code"] != "unknown_worktree" {
		t.Fatalf("%v", m)
	}
	select {
	case <-term.Done():
		t.Fatal("the main worktree's terminal closed")
	case <-time.After(100 * time.Millisecond):
	}
}

func TestBranchesAreListedForATreeOnly(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	gitOut(t, b.project, "branch", "fix-login")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"branch.list","id":"bl","tree":"/etc"}`)
	if m, _ := p.untilType("error"); m["code"] != proto.CodeNotATree || m["id"] != "bl" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"branch.list","id":"bl","tree":"relative"}`)
	if m, _ := p.untilType("error"); m["code"] != proto.CodeNotATree {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"branch.list","id":"bl","tree":"` + b.project + `"}`)
	m, _ := p.untilType("branch.list")
	if m["id"] != "bl" || m["current"] != "main" || len(m["local"].([]any)) != 2 || !reflect.DeepEqual(m["remote"], []any{}) {
		t.Fatalf("%v", m)
	}
}

func TestWorkspaceVerbsNeedTheirScope(t *testing.T) {
	for _, c := range []struct {
		who  peer.Principal
		verb string
		raw  string
	}{
		{aLegacy, "worktree.create", `{"type":"worktree.create","id":"x","requestId":"r","project":"/p","name":"a"}`},
		{aLegacy, "worktree.delete", `{"type":"worktree.delete","id":"x","path":"/p/a"}`},
		{aPTY, "worktree.create", `{"type":"worktree.create","id":"x","requestId":"r","project":"/p","name":"a"}`},
		{aPTY, "agent.account", `{"type":"agent.account","id":"x","agentId":"a1","account":"builtin"}`},
	} {
		var b bench
		_, _, p := local(t, c.who, withWorkspace(t, &b))
		helloWith(p, proto.CapWorkspace)
		p.send(c.raw)
		if m, _ := p.untilType("error"); m["code"] != proto.CodeScopeDenied {
			t.Errorf("%s by %v: %v", c.verb, c.who.Kind, m)
		}
	}
}

func TestAPhoneManagesLocalsWithSpawnOnceItSpeaksWorkspaceV1(t *testing.T) {
	for _, c := range []struct {
		name string
		who  peer.Principal
		caps []string
		code string
	}{
		{"locals only", aPhone, []string{proto.CapLocals}, proto.CodeScopeDenied},
		{"with workspace", aPhone, []string{proto.CapLocals, proto.CapWorkspace}, ""},
		{"without spawn", aLegacy, []string{proto.CapLocals, proto.CapWorkspace}, proto.CodeScopeDenied},
		{"a PTY", aPTY, []string{proto.CapLocals, proto.CapWorkspace}, proto.CodeScopeDenied},
	} {
		var b bench
		_, _, p := local(t, c.who, withWorkspace(t, &b))
		helloWith(p, c.caps...)
		p.send(`{"type":"local.create","id":"lc","localId":"l1","project":"` + b.project + `","name":"Review"}`)
		want := "ack"
		if c.code != "" {
			want = "error"
		}
		if m, _ := p.untilType(want); m["id"] != "lc" || m["code"] != nil && m["code"] != c.code {
			t.Errorf("%s: %v", c.name, m)
		}
		if got := len(b.s.Locals.List()); (got == 1) != (c.code == "") {
			t.Errorf("%s: %d locals", c.name, got)
		}
	}
}

func TestAccountListNamesEachProvidersAccountsOrOnlyOnes(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"account.list","id":"al"}`)
	m, _ := p.untilType("account.list")
	want := []any{
		map[string]any{"id": "builtin", "provider": "claude", "name": "Not signed in", "signedIn": false, "isDefault": true},
		map[string]any{"id": "builtin", "provider": "codex", "name": "Not signed in", "signedIn": false, "isDefault": true},
	}
	if m["id"] != "al" || !reflect.DeepEqual(m["accounts"], want) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"account.list","id":"al","provider":"codex"}`)
	if m, _ := p.untilType("account.list"); !reflect.DeepEqual(m["accounts"], want[1:]) {
		t.Fatalf("%v", m)
	}
}

func TestAnAgentThatCantSwitchAccountsSaysSo(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"agent.account","id":"aa","agentId":"a1","account":"builtin"}`)
	if m, _ := p.untilType("error"); m["id"] != "aa" || m["message"] != "This session can't switch accounts" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.account","id":"aa","agentId":"gone","account":"builtin"}`)
	if m, _ := p.untilType("error"); m["id"] != "aa" {
		t.Fatalf("%v", m)
	}
}

func TestAWorkspacePhoneNamesAWorktreeAndOneWithoutTheCapMayNot(t *testing.T) {
	for _, caps := range [][]string{{proto.CapNames}, {proto.CapNames, proto.CapWorkspace}} {
		var b bench
		_, _, p := local(t, aPhone, withWorkspace(t, &b))
		helloWith(p, caps...)
		p.send(`{"type":"worktree.rename","id":"wr","path":"` + b.project + `","title":"Fix login"}`)
		want := "error"
		if len(caps) == 2 {
			want = "ack"
		}
		if m, _ := p.untilType(want); m["id"] != "wr" {
			t.Fatalf("%v: %v", caps, m)
		}
		if got := b.s.Names.Titles()[b.project]; (got == "Fix login") != (len(caps) == 2) {
			t.Fatalf("%v: %q", caps, got)
		}
	}
}

func TestAPhoneNamesOnlyAFolderInAProject(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapNames, proto.CapWorkspace)
	p.send(`{"type":"worktree.rename","id":"wr","path":"/","title":"Everything"}`)
	if m, _ := p.untilType("error"); m["id"] != "wr" || m["code"] != proto.CodeNotATree {
		t.Fatalf("%v", m)
	}
	if got := b.s.Names.Titles()["/"]; got != "" {
		t.Fatalf("named %q", got)
	}
}

func TestAPeerWithoutWorkspaceV1GetsTheProjectListAsItIsNow(t *testing.T) {
	var b bench
	_, _, p := local(t, peer.OwnerOf(1), withWorkspace(t, &b))
	helloWith(p)
	tree := filepath.Join(b.root, "pocket", "made-outside")
	if _, err := worktree.Add(b.s.Registry.Load(), b.project, "made-outside", ""); err != nil {
		t.Fatal(err)
	}
	p.send(`{"type":"project.list","id":"pl"}`)
	m, _ := p.untilType("project.list")
	if m["id"] != "pl" || !reflect.DeepEqual(treePaths(m), []string{b.project, tree}) {
		t.Fatalf("%v", m)
	}
}
