package launch

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/state"
)

func resumeOf(t state.Terminal) ([]string, *Failure) { return Resume(t, config.DefaultArgs(t.Provider)) }

func saved(provider string, l state.Launch) state.Terminal {
	return state.Terminal{Provider: provider, ConversationID: "c-1", Launch: &l}
}

func TestResumeBringsBackTheSavedAccessPlanModelAndEffort(t *testing.T) {
	for _, c := range []struct {
		t    state.Terminal
		want string
	}{
		{saved("claude", state.Launch{Access: "settings"}), "claude --resume c-1"},
		{saved("claude", state.Launch{Access: "settings", Plan: true}), "claude --resume c-1 --permission-mode plan"},
		{saved("claude", state.Launch{Access: "ask"}), "claude --resume c-1 --permission-mode default"},
		{saved("claude", state.Launch{Access: "edits"}), "claude --resume c-1 --permission-mode acceptEdits"},
		{saved("claude", state.Launch{Access: "auto"}), "claude --resume c-1 --permission-mode auto"},
		{saved("claude", state.Launch{Access: "edits", Plan: true, Model: "opus", Effort: "high"}), "claude --resume c-1 --permission-mode plan --model opus --effort high"},
		{saved("codex", state.Launch{Access: "settings", Model: "gpt-5.5"}), "codex resume c-1 -m gpt-5.5"},
		{saved("codex", state.Launch{Access: "ask"}), "codex resume c-1 -s read-only -a on-request"},
		{saved("codex", state.Launch{Access: "edits", Model: "gpt-5.5", Effort: "high"}), "codex resume c-1 -s workspace-write -a on-request -m gpt-5.5"},
		{saved("codex", state.Launch{Access: "auto"}), "codex resume c-1 --approve-for-me"},
		{state.Terminal{Provider: "claude", ConversationID: "c-1"}, "claude --resume c-1 --permission-mode default"},
	} {
		argv, f := resumeOf(c.t)
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("Resume(%+v) = %q, %v; want %q", c.t, argv, f, c.want)
		}
	}
}

func TestFullResumesAsAsk(t *testing.T) {
	for provider, want := range map[string]string{"claude": "default", "codex": "read-only"} {
		argv, _ := resumeOf(saved(provider, state.Launch{Access: "full"}))
		if !slices.Contains(argv, want) || slices.Contains(argv, "--dangerously-skip-permissions") || slices.Contains(argv, "danger-full-access") {
			t.Errorf("%s full resumes as %q", provider, argv)
		}
	}
}

func TestCodexResumeNeverCarriesConfigFlags(t *testing.T) {
	argv, _ := resumeOf(saved("codex", state.Launch{Access: "edits", Model: "m", Effort: "high"}))
	if slices.Contains(argv, "-c") || slices.Contains(argv, "--config") {
		t.Fatalf("argv = %q", argv)
	}
}

func TestResumeRejectsHerdrBreaches(t *testing.T) {
	bad := []state.Terminal{
		{Provider: "claude", ConversationID: "c'1"},
		{Provider: "claude", ConversationID: ""},
		{Provider: "claude", ConversationID: strings.Repeat("a", 129)},
		{Provider: "gemini", ConversationID: "c-1"},
		saved("claude", state.Launch{Access: "ask", Model: "opus\n"}),
		saved("claude", state.Launch{Access: "ask", Model: "o'pus"}),
		saved("claude", state.Launch{Access: "ask", Model: strings.Repeat("m", 9000)}),
	}
	for _, s := range bad {
		if _, f := resumeOf(s); f == nil || f.Code != "invalid_resume_argv" {
			t.Errorf("Resume(%q) = %v", s.ConversationID, f)
		}
	}
	for _, argv := range [][]string{nil, {"/bin/claude"}, slices.Repeat([]string{"x"}, 65)} {
		if f := ValidResume(argv); f == nil || f.Code != "invalid_resume_argv" {
			t.Errorf("ValidResume(%d args) = %v", len(argv), f)
		}
	}
}

func TestParseReadsBackTheResumeArgv(t *testing.T) {
	for _, l := range []state.Launch{{Access: "ask"}, {Access: "edits", Model: "opus", Effort: "high"}, {Access: "auto"}, {Access: "ask", Plan: true}} {
		argv, _ := resumeOf(saved("claude", l))
		if got := state.Parse("claude", argv); got != l {
			t.Errorf("claude %+v reads back as %+v", l, got)
		}
	}
	for _, l := range []state.Launch{{Access: "ask"}, {Access: "edits", Model: "gpt-5.5"}, {Access: "auto"}} {
		argv, _ := resumeOf(saved("codex", l))
		if got := state.Parse("codex", argv); got != l {
			t.Errorf("codex %+v reads back as %+v", l, got)
		}
	}
}

func TestAMissingBinaryIsNotAccepted(t *testing.T) {
	dir := t.TempDir()
	onPath := func(p string) (string, bool) {
		path, err := resolve(p, []string{"PATH=" + dir})
		return path, err == nil
	}
	if _, _, reason := resumeCmd(saved("claude", state.Launch{Access: "ask"}), config.DefaultArgs("claude"), "/bin/zsh", "/pd", onPath); reason != "resume_not_accepted" {
		t.Fatalf("reason = %q", reason)
	}
	os.WriteFile(filepath.Join(dir, "claude"), []byte("#!/bin/sh\n"), 0o755)
	cmd, args, reason := resumeCmd(saved("claude", state.Launch{Access: "ask"}), config.DefaultArgs("claude"), "/bin/zsh", "/pd", onPath)
	if reason != "" || cmd != "/bin/zsh" || !slices.Contains(args, "--resume") {
		t.Fatalf("resumeCmd = %q %q %q", cmd, args, reason)
	}
}

func TestAResumeRunsTheProvidersConfiguredCommand(t *testing.T) {
	custom := func(string) (string, bool) { return "/opt/tools/my-claude", true }
	_, args, reason := resumeCmd(saved("claude", state.Launch{Access: "ask"}), config.DefaultArgs("claude"), "/bin/zsh", "/pd", custom)
	if reason != "" || !slices.Contains(args, "/opt/tools/my-claude") || slices.Contains(args, "claude") {
		t.Fatalf("resumeCmd = %q %q", args, reason)
	}
}

func TestResumeUsesTheResumeArgsAndRefusesWithoutThem(t *testing.T) {
	argv, f := Resume(saved("claude", state.Launch{Access: "settings"}), config.AgentArgs{Resume: []string{"-r"}})
	if f != nil || strings.Join(argv, " ") != "claude -r c-1" {
		t.Fatalf("got %q %v", argv, f)
	}
	if _, f := Resume(saved("claude", state.Launch{Access: "ask"}), config.AgentArgs{}); f == nil || f.Code != "resume_not_accepted" {
		t.Fatalf("no resume args: %v", f)
	}
}
