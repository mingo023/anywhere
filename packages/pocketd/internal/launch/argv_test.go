package launch

import (
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func spec(provider, access string, plan bool) proto.LaunchSpec {
	return proto.LaunchSpec{Project: "/p", Checkout: proto.Checkout{Worktree: "/w"}, Provider: provider, Access: access, Plan: plan}
}

func TestArgvMatchesTheF14TableForEveryProviderAccessAndPlan(t *testing.T) {
	for _, c := range []struct {
		provider, access string
		plan             bool
		want             string
	}{
		{"claude", "settings", false, "claude"},
		{"claude", "ask", false, "claude --permission-mode default"},
		{"claude", "edits", false, "claude --permission-mode acceptEdits"},
		{"claude", "auto", false, "claude --permission-mode auto"},
		{"claude", "full", false, "claude --permission-mode bypassPermissions --allow-dangerously-skip-permissions"},
		{"claude", "full", true, "claude --permission-mode plan"},
		{"codex", "settings", false, "codex"},
		{"codex", "ask", false, "codex -s read-only -a on-request"},
		{"codex", "edits", false, "codex -s workspace-write -a on-request"},
		{"codex", "auto", false, "codex --approve-for-me"},
		{"codex", "full", false, "codex -s danger-full-access -a never"},
	} {
		argv, f := Argv(spec(c.provider, c.access, c.plan), "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s %s plan=%v: got %q %v, want %q", c.provider, c.access, c.plan, argv, f, c.want)
		}
	}
}

func TestCodexPlanIsNotAllowed(t *testing.T) {
	_, f := Argv(spec("codex", "ask", true), "")
	if f == nil || f.Code != "access_not_allowed" || f.Message != "Codex can't plan first in a terminal session" {
		t.Fatalf("got %+v", f)
	}
}

func TestPromptGoesLastAfterDoubleDash(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Model, s.Effort, s.Prompt = "opus", "high", "-rf fix the flaky tests now please"
	argv, _ := Argv(s, "")
	want := []string{"claude", "--permission-mode", "default", "--model", "opus", "--effort", "high", "-n", "rf-fix-the-flaky", "--", "-rf fix the flaky tests now please"}
	if !slices.Equal(argv, want) {
		t.Fatalf("got %q", argv)
	}
}

func TestANewWorktreeNamesTheSession(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Prompt = "fix it"
	argv, _ := Argv(s, "calm-otter")
	if !slices.Contains(argv, "calm-otter") || slices.Contains(argv, "fix-it") {
		t.Fatalf("got %q", argv)
	}
}

func TestABadModelOrEffortIsAnInvalidSpec(t *testing.T) {
	for _, s := range []proto.LaunchSpec{
		{Provider: "claude", Access: "ask", Model: "opus; rm -rf"},
		{Provider: "claude", Access: "ask", Effort: "extreme"},
		{Provider: "codex", Access: "ask", Effort: "high"},
	} {
		if _, f := Argv(s, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%+v: got %+v", s, f)
		}
	}
}

func TestNoAccessEverPassesFullAuto(t *testing.T) {
	for _, provider := range []string{"claude", "codex"} {
		for _, access := range proto.Accesses {
			for _, plan := range []bool{false, true} {
				if argv, _ := Argv(spec(provider, access, plan), ""); slices.Contains(argv, "--full-auto") {
					t.Errorf("%s %s plan=%v: got %q", provider, access, plan, argv)
				}
			}
		}
	}
}

func TestControlBytesAreStrippedFromThePrompt(t *testing.T) {
	s := spec("codex", "ask", false)
	s.Prompt = "fix\x1b[2J it\x03\x7f\n\tnow"
	argv, _ := Argv(s, "")
	if got := argv[len(argv)-1]; got != "fix[2J it\n\tnow" {
		t.Fatalf("prompt = %q", got)
	}
}

func TestAModelThatLooksLikeAFlagIsAnInvalidSpec(t *testing.T) {
	for _, m := range []string{"-c", "--dangerously-bypass-approvals-and-sandbox"} {
		if _, f := Argv(proto.LaunchSpec{Provider: "codex", Access: "ask", Model: m}, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%q: got %+v", m, f)
		}
	}
}
