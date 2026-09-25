package timeline

import (
	"encoding/json"

	"pocketd/internal/proto"
)

type toolInput map[string]any

func (in toolInput) str(key string) (string, bool) {
	s, ok := in[key].(string)
	return s, ok
}

func (in toolInput) get(key string) string {
	s, _ := in.str(key)
	return s
}

// Detail maps a Claude tool call to the card the phone draws.
func Detail(name string, raw json.RawMessage) proto.ToolDetail {
	var in toolInput
	_ = json.Unmarshal(raw, &in)
	switch name {
	case "Bash", "BashOutput":
		return proto.ToolDetail{Kind: "shell", Command: in.get("command"), Description: in.get("description")}
	case "Read", "NotebookRead":
		return proto.ToolDetail{Kind: "read", Path: in.get("file_path")}
	case "Edit", "NotebookEdit":
		d := proto.ToolDetail{Kind: "edit", Path: in.get("file_path")}
		newText, ok := in.str("new_string")
		if !ok {
			newText, ok = in.str("new_source")
		}
		if ok {
			oldText, has := in.str("old_string")
			if !has {
				oldText = in.get("old_source")
			}
			d.Diff = FileDiff(oldText, newText)
		}
		return d
	case "Write":
		d := proto.ToolDetail{Kind: "write", Path: in.get("file_path")}
		if content, ok := in.str("content"); ok {
			d.Diff = FileDiff("", content)
		}
		return d
	case "Grep", "Glob":
		return proto.ToolDetail{Kind: "search", Query: in.get("pattern"), Path: in.get("path")}
	case "Task", "Agent":
		return proto.ToolDetail{Kind: "task", Description: in.get("description")}
	}
	return proto.ToolDetail{Kind: "other", Name: name, Input: raw}
}
