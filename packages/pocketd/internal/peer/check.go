package peer

import (
	"errors"
	"fmt"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

// Needs is the allowlist of verbs, keyed "surface:verb". A verb missing here
// is refused for everyone. ops principals are only ever owner or PTY, so ops
// verbs that need spawn need owner too.
var Needs = map[string]Scope{
	"ws:agent.list":         Observe,
	"ws:agent.timeline":     Observe,
	"ws:agent.view":         Observe,
	"ws:agent.seen":         Observe,
	"ws:project.list":       Observe,
	"ws:agent.prompt":       Drive,
	"ws:agent.interrupt":    Drive,
	"ws:agent.compact":      Drive,
	"ws:agent.close":        Drive,
	"ws:agent.pin":          Drive,
	"ws:permission.resolve": Approve,
	"ws:pair.begin":         Own,
	"ws:agent.create":       Spawn,
	"ws:agent.providers":    Observe,
	"ws:config.set":         Own,
	"ws:worktree.rename":    Own,
	"ops:launch-exit":       Observe,
	"ops:config-set":        Own,
	"ops:list":              Observe,
	"ops:status":            Observe,
	"ops:hook":              Observe,
	"ops:spawn":             Own,
	"ops:attach":            Own,
	"ops:screen":            Own,
	"ops:resize":            Own,
	"ops:close":             Own,
	"ops:input":             Own,
	"ops:prompt":            Own,
	"ops:devices":           Own,
	"ops:devices.rename":    Own,
	"ops:devices.revoke":    Own,
	"ops:pair.begin":        Own,
}

var guarded = map[string]bool{
	"ws:agent.prompt": true, "ws:agent.interrupt": true, "ws:agent.compact": true, "ws:agent.close": true,
	"ops:input": true, "ops:prompt": true,
}

type Refusal struct{ Code, Message string }

// CodeOf is the wire error code for err, or "" when it has none.
func CodeOf(err error) string {
	if r, ok := errors.AsType[*Refusal](err); ok {
		return r.Code
	}
	if errors.Is(err, terminal.ErrPromptTooLarge) {
		return proto.CodePromptTooLarge
	}
	return ""
}

func (r *Refusal) Error() string { return r.Message }

// Check refuses verb unless p may run it. asking names the target Terminal
// when it waits on an ask, else "". For a PTY peer the ask guard comes before
// scope, so a refused self-approval reads as ask_open, not a missing scope.
func (p Principal) Check(verb, asking, text string) *Refusal {
	_, bare, _ := strings.Cut(verb, ":")
	if p.Kind == PTY && asking != "" && guarded[verb] {
		return &Refusal{proto.CodeAskOpen, fmt.Sprintf("Terminal %s is waiting on an ask; answer it from the desktop or phone", asking)}
	}
	if p.Kind == PTY && (verb == "ws:agent.prompt" || verb == "ops:prompt") && (strings.HasPrefix(text, "!") || strings.HasPrefix(text, "/")) {
		return &Refusal{proto.CodePromptRefused, "agent prompts can't start with ! or /"}
	}
	need, ok := Needs[verb]
	if !ok {
		return &Refusal{proto.CodeScopeDenied, bare + " is not a known verb"}
	}
	if !p.Has(need) {
		return &Refusal{proto.CodeScopeDenied, fmt.Sprintf("%s needs %s; run it outside Pocket Terminals, or against a scratch pocketd (POCKETD_SOCK)", bare, need)}
	}
	return nil
}
