package launch

import (
	"errors"
	"regexp"
	"strings"

	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

var model = regexp.MustCompile(`^[A-Za-z0-9._:\[\]][A-Za-z0-9._:\[\]-]{0,63}$`)

const badModel = "Model names use letters, digits and . _ : [ ] -, and don't start with -"

// cli is how pocketd runs one provider's command; clis has one per providers ID.
type cli interface {
	// switchEffort refuses an effort a running agent can't switch to.
	switchEffort(effort string) error
	// chat is whether s runs for the chat behind `pocketd pipe`.
	chat(s proto.LaunchSpec) bool
	// argv adds s's flags to argv. done means out is the whole command line.
	argv(s proto.LaunchSpec, argv []string, name, prompt string) (out []string, done bool, f *Failure)
	// chatResume is the whole argv that resumes t for the chat, if it has its own.
	chatResume(t state.Terminal, l state.Launch) ([]string, bool)
	// resume adds l's flags to argv, which resumes t.
	resume(t state.Terminal, l state.Launch, argv []string) []string
	// untrusted fails a launch in dir that would wait on a folder trust dialog.
	untrusted(env []string, dir string) *Failure
	login(exe string) []string
	logout(exe string) []string
}

var clis = map[string]cli{providers.Claude.ID: claude{}, providers.Codex.ID: codex{}}

// ValidSwitch refuses a model or effort a running agent can't switch to.
func ValidSwitch(provider, name, effort string) error {
	if name != "" && !model.MatchString(name) {
		return errors.New(badModel)
	}
	if c := clis[provider]; c != nil && effort != "" {
		if err := c.switchEffort(effort); err != nil {
			return err
		}
	}
	if effort != "" && !model.MatchString(effort) {
		return errors.New("An effort is one word")
	}
	return nil
}

// Chat is whether s runs for the chat behind `pocketd pipe`.
func Chat(s proto.LaunchSpec) bool {
	c := clis[s.Provider]
	return c != nil && c.chat(s)
}

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
	if c := clis[s.Provider]; c != nil {
		out, done, f := c.argv(s, argv, name, prompt)
		if f != nil {
			return nil, f
		}
		if done {
			return out, nil
		}
		argv = out
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
