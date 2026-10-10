package launch

import (
	"regexp"
	"slices"
	"strings"

	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

var ClaudeEfforts = []string{"low", "medium", "high", "xhigh", "max"}

var model = regexp.MustCompile(`^[A-Za-z0-9._:\[\]][A-Za-z0-9._:\[\]-]{0,63}$`)

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

// Argv is a Session's command line. name is a new Worktree's name; without
// one, claude's session is named after the prompt. A fork's args come first,
// since codex forks with a subcommand.
func Argv(s proto.LaunchSpec, name string, args config.AgentArgs) ([]string, *Failure) {
	if s.Model != "" && !model.MatchString(s.Model) {
		return nil, fail("invalid_spec", "Model names use letters, digits and . _ : [ ] -, and don't start with -")
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
			return nil, fail("invalid_spec", "Claude's effort is low, medium, high, xhigh or max")
		}
		if s.Plan {
			argv = append(argv, "--permission-mode", "plan")
		} else {
			argv = append(argv, claudeAccess[s.Access]...)
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
	case "codex":
		if s.Plan {
			return nil, fail("access_not_allowed", "Codex can't plan first in a terminal session")
		}
		if s.Effort != "" {
			return nil, fail("invalid_spec", "Codex takes no effort here")
		}
		argv = append(argv, codexAccess[s.Access]...)
		if s.Model != "" {
			argv = append(argv, "-m", s.Model)
		}
	}
	if prompt != "" {
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
