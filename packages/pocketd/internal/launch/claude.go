package launch

import (
	"errors"
	"slices"

	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

const badClaudeEffort = "Claude's effort is low, medium, high, xhigh or max"

type claude struct{}

var claudeAccess = map[string][]string{
	"settings": nil,
	"ask":      {"--permission-mode", "default"},
	"edits":    {"--permission-mode", "acceptEdits"},
	"auto":     {"--permission-mode", "auto"},
	"full":     {"--permission-mode", "bypassPermissions"},
}

// allowBypass lets claude take bypassPermissions without starting in it.
const allowBypass = "--allow-dangerously-skip-permissions"

// chatFlags run claude for the chat: stream-json both ways, its asks to use a
// tool as control requests rather than a dialog, and Full allowed so the chat
// can switch to it whatever it launched or resumed at.
var chatFlags = []string{"-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--permission-prompt-tool", "stdio", allowBypass}

func (claude) switchEffort(effort string) error {
	if !slices.Contains(providers.Claude.Efforts, effort) {
		return errors.New(badClaudeEffort)
	}
	return nil
}

func (claude) chat(s proto.LaunchSpec) bool { return s.Chat }

// argv names the session after the prompt when there's no Worktree name.
func (c claude) argv(s proto.LaunchSpec, argv []string, name, prompt string) ([]string, bool, *Failure) {
	if s.Effort != "" && !slices.Contains(providers.Claude.Efforts, s.Effort) {
		return nil, false, fail("invalid_spec", badClaudeEffort)
	}
	if s.Plan {
		argv = append(argv, "--permission-mode", "plan")
	} else {
		argv = append(argv, claudeAccess[s.Access]...)
	}
	if c.chat(s) {
		argv = append(argv, chatFlags...)
	} else if s.Access == "full" && !s.Plan {
		argv = append(argv, allowBypass)
	}
	if s.Model != "" {
		argv = append(argv, "--model", s.Model)
	}
	if s.Effort != "" {
		argv = append(argv, "--effort", s.Effort)
	}
	if name == "" {
		name = Slug(prompt)
	}
	if name != "" {
		argv = append(argv, "-n", name)
	}
	return argv, false, nil
}

func (claude) chatResume(state.Terminal, state.Launch) ([]string, bool) { return nil, false }

func (claude) resume(t state.Terminal, l state.Launch, argv []string) []string {
	mode := map[string]string{"edits": "acceptEdits", "auto": "auto"}[l.Access]
	if l.Plan {
		mode = "plan"
	}
	if mode == "" && l.Access != "settings" {
		mode = "default"
	}
	if mode != "" {
		argv = append(argv, "--permission-mode", mode)
	}
	if l.Model != "" {
		argv = append(argv, "--model", l.Model)
	}
	if l.Effort != "" {
		argv = append(argv, "--effort", l.Effort)
	}
	if t.Chat {
		argv = append(argv, chatFlags...)
	}
	return argv
}

func (claude) untrusted(env []string, dir string) *Failure {
	if trustedAt(env, dir) {
		return nil
	}
	return fail("folder_not_trusted", "Trust this folder in Claude on your Mac first")
}

func (claude) login(exe string) []string { return []string{exe, "auth", "login"} }

func (claude) logout(exe string) []string { return []string{exe, "auth", "logout"} }
