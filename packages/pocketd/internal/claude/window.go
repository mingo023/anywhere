package claude

import (
	"encoding/json"
	"strings"
)

// windows is claude 2.1.285's model catalog: context window by id prefix.
var windows = map[string]int64{
	"claude-opus-5": 1_000_000, "claude-opus-5-5": 1_000_000, "claude-sonnet-5": 1_000_000, "claude-sonnet-5-5": 1_000_000,
	"claude-opus-4-7": 1_000_000, "claude-opus-4-8": 1_000_000, "claude-fable-5": 1_000_000, "claude-fable-5-1": 1_000_000,
	"claude-mythos-5": 1_000_000, "claude-mythos-5-1": 1_000_000,
	"claude-haiku-4-5": 200_000, "claude-opus-4-0": 200_000, "claude-opus-4-1": 200_000, "claude-opus-4-5": 200_000,
	"claude-opus-4-6": 200_000, "claude-sonnet-4-0": 200_000, "claude-sonnet-4-5": 200_000, "claude-sonnet-4-6": 200_000,
}

// Window is model's context window in tokens, 0 when unknown. The transcript
// never names the window, so it comes from the id: the longest known prefix,
// or a [1m] suffix.
func Window(model string) int64 {
	if strings.HasSuffix(model, "[1m]") {
		return 1_000_000
	}
	var best string
	for prefix := range windows {
		if strings.HasPrefix(model, prefix) && len(prefix) > len(best) {
			best = prefix
		}
	}
	return windows[best]
}

// ContextWindow is Window(model) plus what the transcript leaves out: its id
// drops the [1m] suffix that hook, the SessionStart hook's id, keeps.
func ContextWindow(model, hook string, used int64) int64 {
	w := Window(model)
	base, oneM := strings.CutSuffix(hook, "[1m]")
	if (oneM && strings.HasPrefix(model, base)) || (w > 0 && used > w) {
		return 1_000_000
	}
	return w
}

// Tokens is the context a main-chain assistant line was answered from.
func Tokens(raw []byte) (int64, bool) {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.Type != "assistant" || l.IsSidechain || l.Message.Model == "<synthetic>" || l.Message.Usage == nil {
		return 0, false
	}
	u := l.Message.Usage
	return u.InputTokens + u.CacheCreationInputTokens + u.CacheReadInputTokens, true
}
