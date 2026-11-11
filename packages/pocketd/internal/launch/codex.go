package launch

import (
	"pocketd/internal/proto"
	"pocketd/internal/state"
)

type codex struct{}

var codexAccess = map[string][]string{
	"settings": nil,
	"ask":      {"-s", "read-only", "-a", "on-request"},
	"edits":    {"-s", "workspace-write", "-a", "on-request"},
	"auto":     {"--approve-for-me"},
	"full":     {"-s", "danger-full-access", "-a", "never"},
}

// codexChatAccess is codexAccess as config for `codex app-server`, which
// takes none of the TUI's flags.
var codexChatAccess = map[string][]string{
	"settings": nil,
	"ask":      {"-c", `sandbox_mode="read-only"`, "-c", `approval_policy="on-request"`},
	"edits":    {"-c", `sandbox_mode="workspace-write"`, "-c", `approval_policy="on-request"`},
	"auto":     {"-c", `sandbox_mode="workspace-write"`, "-c", `approvals_reviewer="auto_review"`},
	"full":     {"-c", `sandbox_mode="danger-full-access"`, "-c", `approval_policy="never"`},
}

// codexChat runs codex for the chat: its app-server on stdio, which pocketd
// drives over JSON-RPC.
func codexChat(access, model, effort string) []string {
	argv := append([]string{"codex", "app-server"}, codexChatAccess[access]...)
	if model != "" {
		argv = append(argv, "-c", `model="`+model+`"`)
	}
	return append(argv, effortConfig(effort)...)
}

// effortConfig sets codex's reasoning effort, the same in its TUI and
// app-server. One that isn't a word, as from a hand-edited state file, is
// left out rather than written into codex's config.
func effortConfig(effort string) []string {
	if !model.MatchString(effort) {
		return nil
	}
	return []string{"-c", `model_reasoning_effort="` + effort + `"`}
}

// switchEffort passes any effort: a codex model lists its own.
func (codex) switchEffort(string) error { return nil }

// chat leaves a fork in the TUI, as only that forks from the command line.
func (codex) chat(s proto.LaunchSpec) bool { return s.Chat && s.Fork == "" }

func (codex) attaches(proto.LaunchSpec) bool { return true }

func (c codex) argv(s proto.LaunchSpec, argv []string, _, _ string) ([]string, bool, *Failure) {
	if s.Plan {
		return nil, false, fail("access_not_allowed", "Codex can't plan first in a terminal session")
	}
	if s.Effort != "" && !model.MatchString(s.Effort) {
		return nil, false, fail("invalid_spec", badEffort)
	}
	if c.chat(s) {
		return codexChat(s.Access, s.Model, s.Effort), true, nil
	}
	argv = append(argv, codexAccess[s.Access]...)
	if s.Model != "" {
		argv = append(argv, "-m", s.Model)
	}
	return append(argv, effortConfig(s.Effort)...), false, nil
}

// chatResume runs the app-server again; pocketd resumes its thread over JSON-RPC.
func (codex) chatResume(t state.Terminal, l state.Launch) ([]string, bool) {
	if !t.Chat {
		return nil, false
	}
	if l.Access == "full" && !t.KeepAccess {
		l.Access = "ask"
	}
	return codexChat(l.Access, l.Model, l.Effort), true
}

func (codex) resume(t state.Terminal, l state.Launch, argv []string) []string {
	switch {
	case l.Access == "full" && t.KeepAccess:
		argv = append(argv, codexAccess["full"]...)
	case l.Access == "settings":
	case l.Access == "edits":
		argv = append(argv, "-s", "workspace-write", "-a", "on-request")
	case l.Access == "auto":
		argv = append(argv, "--approve-for-me")
	default:
		argv = append(argv, "-s", "read-only", "-a", "on-request")
	}
	if l.Model != "" {
		argv = append(argv, "-m", l.Model)
	}
	return append(argv, effortConfig(l.Effort)...)
}

func (codex) untrusted([]string, string) *Failure { return nil }

// login uses a device code, which works on a host whose browser is elsewhere.
func (codex) login(exe string) []string { return []string{exe, "login", "--device-auth"} }

func (codex) logout(exe string) []string { return []string{exe, "logout"} }
