package launch

import (
	"errors"
	"regexp"
	"slices"
	"strings"

	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

var ClaudeEfforts = []string{"low", "medium", "high", "xhigh", "max"}

var model = regexp.MustCompile(`^[A-Za-z0-9._:\[\]][A-Za-z0-9._:\[\]-]{0,63}$`)

const badModel, badClaudeEffort = "Model names use letters, digits and . _ : [ ] -, and don't start with -", "Claude's effort is low, medium, high, xhigh or max"

// ValidSwitch refuses a model or effort a running agent can't switch to. A
// codex model lists its own efforts, so codex's is only checked for shape.
func ValidSwitch(provider, name, effort string) error {
	switch {
	case name != "" && !model.MatchString(name):
		return errors.New(badModel)
	case provider == "claude" && effort != "" && !slices.Contains(ClaudeEfforts, effort):
		return errors.New(badClaudeEffort)
	case effort != "" && !model.MatchString(effort):
		return errors.New("An effort is one word")
	}
	return nil
}

var claudeAccess = map[string][]string{
	"settings": nil,
	"ask":      {"--permission-mode", "default"},
	"edits":    {"--permission-mode", "acceptEdits"},
	"auto":     {"--permission-mode", "auto"},
	"full":     {"--permission-mode", "bypassPermissions", "--allow-dangerously-skip-permissions"},
}

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
	if effort != "" {
		argv = append(argv, "-c", `model_reasoning_effort="`+effort+`"`)
	}
	return argv
}

// Chat is whether s runs for the chat behind `pocketd pipe`. A codex fork
// runs in its TUI, as only that forks from the command line.
func Chat(s proto.LaunchSpec) bool {
	return s.Provider == "claude" || s.Provider == "codex" && s.Fork == ""
}

// chatFlags run claude for the chat: stream-json both ways, and its asks to
// use a tool as control requests rather than a dialog.
var chatFlags = []string{"-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--permission-prompt-tool", "stdio"}

// Piped runs argv behind `pocketd pipe`, which relays its stdio to pocketd.
func Piped(exe string, argv []string) []string {
	return append([]string{exe, "pipe"}, argv...)
}

// Argv is a Session's command line. An agent run for the chat gets its
// prompt as the first message instead. name is a new Worktree's name; without
// one, claude's session is named after the prompt. A fork's args come first,
// since codex forks with a subcommand.
func Argv(s proto.LaunchSpec, name string, args config.AgentArgs) ([]string, *Failure) {
	if s.Model != "" && !model.MatchString(s.Model) {
		return nil, fail("invalid_spec", badModel)
	}
	prompt, err := terminal.Sanitize(s.Prompt)
	if err != nil {
		return nil, fail("invalid_spec", err.Error())
	}
	argv := []string{s.Provider}
	if s.Fork != "" {
		if len(args.Fork) == 0 {
			return nil, fail("fork_unsupported", "This agent has no fork args in Settings")
		}
		if !resumeID.MatchString(s.Fork) {
			return nil, fail("invalid_spec", "That session id can't be forked")
		}
		argv = append(argv, config.Fill(args.Fork, s.Fork)...)
	}
	switch s.Provider {
	case "claude":
		if s.Effort != "" && !slices.Contains(ClaudeEfforts, s.Effort) {
			return nil, fail("invalid_spec", badClaudeEffort)
		}
		if s.Plan {
			argv = append(argv, "--permission-mode", "plan")
		} else {
			argv = append(argv, claudeAccess[s.Access]...)
		}
		argv = append(argv, chatFlags...)
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
	case "codex":
		if s.Plan {
			return nil, fail("access_not_allowed", "Codex can't plan first in a terminal session")
		}
		if s.Effort != "" {
			return nil, fail("invalid_spec", "Codex takes no effort here")
		}
		if Chat(s) {
			return codexChat(s.Access, s.Model, ""), nil
		}
		argv = append(argv, codexAccess[s.Access]...)
		if s.Model != "" {
			argv = append(argv, "-m", s.Model)
		}
	}
	if prompt != "" && !Chat(s) {
		argv = append(append(argv, args.Prompt...), prompt)
	}
	return argv, nil
}

// Slug is the first four words of prompt, lower-case, letters and digits only.
func Slug(prompt string) string {
	var words []string
	for _, w := range strings.Fields(strings.ToLower(prompt)) {
		w = strings.Map(func(r rune) rune {
			if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' {
				return r
			}
			return -1
		}, w)
		if w != "" {
			words = append(words, w)
		}
		if len(words) == 4 {
			break
		}
	}
	return strings.Join(words, "-")
}
