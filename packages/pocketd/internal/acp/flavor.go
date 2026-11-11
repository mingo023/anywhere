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
	// auth picks the method to authenticate with from initialize's result; "" is none.
	auth     func(init json.RawMessage) string
	authMeta map[string]any
	// modes is the agent's own mode for each access. An access without one
	// leaves the mode alone, and pocketd's answers alone hold it to that access.
	modes map[string]string
	// plan is the mode it plans in, and the one that ends planning.
	plan [2]string
	// modeOption has its modes be the values of its "mode" config option.
	modeOption bool
	// newMeta is session/new's _meta for an access.
	newMeta map[string]map[string]any
	// effortMode has its efforts be modes.
	effortMode bool
	// lenient answers a request it doesn't know with {} rather than an error.
	lenient bool
	// spawned takes plan mode and full access only on its command line, so
	// neither switches in a running session.
	spawned bool
}

// flavors has every ACP provider.
var flavors = map[string]flavor{
	// Cursor reads its sign-in at start; its authenticate opens a browser
	// sign-in on the Mac and waits on it. It tells of its sub-agents' work only to a client that takes it.
	providers.Cursor.ID: {
		caps:    map[string]any{"_meta": map[string]any{"parameterizedModelPicker": true, "subagents": true}},
		plan:    [2]string{"plan", "agent"},
		lenient: true,
	},
	providers.Grok.ID: {
		auth:       grokAuth,
		authMeta:   map[string]any{"headless": true},
		newMeta:    map[string]map[string]any{"full": {"yoloMode": true}, "auto": {"autoMode": true}},
		effortMode: true,
		spawned:    true,
	},
	providers.OpenCode.ID: {plan: [2]string{"plan", "build"}, modeOption: true},
	// fx asks before every read in its own ask mode, so it runs in auto mode and polices itself.
	providers.Fx.ID: {
		modes: map[string]string{"ask": "auto", "edits": "auto", "auto": "auto", "full": "full-access"},
	},
	providers.Hermes.ID: {
		modes: map[string]string{"ask": "default", "edits": "accept_edits", "auto": "dont_ask", "full": "dont_ask"},
	},
	providers.Antigravity.ID: {
		caps: map[string]any{"session": map[string]any{"configOptions": map[string]any{"boolean": map[string]any{}}}},
		// Its server has no mode that runs every tool; pocketd answers for Full.
		modes: map[string]string{"ask": "default", "edits": "accept-edits", "auto": "default", "full": "accept-edits"},
		plan:  [2]string{"plan", "default"},
	},
}

// grokAuth never picks grok.com, a browser sign-in nothing here can finish:
// an API key when Grok saw one, else the method it defaults to, else the
// token `grok login` cached.
func grokAuth(init json.RawMessage) string {
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
	case plan && f.plan[0] != "":
		return f.plan[0]
	case f.modes[access] != "":
		return f.modes[access]
	case leaving:
		return f.plan[1]
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
