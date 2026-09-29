// Package claude turns Claude Code's JSONL transcript into timeline events.
package claude

import (
	"bufio"
	"context"
	"encoding/json"
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
	AITitle          string `json:"aiTitle"`
	Summary          string `json:"summary"`
	DurationMs       int64  `json:"durationMs"`
	CompactMetadata  struct {
		Trigger string `json:"trigger"`
	} `json:"compactMetadata"`
	Message struct {
		Model   string          `json:"model"`
		Content json.RawMessage `json:"content"`
	} `json:"message"`
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

// Map returns the events one transcript line adds, and the session title if
// the line sets one.
func Map(raw []byte) (events []timeline.Event, title string) {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.IsMeta || l.IsSidechain || l.IsCompactSummary {
		return nil, ""
	}
	switch l.Type {
	case "ai-title":
		return nil, l.AITitle
	case "summary":
		return nil, l.Summary
	case "system":
		switch l.Subtype {
		case "turn_duration":
			return []timeline.Event{{Kind: "result", OK: true, DurationMs: l.DurationMs}}, ""
		case "compact_boundary":
			return []timeline.Event{{Kind: "compacted", Trigger: l.CompactMetadata.Trigger}}, ""
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
		return userEvents(l.Message.Content), ""
	}
	return events, ""
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
