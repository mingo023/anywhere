package launch

import (
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
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
