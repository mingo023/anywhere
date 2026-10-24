package wsserver

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/devices"
	"pocketd/internal/launch"
	"pocketd/internal/locals"
	"pocketd/internal/names"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/workspace"
)

var (
	aPhone  = peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	aLegacy = peer.FromDevice(devices.Device{ID: devices.LegacyID, Scopes: devices.LegacyScopes})
	aPTY    = peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}
)

func gitIn(t *testing.T, dir string, args ...string) string {
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
	gitIn(t, b.project, "init", "-q", "-b", "main")
	gitIn(t, b.project, "commit", "-q", "--allow-empty", "-m", "init")
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

// until reads messages until one of type, returning those before it too.
func (p *phone) until(typ string) (map[string]any, []map[string]any) {
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
	created, before := p.until("worktree.created")
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
	if m := p.recv(); m["id"] != "pl" || len(treePaths(m)) != 2 {
		t.Fatalf("%v", m)
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
	created, before := p.until("worktree.created")
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
		m, _ := p.until("error")
		if code != "" && m["code"] != code || m["id"] != "x" {
			t.Errorf("%s: %v", raw, m)
		}
	}
}

func TestDeletingAWorktreeClosesItsTerminalsForgetsItsNameAndMayDeleteItsBranch(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "calm-otter")
	gitIn(t, b.project, "worktree", "add", "-q", "-b", "calm-otter", wt)
	b.s.Names.Rename(wt, "Fix login")
	in, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: filepath.Join(wt)})
	out, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: b.project})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + wt + `","deleteBranch":true}`)
	if m, _ := p.until("ack"); m["id"] != "wd" {
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
	if gitIn(t, b.project, "branch", "--list", "calm-otter") != "" {
		t.Fatal("the branch stayed")
	}
	if _, ok := b.s.Names.Titles()[wt]; ok {
		t.Fatal("the name stayed")
	}
	if list, _ := b.s.Workspace.Projects.List(); len(list[0].Worktrees) != 1 {
		t.Fatalf("the cache still lists it: %+v", list)
	}
}

func TestADeletedWorktreeKeepsItsBranchUnlessAsked(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	wt := filepath.Join(b.root, "pocket", "keep")
	gitIn(t, b.project, "worktree", "add", "-q", "-b", "keep", wt)
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + wt + `"}`)
	p.until("ack")
	if gitIn(t, b.project, "branch", "--list", "keep") == "" {
		t.Fatal("the branch went")
	}
}

func TestTheMainWorktreeIsNeverDeletedAndItsTerminalsStayOpen(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	term, _ := b.terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: b.project})
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"worktree.delete","id":"wd","path":"` + b.project + `"}`)
	if m, _ := p.until("error"); m["code"] != proto.CodeMainWorktree || m["id"] != "wd" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"worktree.delete","id":"wd","path":"/nowhere/else"}`)
	if m, _ := p.until("error"); m["code"] != "unknown_worktree" {
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
	gitIn(t, b.project, "branch", "fix-login")
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"branch.list","id":"bl","tree":"/etc"}`)
	if m, _ := p.until("error"); m["code"] != proto.CodeNotATree || m["id"] != "bl" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"branch.list","id":"bl","tree":"relative"}`)
	if m, _ := p.until("error"); m["code"] != proto.CodeNotATree {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"branch.list","id":"bl","tree":"` + b.project + `"}`)
	m, _ := p.until("branch.list")
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
		if m, _ := p.until("error"); m["code"] != proto.CodeScopeDenied {
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
		if m, _ := p.until(want); m["id"] != "lc" || m["code"] != nil && m["code"] != c.code {
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
	m, _ := p.until("account.list")
	want := []any{
		map[string]any{"id": "builtin", "provider": "claude", "name": "Not signed in", "signedIn": false, "isDefault": true},
		map[string]any{"id": "builtin", "provider": "codex", "name": "Not signed in", "signedIn": false, "isDefault": true},
	}
	if m["id"] != "al" || !reflect.DeepEqual(m["accounts"], want) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"account.list","id":"al","provider":"codex"}`)
	if m, _ := p.until("account.list"); !reflect.DeepEqual(m["accounts"], want[1:]) {
		t.Fatalf("%v", m)
	}
}

func TestAnAgentThatCantSwitchAccountsSaysSo(t *testing.T) {
	var b bench
	_, _, p := local(t, aPhone, withWorkspace(t, &b))
	helloWith(p, proto.CapWorkspace)
	p.send(`{"type":"agent.account","id":"aa","agentId":"a1","account":"builtin"}`)
	if m, _ := p.until("error"); m["id"] != "aa" || m["message"] != "This session can't switch accounts" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.account","id":"aa","agentId":"gone","account":"builtin"}`)
	if m, _ := p.until("error"); m["id"] != "aa" {
		t.Fatalf("%v", m)
	}
}

func TestAWorkspacePhoneNamesAWorktreeAndOneWithoutTheCapMayNot(t *testing.T) {
	for _, caps := range [][]string{{proto.CapNames}, {proto.CapNames, proto.CapWorkspace}} {
		var b bench
		_, _, p := local(t, aPhone, withWorkspace(t, &b))
		helloWith(p, caps...)
		p.send(`{"type":"worktree.rename","id":"wr","path":"/w/calm-otter","title":"Fix login"}`)
		want := "error"
		if len(caps) == 2 {
			want = "ack"
		}
		if m, _ := p.until(want); m["id"] != "wr" {
			t.Fatalf("%v: %v", caps, m)
		}
		if got := b.s.Names.Titles()["/w/calm-otter"]; (got == "Fix login") != (len(caps) == 2) {
			t.Fatalf("%v: %q", caps, got)
		}
	}
}
