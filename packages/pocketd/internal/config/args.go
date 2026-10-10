package config

import (
	"errors"
	"slices"
	"strings"
	"unicode"
)

// SessionID marks where fork args take the source session's id.
const SessionID = "{sessionId}"

// ArgKeys are a provider's config.json keys for the words its command line takes.
var ArgKeys = []string{"promptArgs", "resumeArgs", "forkArgs"}

var defaultArgs = map[string][3]string{
	"claude": {"--", "--resume", "--resume " + SessionID + " --fork-session"},
	"codex":  {"--", "resume", "fork " + SessionID},
}

// AgentArgs are the words around a provider's command: before a prompt, before
// the id it resumes, and the fork args with SessionID still in them.
type AgentArgs struct{ Prompt, Resume, Fork []string }

// DefaultArgs is what a launch uses when config.json sets nothing.
func DefaultArgs(provider string) AgentArgs {
	d := defaultArgs[provider]
	return AgentArgs{Prompt: strings.Fields(d[0]), Resume: strings.Fields(d[1]), Fork: strings.Fields(d[2])}
}

// AgentArgs is provider's args from config.json, each its default when unset or not valid.
func (s *Settings) AgentArgs(provider string) AgentArgs {
	var w [3][]string
	for i, key := range ArgKeys {
		w[i], _ = SplitArgs(s.argText(provider, key))
	}
	return AgentArgs{Prompt: w[0], Resume: w[1], Fork: w[2]}
}

// argText is provider.key as typed, or its default when unset or not valid.
func (s *Settings) argText(provider, key string) string {
	text := ""
	if s.get(provider, key, &text) && ValidArgs(key, text) == nil {
		return text
	}
	return defaultArgs[provider][slices.Index(ArgKeys, key)]
}

// ValidArgs checks args text for key: words a shell would split, with no quote
// left in a word, and fork args naming SessionID unless empty.
func ValidArgs(key, text string) error {
	words, err := SplitArgs(text)
	switch {
	case err != nil:
		return err
	case len(words) > 32 || len(text) > 1024:
		return errors.New("args are at most 32 words")
	case slices.ContainsFunc(words, func(w string) bool { return strings.ContainsFunc(w, func(r rune) bool { return r == '\'' || unicode.IsControl(r) }) }):
		return errors.New("args can't hold a quote or control character")
	case key == "forkArgs" && len(words) > 0 && !slices.ContainsFunc(words, func(w string) bool { return strings.Contains(w, SessionID) }):
		return errors.New("fork args need " + SessionID + " where the session id goes")
	}
	return nil
}

// SplitArgs splits text into words as a shell would: spaces separate, quotes group, a backslash escapes.
func SplitArgs(text string) ([]string, error) {
	var words []string
	var w strings.Builder
	in, quote, escaped := false, rune(0), false
	for _, r := range text {
		switch {
		case escaped:
			w.WriteRune(r)
			escaped = false
		case r == '\\' && quote != '\'':
			escaped, in = true, true
		case quote != 0 && r == quote:
			quote = 0
		case quote != 0:
			w.WriteRune(r)
		case r == '\'' || r == '"':
			quote, in = r, true
		case unicode.IsSpace(r):
			if in {
				words = append(words, w.String())
				w.Reset()
				in = false
			}
		default:
			w.WriteRune(r)
			in = true
		}
	}
	if quote != 0 || escaped {
		return nil, errors.New("args have an unclosed quote")
	}
	if in {
		words = append(words, w.String())
	}
	return words, nil
}

// Fill puts id where words name SessionID.
func Fill(words []string, id string) []string {
	out := make([]string, len(words))
	for i, w := range words {
		out[i] = strings.ReplaceAll(w, SessionID, id)
	}
	return out
}
