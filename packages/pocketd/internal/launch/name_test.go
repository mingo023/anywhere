package launch

import (
	"encoding/json"
	"errors"
	"os/exec"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
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
	return l, a, job{provider: "claude", prompt: "fix the login", agent: "a1", project: s.Project, tree: c.Path, branch: "calm-otter"}
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

func current(t *testing.T, path string) string {
	out, _ := exec.Command("git", "-C", path, "branch", "--show-current").Output()
	return strings.TrimSpace(string(out))
}

func TestNamingTitlesTheSessionAndWorktreeAndRenamesTheBranch(t *testing.T) {
	l, a, j := placed(t)
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	l.name(j)
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
	l.name(j)
	if len(*prompts) != 2 || !strings.Contains((*prompts)[1], "<agent-reply>\nI'll fix the login form\n</agent-reply>") {
		t.Fatalf("%q", *prompts)
	}
	if a.Summary().Title != "Fix login form" || current(t, j.tree) != "fix-login-form" {
		t.Fatalf("%+v", a.Summary())
	}
}

func TestNamingGivesUpAfterThreeFailuresAndSaysSo(t *testing.T) {
	l, _, j := placed(t)
	msgs, stop := l.Names.Subscribe()
	defer stop()
	prompts := answers(l, errors.New("boom"))
	l.name(j)
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
	l.name(j)
	if l.Names.Titles()[j.tree] != "Mine" || a.Summary().Title != "Fix login" {
		t.Fatalf("%v %q", l.Names.Titles(), a.Summary().Title)
	}
}

func TestAPushedPlaceholderKeepsItsBranch(t *testing.T) {
	l, _, j := placed(t)
	exec.Command("git", "-C", j.project, "update-ref", "refs/remotes/origin/calm-otter", "HEAD").Run()
	answers(l, naming.Result{Title: "Fix login", Branch: "fix-login"})
	l.name(j)
	if current(t, j.tree) != "calm-otter" || l.Names.Titles()[j.tree] != "Fix login" {
		t.Fatal(current(t, j.tree))
	}
}
