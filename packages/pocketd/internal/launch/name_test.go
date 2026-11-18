package launch

import (
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/agent"
	"pocketd/internal/envdir"
	"pocketd/internal/locals"
	"pocketd/internal/names"
	"pocketd/internal/naming"
	"pocketd/internal/timeline"
	"pocketd/internal/worktree"
)

func placed(t *testing.T) (*Launcher, *agent.Agent, job) {
	t.Helper()
	l, s := newWorktree(t, "")
	c, err := worktree.Add(l.reg.Load(), s.Project, "calm-otter", "")
	if err != nil {
		t.Fatal(err)
	}
	l.Names = names.Open(t.TempDir())
	a := l.d.Agents.Add("a1", c.Path, "claude", nil)
	return l, a, job{provider: "claude", prompt: "fix the login", project: s.Project, tree: c.Path, branch: "calm-otter"}
}

func answers(l *Launcher, out ...any) *[]string {
	var prompts []string
	l.generate = func(_, _, prompt string, _ []string) (naming.Result, error) {
		prompts = append(prompts, prompt)
		switch v := out[min(len(prompts), len(out))-1].(type) {
		case naming.Result:
			return v, nil
		default:
			return naming.Result{}, v.(error)
		}
	}
	return &prompts
}

// name runs j on a fresh claim on a's Conversation.
func name(l *Launcher, a *agent.Agent, j job) {
	claim, _ := a.ClaimNaming()
	l.name(a, claim, j)
}

func current(t *testing.T, path string) string {
	out, _ := exec.Command("git", "-C", path, "branch", "--show-current").Output()
	return strings.TrimSpace(string(out))
}

func TestNamingTitlesTheSessionAndWorktreeAndRenamesTheBranch(t *testing.T) {
	l, a, j := placed(t)
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	name(l, a, j)
	if a.Summary().Title != "Fix login" || l.Names.Titles()[j.tree] != "Fix login" || current(t, j.tree) != "fix-login" || a.Summary().Branch != "fix-login" {
		t.Fatalf("%+v %v %s", a.Summary(), l.Names.Titles(), current(t, j.tree))
	}
}

func TestAVaguePromptIsNamedAgainFromTheAgentsFirstReply(t *testing.T) {
	old := replyPoll
	replyPoll = time.Millisecond
	t.Cleanup(func() { replyPoll = old })
	l, a, j := placed(t)
	a.Record(timeline.Event{Kind: "assistant_text", Text: "I'll fix the login form"})
	a.TurnEnded(false)
	prompts := answers(l, naming.Result{Title: "Hello", Branch: "hello", Vague: true}, naming.Result{Title: "Fix login form", Branch: "fix-login-form"})
	name(l, a, j)
	if len(*prompts) != 2 || !strings.Contains((*prompts)[1], "<agent-reply>\nI'll fix the login form\n</agent-reply>") {
		t.Fatalf("%q", *prompts)
	}
	if a.Summary().Title != "Fix login form" || current(t, j.tree) != "fix-login-form" {
		t.Fatalf("%+v", a.Summary())
	}
}

func TestNamingGivesUpAfterThreeFailuresAndSaysSo(t *testing.T) {
	l, a, j := placed(t)
	msgs, stop := l.Names.Subscribe()
	defer stop()
	prompts := answers(l, errors.New("boom"))
	name(l, a, j)
	if len(*prompts) != 3 || current(t, j.tree) != "calm-otter" || len(l.Names.Titles()) != 0 {
		t.Fatalf("%d tries, branch %s", len(*prompts), current(t, j.tree))
	}
	var m map[string]any
	json.Unmarshal(<-msgs, &m)
	if m["type"] != "naming.failed" || m["agentId"] != "a1" {
		t.Fatalf("%v", m)
	}
}

func TestAnAgentWithNoHeadlessRunIsNamedByItsOwnTitleOrLeftUnnamedWithNoFailure(t *testing.T) {
	old := replyPoll
	replyPoll = time.Millisecond
	t.Cleanup(func() { replyPoll = old })
	l, _, j := placed(t)
	l.Locals = locals.Open(t.TempDir())
	l.Locals.Create("l1", j.project, "Local 1")
	l.Locals.Create("l2", j.project, "Local 2")
	msgs, stop := l.Names.Subscribe()
	defer stop()

	hermes := l.d.Agents.Add("a2", j.tree, "hermes", nil)
	hermes.Record(timeline.Event{Kind: "user", Text: "reply with the single word ok"})
	time.AfterFunc(20*time.Millisecond, func() { hermes.SetTitle("Reply with ok") })
	j.provider, j.local = "hermes", "l1"
	name(l, hermes, j)
	if got, _ := l.Locals.Get("l1"); got.Name != "Reply with ok" || l.Locals.Auto("l1", "Later") || l.Names.Titles()[j.tree] != "Reply with ok" || current(t, j.tree) != "calm-otter" {
		t.Fatalf("%+v %v %s", got, l.Names.Titles(), current(t, j.tree))
	}

	pi := l.d.Agents.Add("a3", j.project, "pi", nil)
	pi.Record(timeline.Event{Kind: "user", Text: "say hi"})
	time.AfterFunc(20*time.Millisecond, func() { l.d.Agents.Remove("a3") })
	name(l, pi, job{provider: "pi", prompt: "say hi", local: "l2"})
	if got, _ := l.Locals.Get("l2"); got.Name != "Local 2" || !l.Locals.Auto("l2", "Later") {
		t.Fatalf("%+v", got)
	}
	for len(msgs) > 0 {
		if m := <-msgs; strings.Contains(string(m), "naming.failed") {
			t.Fatalf("%s", m)
		}
	}
}

func TestARenamedWorktreeKeepsTheUsersName(t *testing.T) {
	l, a, j := placed(t)
	l.Names.Rename(j.tree, "Mine")
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	name(l, a, j)
	if l.Names.Titles()[j.tree] != "Mine" || a.Summary().Title != "Fix login" {
		t.Fatalf("%v %q", l.Names.Titles(), a.Summary().Title)
	}
}

func TestAPushedPlaceholderKeepsItsBranch(t *testing.T) {
	l, a, j := placed(t)
	exec.Command("git", "-C", j.project, "update-ref", "refs/remotes/origin/calm-otter", "HEAD").Run()
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	name(l, a, j)
	if current(t, j.tree) != "calm-otter" || l.Names.Titles()[j.tree] != "Fix login" {
		t.Fatal(current(t, j.tree))
	}
}

func TestNamingTitlesTheLocalTheSessionStartedInOnlyOnce(t *testing.T) {
	l, a, j := placed(t)
	l.Locals = locals.Open(t.TempDir())
	l.Locals.Create("l1", j.project, "Local 2")
	j.tree, j.local = "", "l1"
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"}, naming.Result{Title: "Add tests", Branch: "add-tests"})
	name(l, a, j)
	name(l, a, j)
	if got, _ := l.Locals.Get("l1"); got.Name != "Fix login" || a.Summary().Title != "Add tests" || current(t, j.project) == "fix-login" {
		t.Fatalf("%q %+v", got.Name, a.Summary())
	}
}

func TestATypedPromptNamesTheSessionOncePerConversation(t *testing.T) {
	l, a, _ := placed(t)
	prompts := answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"}, naming.Result{Title: "Add tests", Branch: "add-tests"})
	a.SetConversation("c1")
	l.Prompted(a, "fix the login", os.Environ())
	eventually(t, func() bool { return a.Summary().Title == "Fix login" })
	l.Prompted(a, "and add tests", os.Environ())
	a.SetConversation("c2")
	l.Prompted(a, "add tests", os.Environ())
	eventually(t, func() bool { return a.Summary().Title == "Add tests" })
	if len(*prompts) != 2 {
		t.Fatalf("%q", *prompts)
	}
}

func TestAPromptTypedWhileItsSessionIsCreatedIsLeftToTheCreate(t *testing.T) {
	l, a, _ := placed(t)
	prompts := answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	a.SetTerminal("t1")
	l.pending["t1"] = make(chan exit)
	l.Prompted(a, "fix the login", os.Environ())
	if _, ok := a.ClaimNaming(); !ok || len(*prompts) != 0 {
		t.Fatalf("the prompt claimed the session: %q", *prompts)
	}
}

func eventually(t *testing.T, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(10 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatal("timed out")
}

// twoAccounts signs claude in on the built-in account and on work, whose config dir it returns.
func twoAccounts(t *testing.T, l *Launcher) string {
	t.Helper()
	s := &accounts.Store{Root: t.TempDir(), Home: t.TempDir()}
	work := filepath.Join(s.Root, "claude", "work")
	for _, dir := range []string{filepath.Join(s.Home, ".claude"), work} {
		os.MkdirAll(dir, 0o700)
		os.WriteFile(filepath.Join(dir, ".credentials.json"), []byte(`{"claudeAiOauth":{"accessToken":"tok-`+filepath.Base(dir)+`"}}`), 0o600)
	}
	os.WriteFile(filepath.Join(s.Root, "accounts.json"), []byte(`{"accounts":[{"id":"work","provider":"claude"}]}`), 0o600)
	l.d.Accounts = s
	return work
}

// refusing has naming refuse at a limit on the config dirs in limited, "" being
// the built-in account's, and returns the dir each run used.
func refusing(l *Launcher, limited ...string) *[]string {
	dirs := &[]string{}
	l.generate = func(_, _, _ string, env []string) (naming.Result, error) {
		dir := envdir.Value(env, "CLAUDE_CONFIG_DIR")
		*dirs = append(*dirs, dir)
		if slices.Contains(limited, dir) {
			return naming.Result{}, naming.ErrLimit
		}
		return naming.Result{Title: "Fix login", Branch: "fix-login"}, nil
	}
	return dirs
}

func TestNamingAtItsAccountsLimitMovesToAnotherSignedInAccount(t *testing.T) {
	l, a, j := placed(t)
	work := twoAccounts(t, l)
	dirs := refusing(l, "")
	j.exe = "/bin/claude"
	name(l, a, j)
	if a.Summary().Title != "Fix login" || current(t, j.tree) != "fix-login" || !slices.Equal(*dirs, []string{"", work}) {
		t.Fatalf("%+v %q", a.Summary(), *dirs)
	}

	l, a, j = placed(t)
	work = twoAccounts(t, l)
	dirs = refusing(l, work+"/")
	j.exe = "/bin/claude"
	j.env = []string{"HOME=/h", "CLAUDE_CONFIG_DIR=" + work + "/"}
	name(l, a, j)
	if a.Summary().Title != "Fix login" || !slices.Equal(*dirs, []string{work + "/", ""}) {
		t.Fatalf("to the built-in account: %+v %q", a.Summary(), *dirs)
	}
}

func TestNamingMovedToAManagedAccountDropsTheLoginThatWouldOverrideIt(t *testing.T) {
	l, a, j := placed(t)
	twoAccounts(t, l)
	var envs [][]string
	l.generate = func(_, _, _ string, env []string) (naming.Result, error) {
		envs = append(envs, env)
		if len(envs) == 1 {
			return naming.Result{}, naming.ErrLimit
		}
		return naming.Result{Title: "Fix login"}, nil
	}
	j.exe, j.env = "/bin/claude", []string{"HOME=/h", "CLAUDE_CODE_OAUTH_TOKEN=t"}
	name(l, a, j)
	if len(envs) != 2 || slices.Contains(envs[1], "CLAUDE_CODE_OAUTH_TOKEN=t") || !slices.Contains(envs[1], "HOME=/h") {
		t.Fatalf("%q", envs)
	}
}

func TestNamingFailsWhenEveryAccountIsAtItsLimitOrSwitchingIsOff(t *testing.T) {
	for _, auto := range []bool{true, false} {
		l, a, j := placed(t)
		j.exe = "/bin/claude"
		work := twoAccounts(t, l)
		limited := []string{""}
		if auto {
			limited = append(limited, work)
		}
		dirs := refusing(l, limited...)
		l.d.Accounts.SetAutoSwitch(auto)
		msgs, stop := l.Names.Subscribe()
		name(l, a, j)
		var m map[string]any
		json.Unmarshal(<-msgs, &m)
		stop()
		if a.Summary().Title != "" || m["type"] != "naming.failed" || !auto && slices.Contains(*dirs, work) {
			t.Fatalf("auto %v: %+v %v %q", auto, a.Summary(), m, *dirs)
		}
	}
}

// The user's rc runs before a shell function, and can set the account back.
func TestNamingThroughAShellFunctionStaysOnItsAccount(t *testing.T) {
	l, a, j := placed(t)
	work := twoAccounts(t, l)
	dirs := refusing(l, "")
	j.exe = "claude"
	name(l, a, j)
	if a.Summary().Title != "" || slices.Contains(*dirs, work) {
		t.Fatalf("%+v %q", a.Summary(), *dirs)
	}
}
