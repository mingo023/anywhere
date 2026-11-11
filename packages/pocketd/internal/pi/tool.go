package pi

import (
	"encoding/json"
	"regexp"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// object is a call's arguments, which pi tells as an object or as its JSON in a string.
func object(args json.RawMessage) json.RawMessage {
	var s string
	if json.Unmarshal(args, &s) == nil && json.Valid([]byte(s)) {
		return json.RawMessage(s)
	}
	return args
}

type input map[string]any

// str is the first of keys in holds as text.
func (in input) str(keys ...string) string {
	for _, k := range keys {
		if s, ok := in[k].(string); ok && s != "" {
			return s
		}
	}
	return ""
}

// describe is a call's name and the card the phone draws for it. pi has no
// kinds, so its tools, and the ones its extensions add, go by their names.
func describe(name string, args json.RawMessage) (string, proto.ToolDetail) {
	var in input
	json.Unmarshal(args, &in)
	n := strings.ToLower(name)
	has := func(words ...string) bool {
		for _, w := range words {
			if strings.Contains(n, w) {
				return true
			}
		}
		return false
	}
	path := in.str("path", "file_path", "filePath")
	switch {
	case has("bash", "command", "shell"):
		return "shell", proto.ToolDetail{Kind: "shell", Command: in.str("command", "cmd", "script"), Description: in.str("description")}
	case has("write"):
		d := proto.ToolDetail{Kind: "write", Path: path}
		if content, ok := in["content"].(string); ok {
			d.Diff = timeline.FileDiff("", content)
		}
		return "write", d
	case has("edit", "patch", "replace"):
		d := proto.ToolDetail{Kind: "edit", Path: path}
		if before, after, ok := edits(in); ok {
			d.Diff = timeline.FileDiff(before, after)
		}
		return "edit", d
	case has("read"):
		return "read", proto.ToolDetail{Kind: "read", Path: path}
	case has("grep", "glob", "search", "find") || n == "ls":
		return "search", proto.ToolDetail{Kind: "search", Query: in.str("pattern", "query", "glob"), Path: path}
	}
	d := proto.ToolDetail{Kind: "other", Name: name}
	if len(in) > 0 {
		d.Input = args
	}
	return name, d
}

// edits is the text an edit replaces and what it puts there: pi's edit takes
// a list of replacements, and older ones a single pair.
func edits(in input) (before, after string, ok bool) {
	list, _ := in["edits"].([]any)
	if len(list) == 0 {
		list = []any{map[string]any(in)}
	}
	var old, new []string
	for _, e := range list {
		pair, _ := e.(map[string]any)
		o, _ := pair["oldText"].(string)
		n, has := pair["newText"].(string)
		if !has {
			return "", "", false
		}
		old, new = append(old, o), append(new, n)
	}
	return strings.Join(old, "\n"), strings.Join(new, "\n"), true
}

// todos is the input of Claude's TodoWrite, which the timeline folds into
// its task list, for a call to a todo tool an extension added.
func todos(name string, args json.RawMessage) (json.RawMessage, bool) {
	n := strings.NewReplacer(" ", "", "_", "", "-", "").Replace(strings.ToLower(name))
	if !strings.HasSuffix(n, "todowrite") && !strings.HasSuffix(n, "writetodos") && !strings.HasSuffix(n, "updatetodos") {
		return nil, false
	}
	var in struct {
		Todos []input `json:"todos"`
	}
	if json.Unmarshal(args, &in) != nil || in.Todos == nil {
		return nil, false
	}
	out := []map[string]string{}
	for _, t := range in.Todos {
		status := strings.NewReplacer(" ", "", "_", "", "-", "").Replace(strings.ToLower(t.str("status")))
		switch status {
		case "completed", "complete", "done":
			status = "completed"
		case "inprogress", "active", "running":
			status = "in_progress"
		default:
			status = "pending"
		}
		if text := t.str("content", "activeForm", "text"); text != "" {
			out = append(out, map[string]string{"content": text, "status": status})
		}
	}
	raw, _ := json.Marshal(map[string]any{"todos": out})
	return raw, true
}

var ansi = regexp.MustCompile(`\x1b\[[0-9;?]*[ -/]*[@-~]`)

// plain is text an extension wrote for a terminal, without its colours.
func plain(s string) string { return strings.TrimSpace(ansi.ReplaceAllString(s, "")) }
