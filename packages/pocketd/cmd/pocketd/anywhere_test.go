package main

import (
	"errors"
	"flag"
	"io"
	"reflect"
	"strings"
	"testing"

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
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: "calm-otter"})
	if err != nil || spec.Project != "/code/web" || spec.Checkout.New == nil || spec.Checkout.New.Name != "calm-otter" || spec.Checkout.Worktree != "" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
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
