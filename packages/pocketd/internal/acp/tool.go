package acp

import (
	"bytes"
	"cmp"
	"encoding/json"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// toolCall is a tool call as a tool_call, a tool_call_update or a permission request tells it.
type toolCall struct {
	ID     string `json:"toolCallId"`
	Title  string `json:"title"`
	Kind   string `json:"kind"`
	Status string `json:"status"`
	// Cursor tells a command it asks about here.
	Command   string          `json:"command"`
	RawInput  json.RawMessage `json:"rawInput"`
	RawOutput json.RawMessage `json:"rawOutput"`
	Content   []toolContent   `json:"content"`
	Locations []struct {
		Path string `json:"path"`
	} `json:"locations"`
	// Grok tells a call's kind, label and input here.
	Meta struct {
		Tool struct {
			Kind  string          `json:"kind"`
			Label string          `json:"label"`
			Input json.RawMessage `json:"input"`
		} `json:"x.ai/tool"`
	} `json:"_meta"`
}

// toolContent is a block of a call's content: text, or the diff of an edit.
type toolContent struct {
	Type    string          `json:"type"`
	Content json.RawMessage `json:"content"`
	Path    string          `json:"path"`
	OldText *string         `json:"oldText"`
	NewText string          `json:"newText"`
}

// call reads a tool call as ACP tells it, or as Grok and Cursor also do: its
// id in snake case, the call under a key of its own, a permission's under
// "subject".
func call(raw json.RawMessage) toolCall {
	var w struct {
		toolCall
		SnakeID string          `json:"tool_call_id"`
		Camel   json.RawMessage `json:"toolCall"`
		Snake   json.RawMessage `json:"tool_call"`
		Subject json.RawMessage `json:"subject"`
	}
	json.Unmarshal(raw, &w)
	t := w.toolCall
	t.ID = cmp.Or(t.ID, w.SnakeID)
	if nested := cmp.Or(string(w.Camel), string(w.Snake), string(w.Subject)); nested != "" {
		t.merge(call(json.RawMessage(nested)))
	}
	return t
}

// tool is a call as told so far, until it ends.
type tool struct {
	toolCall
	started bool
}

func empty(raw json.RawMessage) bool {
	raw = bytes.TrimSpace(raw)
	return len(raw) == 0 || bytes.Equal(raw, []byte("null")) || bytes.Equal(raw, []byte("{}"))
}

// merge adds what u tells of the call: an update names only what changed.
func (t *toolCall) merge(u toolCall) {
	t.ID = cmp.Or(u.ID, t.ID)
	t.Command = cmp.Or(u.Command, t.Command)
	t.Title = cmp.Or(u.Title, u.Meta.Tool.Label, t.Title)
	t.Kind = cmp.Or(u.Kind, u.Meta.Tool.Kind, t.Kind)
	t.Status = cmp.Or(u.Status, t.Status)
	if input := cmp.Or(string(u.RawInput), string(u.Meta.Tool.Input)); !empty(json.RawMessage(input)) {
		t.RawInput = json.RawMessage(input)
	}
	if !empty(u.RawOutput) {
		t.RawOutput = u.RawOutput
	}
	if len(u.Content) > 0 {
		t.Content = u.Content
	}
	if len(u.Locations) > 0 {
		t.Locations = u.Locations
	}
}

// waits is a call told of before what shows it: OpenCode tells a command's
// folder first, and the command in an update. An edit shows with its diff,
// which may come only as the user is asked about it or as it ends.
func (t *toolCall) waits() bool {
	switch {
	case t.done():
		return false
	case t.Kind == "edit":
		_, _, _, ok := t.edit()
		return !ok
	case t.Status != "pending" || len(t.Content) > 0:
		return false
	}
	return empty(t.RawInput) || t.Kind == "execute" && t.input("command", "cmd", "script") == ""
}

func (t *toolCall) done() bool { return t.Status == "completed" || t.Status == "failed" }

// text is the text of a content block, or of a list of them.
func text(raw json.RawMessage) string {
	var one struct {
		Text string `json:"text"`
	}
	var many []json.RawMessage
	var s string
	switch {
	case json.Unmarshal(raw, &s) == nil:
		return s
	case json.Unmarshal(raw, &many) == nil:
		var parts []string
		for _, m := range many {
			parts = append(parts, text(m))
		}
		return strings.Join(parts, "")
	}
	json.Unmarshal(raw, &one)
	return one.Text
}

// said is the text of the call's content, its blocks a line apart.
func (t *toolCall) said() string {
	var parts []string
	for _, c := range t.Content {
		if s := text(c.Content); c.Type == "content" && s != "" {
			parts = append(parts, s)
		}
	}
	return strings.Join(parts, "\n")
}

// input reads the first of keys the call's input has as a string, a list
// of words joined: a command comes either way.
func (t *toolCall) input(keys ...string) string {
	var in map[string]any
	json.Unmarshal(t.RawInput, &in)
	for _, k := range keys {
		switch v := in[k].(type) {
		case string:
			if v != "" {
				return v
			}
		case []any:
			var words []string
			for _, w := range v {
				if s, ok := w.(string); ok {
					words = append(words, s)
				}
			}
			if len(words) > 0 {
				return strings.Join(words, " ")
			}
		}
	}
	return ""
}

func (t *toolCall) path() string {
	if len(t.Locations) > 0 && t.Locations[0].Path != "" {
		return t.Locations[0].Path
	}
	return t.input("path", "file_path", "filePath", "filepath", "abs_path")
}

// edit is the change an edit call makes, old being nil for a file written
// whole: its diff content, else the text its input holds, which is all
// OpenCode tells of an edit it doesn't ask about.
func (t *toolCall) edit() (path string, old *string, text string, ok bool) {
	for _, c := range t.Content {
		if c.Type == "diff" {
			return c.Path, c.OldText, c.NewText, true
		}
	}
	if text := t.input("content"); text != "" {
		return "", nil, text, true
	}
	was, now := t.input("oldString", "old_string", "old_str", "oldText"), t.input("newString", "new_string", "new_str", "newText")
	return "", &was, now, was != "" || now != ""
}

// command is the command an execute call runs. Hermes sends no input, only
// "$ <command>" as the call's content.
func (t *toolCall) command() string {
	if c := cmp.Or(t.input("command", "cmd", "script"), t.Command); c != "" {
		return c
	}
	for line := range strings.SplitSeq(t.said(), "\n") {
		if c, ok := strings.CutPrefix(line, "$ "); ok {
			return c
		}
	}
	return t.Title
}

// describe is the call's name and the card the phone draws for it, by its ACP kind.
func (t *toolCall) describe() (string, proto.ToolDetail) {
	switch t.Kind {
	case "execute":
		return "shell", proto.ToolDetail{Kind: "shell", Command: t.command(), Description: t.input("description")}
	case "read":
		return "read", proto.ToolDetail{Kind: "read", Path: cmp.Or(t.path(), t.Title)}
	case "edit":
		d := proto.ToolDetail{Kind: "edit", Path: t.path()}
		if path, old, text, ok := t.edit(); ok {
			d.Path = cmp.Or(path, d.Path)
			if old == nil {
				d.Kind, d.Diff = "write", timeline.FileDiff("", text)
			} else {
				d.Diff = timeline.FileDiff(*old, text)
			}
		}
		return d.Kind, d
	case "search":
		return "search", proto.ToolDetail{Kind: "search", Query: cmp.Or(t.input("pattern", "query", "regex"), t.Title), Path: t.input("path")}
	}
	name := cmp.Or(t.Title, t.Kind, "tool")
	d := proto.ToolDetail{Kind: "other", Name: name}
	if !empty(t.RawInput) && json.Valid(t.RawInput) {
		d.Input = t.RawInput
	}
	return name, d
}

// output is what the call came to: its content, else its raw output.
func (t *toolCall) output() *string {
	out := t.said()
	if out == "" && !empty(t.RawOutput) {
		var raw struct {
			Output string `json:"output"`
		}
		if json.Unmarshal(t.RawOutput, &out) != nil {
			json.Unmarshal(t.RawOutput, &raw)
			out = cmp.Or(raw.Output, string(t.RawOutput))
		}
	}
	if out == "" {
		return nil
	}
	return &out
}
