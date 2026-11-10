package main

import (
	"errors"
	"flag"
	"io"
	"reflect"
	"strings"
	"testing"
	"unicode/utf8"

	"pocketd/internal/proto"
)

func TestPositionalTakesFlagsOnEitherSideOfTheArguments(t *testing.T) {
	flags := flag.NewFlagSet("t", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	timeout := flags.String("timeout", "", "")
	pos, err := positional(flags, []string{"abc", "--timeout", "10m"})
	if err != nil || *timeout != "10m" || !reflect.DeepEqual(pos, []string{"abc"}) {
		t.Fatalf("pos = %v, timeout = %s, err = %v", pos, *timeout, err)
	}
}

func TestLastReplyIsWhatCameAfterTheLastUserMessage(t *testing.T) {
	items := []timelineItem{{"user", "one"}, {"assistant", "old"}, {"user", "two"}, {"tool", ""}, {"assistant", "first"}, {"assistant", "second"}}
	if got := lastReply(items); got != "first\n\nsecond" {
		t.Fatalf("got %q", got)
	}
}

func TestAnUnknownVerbIsAUsageError(t *testing.T) {
	if _, err := anywhere([]string{"session", "frob"}, io.Discard); !errors.Is(err, errUsage) {
		t.Fatalf("err = %v", err)
	}
}

var twoProjects = []proto.Project{
	{Path: "/code/api", Name: "api", Worktrees: []proto.Worktree{{Name: "api", Path: "/code/api", IsMain: true}, {Name: "calm-otter", Path: "/wt/calm-otter"}}},
	{Path: "/code/web", Name: "web", Worktrees: []proto.Worktree{{Name: "web", Path: "/code/web", IsMain: true}}},
}

func TestLaunchSpecFindsTheProjectHoldingTheFolderAndItsMainWorktree(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/wt/calm-otter/src", newSession{provider: "claude", access: "settings"})
	if err != nil || spec.Project != "/code/api" || spec.Checkout.Worktree != "/code/api" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecTakesAProjectByNameAndAWorktreeByName(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/elsewhere", newSession{project: "api", worktree: "calm-otter"})
	if err != nil || spec.Checkout.Worktree != "/wt/calm-otter" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecAsksForANewWorktreeByName(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: freshWorktree{wanted: true, name: "calm-otter"}})
	if err != nil || spec.Project != "/code/web" || spec.Checkout.New == nil || spec.Checkout.New.Name != "calm-otter" || spec.Checkout.New.AutoName || spec.Checkout.Worktree != "" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecHasPocketdNameAnUnnamedNewWorktreeFromThePrompt(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: freshWorktree{wanted: true}, prompt: "fix the login"})
	if n := spec.Checkout.New; err != nil || n == nil || !n.AutoName || !strings.HasPrefix(n.Name, "cli-") {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecKeepsTheNameOfAnUnnamedNewWorktreeWithoutAPrompt(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: freshWorktree{wanted: true}})
	if n := spec.Checkout.New; err != nil || n == nil || n.AutoName || !strings.HasPrefix(n.Name, "cli-") {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecStartsANewWorktreeFromTheBaseGiven(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: freshWorktree{wanted: true}, base: "dev"})
	if err != nil || spec.Checkout.New == nil || spec.Checkout.New.Base != "dev" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecStartsAChat(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{})
	if err != nil || !spec.Chat {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func parseCreateArgs(args ...string) (newSession, error) {
	flags := flag.NewFlagSet("t", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	return parseCreate(flags, args)
}

func TestANewWorktreeTakesTheNameAfterIt(t *testing.T) {
	n, err := parseCreateArgs("--new-worktree", "fix-login", "--prompt", "hi")
	if err != nil || n.newWorktree != (freshWorktree{wanted: true, name: "fix-login"}) || n.prompt != "hi" {
		t.Fatalf("n = %+v, err = %v", n, err)
	}
}

func TestANewWorktreeNeedsNoName(t *testing.T) {
	for _, args := range [][]string{{"--new-worktree", "--prompt", "hi"}, {"--prompt", "hi", "--new-worktree"}} {
		if n, err := parseCreateArgs(args...); err != nil || n.newWorktree != (freshWorktree{wanted: true}) || n.prompt != "hi" {
			t.Errorf("%v: n = %+v, err = %v", args, n, err)
		}
	}
}

func TestCreateRefusesFlagsThatDontFit(t *testing.T) {
	for _, args := range [][]string{
		{"--base", "dev"},
		{"--worktree", "calm-otter", "--base", "dev"},
		{"--worktree", "calm-otter", "--new-worktree"},
		{"--worktree", ""},
		{"--new-worktree", ""},
		{"--new-worktree="},
		{"--new-worktree", "--base", ""},
		{"stray"},
	} {
		if _, err := parseCreateArgs(args...); !errors.Is(err, errUsage) {
			t.Errorf("%v: err = %v", args, err)
		}
	}
}

func parseTicketArgs(args ...string) (newTicket, error) {
	flags := flag.NewFlagSet("t", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	return parseTicket(flags, args)
}

func TestATicketTakesItsFlagsOnEitherSideOfThePrompt(t *testing.T) {
	n, err := parseTicketArgs("Fix the login", "--project", "api", "--start", "--base", "dev", "--title", "Login")
	want := newTicket{project: "api", provider: "claude", base: "dev", title: "Login", prompt: "Fix the login", start: true}
	if err != nil || n != want {
		t.Fatalf("n = %+v, err = %v", n, err)
	}
}

func TestATicketNeedsExactlyOnePrompt(t *testing.T) {
	for _, args := range [][]string{{}, {"one", "two"}} {
		if _, err := parseTicketArgs(args...); !errors.Is(err, errUsage) {
			t.Errorf("%v: err = %v", args, err)
		}
	}
}

func TestATicketGoesToTheProjectHoldingTheFolder(t *testing.T) {
	got, err := ticketFor(twoProjects, "/wt/calm-otter/src", newTicket{provider: "codex", base: "dev", prompt: "Fix it"})
	want := proto.Ticket{Title: "Fix it", Prompt: "Fix it", Provider: "codex", Folder: "/code/api", Base: "dev"}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got = %+v, err = %v", got, err)
	}
}

func TestATicketGoesToTheProjectNamed(t *testing.T) {
	if got, err := ticketFor(twoProjects, "/elsewhere", newTicket{project: "web", provider: "claude", prompt: "Fix it"}); err != nil || got.Folder != "/code/web" {
		t.Fatalf("got = %+v, err = %v", got, err)
	}
}

func TestATicketWithNoTitleIsNamedForThePromptsFirstLine(t *testing.T) {
	if got, _ := ticketFor(twoProjects, "/code/web", newTicket{provider: "claude", prompt: "\n  Fix the login \n\nIt redirects twice\n"}); got.Title != "Fix the login" || got.Prompt != "Fix the login \n\nIt redirects twice" {
		t.Fatalf("got = %+v", got)
	}
}

func TestALongFirstLineIsCutToATitle(t *testing.T) {
	got, _ := ticketFor(twoProjects, "/code/web", newTicket{provider: "claude", prompt: strings.Repeat("é", proto.MaxTicketTitle+5)})
	if n := utf8.RuneCountInString(got.Title); n != proto.MaxTicketTitle {
		t.Fatalf("title is %d long", n)
	}
}

func TestATitleGivenBeatsThePrompt(t *testing.T) {
	if got, _ := ticketFor(twoProjects, "/code/web", newTicket{provider: "claude", title: "Login", prompt: "Fix it"}); got.Title != "Login" {
		t.Fatalf("got = %+v", got)
	}
}

func TestATicketNeedsAPromptAKnownAgentAndAShortEnoughTitle(t *testing.T) {
	for _, n := range []newTicket{
		{provider: "claude", prompt: "   "},
		{provider: "frob", prompt: "Fix it"},
		{provider: "claude", prompt: "Fix it", title: strings.Repeat("a", proto.MaxTicketTitle+1)},
	} {
		if _, err := ticketFor(twoProjects, "/code/web", n); err == nil || !strings.Contains(err.Error(), "a ticket needs") {
			t.Errorf("%+v: err = %v", n, err)
		}
	}
}

func TestLaunchSpecRefusesAFolderOutsideEveryProject(t *testing.T) {
	if _, err := launchSpec(twoProjects, "/code/apiary", newSession{}); err == nil || !strings.Contains(err.Error(), "--project") {
		t.Fatalf("err = %v", err)
	}
}

func TestLaunchSpecRefusesANameTwoProjectsShare(t *testing.T) {
	list := []proto.Project{{Path: "/a/x", Name: "x"}, {Path: "/b/x", Name: "x"}}
	if _, err := launchSpec(list, "/", newSession{project: "x"}); err == nil || !strings.Contains(err.Error(), "path") {
		t.Fatalf("err = %v", err)
	}
}

func TestOutcomeTellsEachWayATurnStops(t *testing.T) {
	for _, c := range []struct {
		a    proto.AgentSummary
		code int
		word string
		over bool
	}{
		{proto.AgentSummary{Status: "working"}, 0, "", false},
		{proto.AgentSummary{Status: "needsYou"}, exitNeedsYou, "needsYou", true},
		{proto.AgentSummary{Status: "closed"}, exitClosed, "closed", true},
		{proto.AgentSummary{Status: "done", Failed: true}, exitFailed, "failed", true},
		{proto.AgentSummary{Status: "done"}, 0, "done", true},
		{proto.AgentSummary{Status: "idle"}, 0, "done", true},
	} {
		if code, word, over := outcome(c.a); code != c.code || word != c.word || over != c.over {
			t.Errorf("%s: got %d %q %v", c.a.Status, code, word, over)
		}
	}
}

func parseEditArgs(t *testing.T, args ...string) (string, map[string]string, error) {
	t.Helper()
	flags := flag.NewFlagSet("anywhere", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	return parseTicketEdit(flags, args)
}

func TestAnEditTakesOnlyTheFieldsItSets(t *testing.T) {
	id, changes, err := parseEditArgs(t, "abc", "--title", "New", "--base", "")
	if err != nil || id != "abc" || !reflect.DeepEqual(changes, map[string]string{"title": "New", "base": ""}) {
		t.Fatalf("got %q %v %v", id, changes, err)
	}
}

func TestAnEditNeedsATicketAndSomethingToChange(t *testing.T) {
	for _, args := range [][]string{{"abc"}, {"--title", "New"}, {"--title", "New", "abc", "def"}} {
		if _, _, err := parseEditArgs(t, args...); !errors.Is(err, errUsage) {
			t.Fatalf("%v: got %v", args, err)
		}
	}
}

func TestAnEditLeavesTheOtherFieldsAlone(t *testing.T) {
	was := proto.Ticket{ID: "abc", Title: "Old", Prompt: "do it", Provider: "claude", Folder: "/repo", Base: "main"}
	got, err := editTicket(was, map[string]string{"title": "New"})
	want := was
	want.Title = "New"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v %v", got, err)
	}
}

func TestAnEditCanClearTheBase(t *testing.T) {
	got, err := editTicket(proto.Ticket{Title: "T", Prompt: "p", Provider: "claude", Folder: "/repo", Base: "main"}, map[string]string{"base": ""})
	if err != nil || got.Base != "" {
		t.Fatalf("got %+v %v", got, err)
	}
}

func TestAnEditCannotLeaveATicketInvalid(t *testing.T) {
	was := proto.Ticket{Title: "T", Prompt: "p", Provider: "claude", Folder: "/repo"}
	for _, changes := range []map[string]string{{"title": " "}, {"prompt": ""}, {"provider": "nope"}, {"base": "-x"}} {
		if _, err := editTicket(was, changes); !errors.Is(err, errBadTicket) {
			t.Fatalf("%v: got %v", changes, err)
		}
	}
}

func TestALiveTicketShowsWhatItsSessionIsDoing(t *testing.T) {
	if got := ticketState(proto.Ticket{State: proto.TicketLive, Status: proto.TicketNeedsYou}); got != proto.TicketNeedsYou {
		t.Fatalf("got %q", got)
	}
	if got := ticketState(proto.Ticket{State: proto.TicketTodo}); got != proto.TicketTodo {
		t.Fatalf("got %q", got)
	}
}
