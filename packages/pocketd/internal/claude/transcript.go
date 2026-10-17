// Package claude turns Claude Code's JSONL transcript into timeline events.
package claude

import (
	"bufio"
	"context"
	"encoding/json"
	"html"
	"io"
	"os"
	"strings"
	"time"

	"pocketd/internal/timeline"
)

type line struct {
	Type             string `json:"type"`
	Subtype          string `json:"subtype"`
	IsMeta           bool   `json:"isMeta"`
	IsSidechain      bool   `json:"isSidechain"`
	IsCompactSummary bool   `json:"isCompactSummary"`
	DurationMs       int64  `json:"durationMs"`
	CompactMetadata  struct {
		Trigger string `json:"trigger"`
	} `json:"compactMetadata"`
	Message struct {
		Model   string          `json:"model"`
		Content json.RawMessage `json:"content"`
		Usage   *struct {
			InputTokens              int64 `json:"input_tokens"`
			CacheCreationInputTokens int64 `json:"cache_creation_input_tokens"`
			CacheReadInputTokens     int64 `json:"cache_read_input_tokens"`
		} `json:"usage"`
	} `json:"message"`
	Origin struct {
		Kind string `json:"kind"`
	} `json:"origin"`
	// ToolUseResult is a string for some tools, so it is read only when needed.
	ToolUseResult json.RawMessage `json:"toolUseResult"`
}

// Model returns the model an assistant line was written by, or "".
func Model(raw []byte) string {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.Type != "assistant" || l.IsSidechain || l.Message.Model == "<synthetic>" {
		return ""
	}
	return l.Message.Model
}

type block struct {
	Type      string          `json:"type"`
	Text      string          `json:"text"`
	Thinking  string          `json:"thinking"`
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Input     json.RawMessage `json:"input"`
	ToolUseID string          `json:"tool_use_id"`
	Content   json.RawMessage `json:"content"`
	IsError   bool            `json:"is_error"`
}

// Map returns the events one transcript line adds.
func Map(raw []byte) []timeline.Event {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.IsSidechain {
		return nil
	}
	return l.events()
}

func (l line) events() (events []timeline.Event) {
	if l.IsMeta || l.IsCompactSummary {
		return nil
	}
	switch l.Type {
	case "system":
		switch l.Subtype {
		case "turn_duration":
			return []timeline.Event{{Kind: "result", OK: true, DurationMs: l.DurationMs}}
		case "compact_boundary":
			return []timeline.Event{{Kind: "compacted", Trigger: l.CompactMetadata.Trigger}}
		}
	case "assistant":
		for _, b := range blocks(l.Message.Content) {
			switch {
			case b.Type == "text" && b.Text != "":
				events = append(events, timeline.Event{Kind: "assistant_text", Text: b.Text})
			case b.Type == "thinking" && b.Thinking != "":
				events = append(events, timeline.Event{Kind: "thinking", Text: b.Thinking})
			case b.Type == "tool_use":
				events = append(events, timeline.Event{Kind: "tool_start", ToolUseID: b.ID, Name: b.Name, Input: b.Input})
			}
		}
	case "user":
		// Claude writes harness input, like task notifications, as user lines; only a human's is a prompt.
		switch l.Origin.Kind {
		case "", "human":
		case "task-notification":
			return taskEnd(l.Message.Content)
		default:
			return nil
		}
		// A background call returns as it launches; it ends with its task notification.
		if task, ok := l.background(); ok {
			for _, b := range blocks(l.Message.Content) {
				if b.Type == "tool_result" {
					events = append(events, timeline.Event{Kind: "tool_background", ToolUseID: b.ToolUseID, TaskID: task})
				}
			}
			return events
		}
		return userEvents(l.Message.Content)
	}
	return events
}

// background says whether the call went on in the background, an agent launched async or a command, and the command's task id.
func (l line) background() (string, bool) {
	var r struct {
		IsAsync          bool   `json:"isAsync"`
		BackgroundTaskID string `json:"backgroundTaskId"`
	}
	if json.Unmarshal(l.ToolUseResult, &r) != nil {
		return "", false
	}
	return r.BackgroundTaskID, r.IsAsync || r.BackgroundTaskID != ""
}

// taskEnd ends the tool call a background task ran under, from the notification Claude writes when it stops.
func taskEnd(content json.RawMessage) []timeline.Event {
	var text string
	json.Unmarshal(content, &text)
	id := tag(text, "tool-use-id")
	if id == "" {
		return nil
	}
	// A task the agent stopped ended as asked, not in error.
	e := timeline.Event{Kind: "tool_end", ToolUseID: id, OK: tag(text, "status") != "failed"}
	// The result is the agent's own text, which may hold the closing tag itself.
	if _, rest, ok := strings.Cut(text, "<result>"); ok {
		if end := strings.LastIndex(rest, "</result>"); end >= 0 {
			out := html.UnescapeString(strings.TrimSpace(rest[:end]))
			e.Output = &out
		}
	}
	return []timeline.Event{e}
}

func tag(text, name string) string {
	_, rest, ok := strings.Cut(text, "<"+name+">")
	value, _, closed := strings.Cut(rest, "</"+name+">")
	if !ok || !closed {
		return ""
	}
	return strings.TrimSpace(value)
}

func userEvents(content json.RawMessage) []timeline.Event {
	var text string
	var events []timeline.Event
	if json.Unmarshal(content, &text) != nil {
		for _, b := range blocks(content) {
			switch b.Type {
			case "text":
				text += b.Text
			case "tool_result":
				out := textOf(b.Content)
				events = append(events, timeline.Event{Kind: "tool_end", ToolUseID: b.ToolUseID, OK: !b.IsError, Output: &out})
			}
		}
	}
	switch {
	case strings.HasPrefix(text, "[Request interrupted by user"):
		return append(events, timeline.Event{Kind: "result", OK: false, Error: "interrupted"})
	case strings.HasPrefix(text, "<command-"), strings.HasPrefix(text, "<local-command-"), text == "":
		return events
	}
	return append(events, timeline.Event{Kind: "user", Text: text})
}

func blocks(raw json.RawMessage) []block {
	var bs []block
	json.Unmarshal(raw, &bs)
	return bs
}

func textOf(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	var out strings.Builder
	for _, b := range blocks(raw) {
		out.WriteString(b.Text)
	}
	return out.String()
}

const poll = 100 * time.Millisecond

// Tail waits for the file at path, then calls fn with each complete line
// until ctx ends, and with the lines already written when it does.
func Tail(ctx context.Context, path string, fn func([]byte)) {
	var f *os.File
	for f == nil {
		f, _ = os.Open(path)
		if f == nil && !sleep(ctx) {
			return
		}
	}
	defer f.Close()
	r := bufio.NewReader(f)
	var partial []byte
	for {
		chunk, err := r.ReadBytes('\n')
		partial = append(partial, chunk...)
		if err == nil {
			fn(partial[:len(partial)-1])
			partial = nil
			continue
		}
		if err != io.EOF || ctx.Err() != nil {
			return
		}
		sleep(ctx)
	}
}

func sleep(ctx context.Context) bool {
	select {
	case <-ctx.Done():
		return false
	case <-time.After(poll):
		return true
	}
}
