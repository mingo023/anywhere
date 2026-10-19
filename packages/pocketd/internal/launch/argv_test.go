package launch

import (
	"slices"
	"strings"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

func argvOf(s proto.LaunchSpec, name string) ([]string, *Failure) {
	return Argv(s, name, config.DefaultArgs(s.Provider))
}

func spec(provider, access string, plan bool) proto.LaunchSpec {
	return proto.LaunchSpec{Project: "/p", Checkout: proto.Checkout{Worktree: "/w"}, Provider: provider, Access: access, Plan: plan}
}

func TestArgvMatchesTheF14TableForEveryProviderAccessAndPlan(t *testing.T) {
	for _, c := range []struct {
		provider, access string
		plan, chat       bool
		want             string
	}{
		{"claude", "settings", false, false, "claude"},
		{"claude", "ask", false, false, "claude --permission-mode default"},
		{"claude", "edits", false, false, "claude --permission-mode acceptEdits"},
		{"claude", "auto", false, false, "claude --permission-mode auto"},
		{"claude", "full", false, false, "claude --permission-mode bypassPermissions --allow-dangerously-skip-permissions"},
		{"claude", "full", true, false, "claude --permission-mode plan"},
		{"codex", "settings", false, false, "codex"},
		{"codex", "ask", false, false, "codex -s read-only -a on-request"},
		{"codex", "edits", false, false, "codex -s workspace-write -a on-request"},
		{"codex", "auto", false, false, "codex --approve-for-me"},
		{"codex", "full", false, false, "codex -s danger-full-access -a never"},
		{"claude", "settings", false, true, "claude -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"claude", "ask", false, true, "claude --permission-mode default -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"claude", "edits", false, true, "claude --permission-mode acceptEdits -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"claude", "auto", false, true, "claude --permission-mode auto -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"claude", "full", false, true, "claude --permission-mode bypassPermissions -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"claude", "full", true, true, "claude --permission-mode plan -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"codex", "settings", false, true, "codex app-server"},
		{"codex", "ask", false, true, `codex app-server -c sandbox_mode="read-only" -c approval_policy="on-request"`},
		{"codex", "edits", false, true, `codex app-server -c sandbox_mode="workspace-write" -c approval_policy="on-request"`},
		{"codex", "auto", false, true, `codex app-server -c sandbox_mode="workspace-write" -c approvals_reviewer="auto_review"`},
		{"codex", "full", false, true, `codex app-server -c sandbox_mode="danger-full-access" -c approval_policy="never"`},
	} {
		s := spec(c.provider, c.access, c.plan)
		s.Chat = c.chat
		argv, f := argvOf(s, "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s %s plan=%v chat=%v: got %q %v, want %q", c.provider, c.access, c.plan, c.chat, argv, f, c.want)
		}
	}
}

func TestAClaudeNotAskedForTheChatRunsInItsTUIWithItsPrompt(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Prompt = "fix it"
	argv, _ := argvOf(s, "")
	want := []string{"claude", "--permission-mode", "default", "-n", "fix-it", "--", "fix it"}
	if !slices.Equal(argv, want) {
		t.Fatalf("got %q", argv)
	}
}

func TestCodexPlanIsNotAllowed(t *testing.T) {
	_, f := argvOf(spec("codex", "ask", true), "")
	if f == nil || f.Code != "access_not_allowed" || f.Message != "Codex can't plan first in a terminal session" {
		t.Fatalf("got %+v", f)
	}
}

func TestPromptGoesLastAfterDoubleDash(t *testing.T) {
	s := spec("codex", "ask", false)
	s.Fork, s.Model, s.Prompt = "c-1", "gpt-5", "-rf fix the flaky tests now please"
	argv, _ := argvOf(s, "")
	want := []string{"codex", "fork", "c-1", "-s", "read-only", "-a", "on-request", "-m", "gpt-5", "--", "-rf fix the flaky tests now please"}
	if !slices.Equal(argv, want) {
		t.Fatalf("got %q", argv)
	}
}

func TestCodexRunsForTheChatAndLeavesItsPromptOut(t *testing.T) {
	s := spec("codex", "edits", false)
	s.Model, s.Prompt, s.Chat = "gpt-5", "fix it", true
	argv, _ := argvOf(s, "")
	want := []string{"codex", "app-server", "-c", `sandbox_mode="workspace-write"`, "-c", `approval_policy="on-request"`, "-c", `model="gpt-5"`}
	if !slices.Equal(argv, want) {
		t.Fatalf("got %q", argv)
	}
}

func TestClaudeRunsForTheChatAndLeavesItsPromptOut(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Model, s.Effort, s.Prompt, s.Chat = "opus", "high", "-rf fix the flaky tests now please", true
	argv, _ := argvOf(s, "")
	want := []string{"claude", "--permission-mode", "default", "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--permission-prompt-tool", "stdio", "--allow-dangerously-skip-permissions",
		"--model", "opus", "--effort", "high", "-n", "rf-fix-the-flaky"}
	if !slices.Equal(argv, want) {
		t.Fatalf("got %q", argv)
	}
}

func TestANewWorktreeNamesTheSession(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Prompt = "fix it"
	argv, _ := argvOf(s, "calm-otter")
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
		if _, f := argvOf(s, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%+v: got %+v", s, f)
		}
	}
}

func TestASwitchTakesOnlyAModelOrEffortTheAgentCouldHave(t *testing.T) {
	for _, s := range [][3]string{{"claude", "opus; rm -rf", ""}, {"claude", "", "ultra"}, {"codex", "", "high now"}} {
		if ValidSwitch(s[0], s[1], s[2]) == nil {
			t.Errorf("%q passed", s)
		}
	}
	for _, s := range [][3]string{{"claude", "opus", "xhigh"}, {"codex", "gpt-6-sol", "ultra"}} {
		if err := ValidSwitch(s[0], s[1], s[2]); err != nil {
			t.Errorf("%q: %v", s, err)
		}
	}
}

func TestNoAccessEverPassesFullAuto(t *testing.T) {
	for _, provider := range []string{"claude", "codex"} {
		for _, access := range proto.Accesses {
			for _, plan := range []bool{false, true} {
				if argv, _ := argvOf(spec(provider, access, plan), ""); slices.Contains(argv, "--full-auto") {
					t.Errorf("%s %s plan=%v: got %q", provider, access, plan, argv)
				}
			}
		}
	}
}

func TestControlBytesAreStrippedFromThePrompt(t *testing.T) {
	s := spec("codex", "ask", false)
	s.Fork, s.Prompt = "c-1", "fix\x1b[2J it\x03\x7f\n\tnow"
	argv, _ := argvOf(s, "")
	if got := argv[len(argv)-1]; got != "fix[2J it\n\tnow" {
		t.Fatalf("prompt = %q", got)
	}
}

func TestAModelThatLooksLikeAFlagIsAnInvalidSpec(t *testing.T) {
	for _, m := range []string{"-c", "--dangerously-bypass-approvals-and-sandbox"} {
		if _, f := argvOf(proto.LaunchSpec{Provider: "codex", Access: "ask", Model: m}, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%q: got %+v", m, f)
		}
	}
}

func TestPromptOnlyArgsComeBeforeThePrompt(t *testing.T) {
	s := spec("codex", "settings", false)
	s.Fork, s.Prompt = "c-1", "fix it"
	args := config.AgentArgs{Prompt: []string{"--prompt"}, Fork: []string{"fork", config.SessionID}}
	argv, _ := Argv(s, "", args)
	if !slices.Equal(argv, []string{"codex", "fork", "c-1", "--prompt", "fix it"}) {
		t.Fatalf("got %q", argv)
	}
	s.Prompt = ""
	if argv, _ := Argv(s, "", args); slices.Contains(argv, "--prompt") {
		t.Fatalf("no prompt, got %q", argv)
	}
}

func TestAForkPutsTheSessionIdInItsArgsBeforeTheFlags(t *testing.T) {
	for _, c := range []struct{ provider, want string }{
		{"claude", "claude --resume c-1 --fork-session --permission-mode default -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio --allow-dangerously-skip-permissions"},
		{"codex", "codex fork c-1 -s read-only -a on-request"},
	} {
		s := spec(c.provider, "ask", false)
		s.Fork, s.Chat = "c-1", true
		argv, f := argvOf(s, "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s: got %q %v, want %q", c.provider, argv, f, c.want)
		}
	}
}

func TestAForkNeedsForkArgsAndAPlainId(t *testing.T) {
	s := spec("claude", "ask", false)
	s.Fork = "c-1"
	if _, f := Argv(s, "", config.AgentArgs{}); f == nil || f.Code != "fork_unsupported" {
		t.Fatalf("no fork args: %v", f)
	}
	s.Fork = "c 1;rm"
	if _, f := argvOf(s, ""); f == nil || f.Code != "invalid_spec" {
		t.Fatalf("bad id: %v", f)
	}
}

func TestEveryProviderHasACLI(t *testing.T) {
	for _, p := range providers.All {
		if clis[p.ID] == nil {
			t.Errorf("%s has no cli", p.ID)
		}
	}
}
