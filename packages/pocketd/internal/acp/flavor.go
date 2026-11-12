package acp

import (
	"encoding/json"
	"slices"

	"pocketd/internal/providers"
)

// flavor is what one agent's ACP server does unlike the rest.
type flavor struct {
	// caps is added to the client capabilities initialize sends.
	caps map[string]any
	// auth is authenticate's params for initialize's result; nil calls none.
	auth func(init json.RawMessage) map[string]any
	// modes is the agent's own mode for each access. An access without one
	// leaves the mode alone, and pocketd's answers alone hold it to that access.
	modes map[string]string
	// planMode is the mode it plans in, and buildMode the one leaving plan
	// goes to where the access has no mode of its own.
	planMode, buildMode string
	// modeOption has its modes be the values of its "mode" config option.
	modeOption bool
	// newMeta is session/new's _meta for an access, which it keeps for the session.
	newMeta map[string]map[string]any
	// effortMode has its efforts be modes.
	effortMode bool
	// lenient answers a request it doesn't know with {} rather than an error.
	lenient bool
	// spawned takes plan mode and full access only on its command line, so
	// it switches either by starting again.
	spawned bool
	// heldPlan has pocketd hold it to a plan: what it asks to do that is no
	// read or search is refused.
	heldPlan bool
	// compactMethod or compactPrompt has it summarise its context: a method
	// of its own, or the command it takes as a prompt. Neither for one that
	// only compacts by itself.
	compactMethod, compactPrompt string
	// summary is how its compaction summary starts, which a loaded history
	// replays as a reply: nothing else marks it.
	summary string
}

// flavors has every ACP provider.
var flavors = map[string]flavor{
	// Cursor reads its sign-in at start; its authenticate opens a browser
	// sign-in on the Mac and waits on it. It tells of its sub-agents' work only to a client that takes it.
	providers.Cursor.ID: {
		caps:      map[string]any{"_meta": map[string]any{"parameterizedModelPicker": true, "subagents": true}},
		planMode:  "plan",
		buildMode: "agent",
		lenient:   true,
	},
	providers.Grok.ID: {
		auth:          grokAuth,
		newMeta:       map[string]map[string]any{"full": {"yoloMode": true}, "auto": {"autoMode": true}},
		effortMode:    true,
		spawned:       true,
		heldPlan:      true,
		compactMethod: "_x.ai/compact_conversation",
	},
	// OpenCode lists no /compact among its commands, and runs it all the same.
	// Its summary follows a template of its own.
	providers.OpenCode.ID: {planMode: "plan", buildMode: "build", modeOption: true, compactPrompt: "/compact", summary: "## Objective\n"},
	// fx asks before every read in its own ask mode, so it runs in auto mode
	// and polices itself. Neither it nor Hermes has a plan mode: each plans in
	// the mode that asks the most.
	providers.Fx.ID: {
		modes:    map[string]string{"ask": "auto", "edits": "auto", "auto": "auto", "full": "full-access"},
		planMode: "ask",
		heldPlan: true,
	},
	providers.Hermes.ID: {
		modes:    map[string]string{"ask": "default", "edits": "accept_edits", "auto": "dont_ask", "full": "dont_ask"},
		planMode: "default",
		heldPlan: true,
	},
	providers.Antigravity.ID: {
		caps: map[string]any{"session": map[string]any{"configOptions": map[string]any{"boolean": map[string]any{}}}},
		// Its server has no mode that runs every tool; pocketd answers for Full.
		modes:     map[string]string{"ask": "default", "edits": "accept-edits", "auto": "default", "full": "accept-edits"},
		planMode:  "plan",
		buildMode: "default",
	},
}

// grokAuth signs in headless, by the method grokMethod picks.
func grokAuth(init json.RawMessage) map[string]any {
	if id := grokMethod(init); id != "" {
		return map[string]any{"methodId": id, "_meta": map[string]any{"headless": true}}
	}
	return nil
}

// grokMethod never picks grok.com, a browser sign-in nothing here can finish:
// an API key when Grok saw one, else the method it defaults to, else the
// token `grok login` cached.
func grokMethod(init json.RawMessage) string {
	var r struct {
		AuthMethods []struct {
			ID string `json:"id"`
		} `json:"authMethods"`
		Meta struct {
			Default string `json:"defaultAuthMethodId"`
		} `json:"_meta"`
	}
	json.Unmarshal(init, &r)
	var ids []string
	for _, m := range r.AuthMethods {
		if m.ID != "" && m.ID != "grok.com" {
			ids = append(ids, m.ID)
		}
	}
	for _, id := range []string{"xai.api_key", r.Meta.Default, "cached_token"} {
		if slices.Contains(ids, id) {
			return id
		}
	}
	if len(ids) > 0 {
		return ids[0]
	}
	return ""
}

// mode is the agent's own mode for access, planning or not; leaving is
// whether it was planning until now. "" leaves its mode alone.
func (f flavor) mode(access string, plan, leaving bool) string {
	switch {
	case plan && f.planMode != "":
		return f.planMode
	case f.modes[access] != "":
		return f.modes[access]
	case leaving:
		return f.buildMode
	}
	return ""
}

type option struct {
	ID   string `json:"optionId"`
	Name string `json:"name"`
	Kind string `json:"kind"`
}

// pick is the option that is one of kinds, the earlier kind first: the one
// whose id says so, as Hermes's allow_session is an allow_always too, else
// the first of that kind.
func pick(options []option, kinds ...string) string {
	for _, kind := range kinds {
		for _, match := range []func(option) bool{
			func(o option) bool { return snake(o.ID) == kind },
			func(o option) bool { return o.Kind == kind },
		} {
			if i := slices.IndexFunc(options, match); i >= 0 {
				return options[i].ID
			}
		}
	}
	return ""
}

var (
	allowOnce = []string{"allow_once", "allow_always", "allow"}
	reject    = []string{"reject_once", "reject_always", "reject", "deny"}
)

// auto is the option pocketd answers a permission request with by itself
// under access, or "" to ask the user: for a tool of kind, one of ACP's.
// It only ever allows once: an agent keeps an "always" past the access that
// picked it, OpenCode for its process and Hermes in the user's config.
func auto(access, kind string, options []option) string {
	if access == "edits" && slices.Contains([]string{"execute", "other", "fetch"}, kind) {
		return ""
	}
	if slices.Contains([]string{"edits", "auto", "full"}, access) {
		return pick(options, "allow_once", "allow")
	}
	return ""
}
