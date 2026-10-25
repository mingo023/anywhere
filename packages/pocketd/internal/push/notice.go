package push

import (
	"encoding/json"
	"strings"

	"pocketd/internal/proto"
)

// Alert kinds, as Notice.kind and events' push{alert}.
const (
	NeedsYou   = "needsYou"
	Done       = "done"
	Failed     = "failed"
	Automation = "automation"
	Test       = "test"
)

// Notice is what an alert's sealed "e" opens to on the phone.
type Notice struct {
	Mac     string `json:"mac"`
	Agent   string `json:"agent"`
	Kind    string `json:"kind"`
	Title   string `json:"title"`
	Place   string `json:"place"`
	Line    string `json:"line"`
	Request string `json:"request,omitempty"`
	// Badge counts this Mac's sessions wanting a look; the phone sums every Mac's.
	Badge int `json:"badge"`
}

// lineMax cuts a banner's line as the desktop's alerts.rs does.
const lineMax = 240

// Cut keeps the first lineMax characters, marking a cut with "…".
func Cut(line string) string {
	r := []rune(line)
	if len(r) <= lineMax {
		return line
	}
	return string(r[:lineMax]) + "…"
}

var providerNames = map[string]string{"claude": "Claude Code", "codex": "Codex"}

// Title is the session's title, or its provider's name before it has one.
func Title(a proto.AgentSummary) string {
	if a.Title != "" {
		return a.Title
	}
	if n, ok := providerNames[a.Provider]; ok {
		return n
	}
	return "Shell"
}

// Place is "project · worktree", the project alone, or "" outside every project.
func Place(a proto.AgentSummary) string {
	switch {
	case a.Project == "":
		return ""
	case a.Worktree == "":
		return a.Project
	}
	return a.Project + " · " + a.Worktree
}

// Parts splits an ask as the desktop's Permission::parts: ("run", "ls /tmp").
func Parts(req proto.PermissionRequest) (verb, what string) {
	d := req.Detail
	switch d.Kind {
	case "shell":
		return "run", d.Command
	case "edit", "write", "read":
		return d.Kind, d.Path
	case "search":
		return "search", d.Query
	case "task":
		return "start", d.Description
	}
	if d.Name != "" {
		return "use", d.Name
	}
	return "use", req.ToolName
}

// Ask is the ask as a line: "Wants to run ls /tmp".
func Ask(req proto.PermissionRequest) string {
	verb, what := Parts(req)
	return strings.TrimSpace("Wants to " + verb + " " + what)
}

// FirstParagraph is the reply's first non-blank paragraph with its whitespace collapsed.
func FirstParagraph(text string) string {
	for _, p := range strings.Split(text, "\n\n") {
		if p = strings.Join(strings.Fields(p), " "); p != "" {
			return p
		}
	}
	return ""
}

var labels = map[string]string{NeedsYou: "Needs you", Done: "Done", Failed: "Failed"}

// Line is the banner's second line: the ask, the reply's first paragraph, or the status, cut to 240.
func Line(kind string, ask *proto.PermissionRequest, reply string) string {
	text := ""
	switch {
	case kind == NeedsYou && ask != nil:
		text = Ask(*ask)
	case kind == Done:
		text = FirstParagraph(reply)
	}
	if text == "" {
		text = labels[kind]
	}
	return Cut(text)
}

// Answerable is an ask the Lock Screen can answer: plain allow or deny, no options or questions.
func Answerable(req proto.PermissionRequest) bool {
	return len(req.Options) == 0 && req.ToolName != "AskUserQuestion"
}

// Category is the notification category that offers the actions kind and ask allow.
func Category(kind string, ask *proto.PermissionRequest) string {
	switch kind {
	case NeedsYou:
		switch {
		case ask == nil || !Answerable(*ask):
			return "QUESTION"
		case ask.Feedback:
			return "PERM_NOTE"
		}
		return "PERM"
	case Done:
		return "DONE"
	case Failed:
		return "FAILED"
	case Automation:
		return "AUTOMATION"
	}
	return ""
}

// Level is the interruption level: Needs you breaks through Focus, an automation that went well waits quietly.
func Level(kind string, failed bool) string {
	switch {
	case kind == NeedsYou:
		return "time-sensitive"
	case kind == Automation && !failed:
		return "passive"
	}
	return "active"
}

var generic = map[string]string{
	NeedsYou:   "A session needs you",
	Done:       "A session is done",
	Failed:     "A session failed",
	Automation: "An automation finished",
	Test:       "Notifications from your Mac work",
}

// AlertPayload is the alert as the relay forwards it. Everything the relay
// and Apple can read is generic; the notice travels sealed in "e".
func AlertPayload(key []byte, n Notice, category, level string) (json.RawMessage, error) {
	plain, err := json.Marshal(n)
	if err != nil {
		return nil, err
	}
	e, err := Seal(key, n.Mac, plain)
	if err != nil {
		return nil, err
	}
	aps := map[string]any{
		"alert":              map[string]string{"title": "Anywhere", "body": generic[n.Kind]},
		"mutable-content":    1,
		"interruption-level": level,
		"badge":              n.Badge,
	}
	if n.Agent != "" {
		aps["thread-id"] = Opaque(key, n.Agent)
	}
	if category != "" {
		aps["category"] = category
	}
	if level != "passive" {
		aps["sound"] = "default"
	}
	return json.Marshal(map[string]any{"aps": aps, "e": e})
}
