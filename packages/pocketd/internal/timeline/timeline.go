// Package timeline folds provider events into the items the phone shows.
package timeline

import (
	"encoding/json"
	"strconv"
	"sync"

	"pocketd/internal/proto"
)

type Event struct {
	Kind       string // user, assistant_text, thinking, tool_start, tool_end, result, compacted, error
	Text       string
	ToolUseID  string
	Name       string
	Input      json.RawMessage
	Detail     *proto.ToolDetail // set by providers that build it themselves (Codex)
	OK         bool
	Output     *string
	DurationMs int64
	Error      string
	Trigger    string
	Usage      *proto.TurnUsage
}

type Timeline struct {
	mu       sync.Mutex
	items    []proto.Item
	epoch    int64
	seq      int64
	tools    map[string]int
	openText int
	folded   map[string]bool
}

func New() *Timeline {
	t := &Timeline{}
	t.reset()
	return t
}

func (t *Timeline) reset() {
	t.tools = map[string]int{}
	t.folded = map[string]bool{}
	t.openText = -1
}

// StartEpoch keeps a new run from merging into text or tools left open by the last one.
func (t *Timeline) StartEpoch() int64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.epoch++
	t.reset()
	return t.epoch
}

// Clear drops a finished conversation. seq keeps counting so a phone's sinceSeq never matches a new item.
func (t *Timeline) Clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.items = nil
	t.epoch++
	t.reset()
}

func (t *Timeline) State() (epoch, maxSeq int64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.epoch, t.seq
}

func (t *Timeline) append(it proto.Item, ts int64) proto.Item {
	t.seq++
	it.ID, it.Seq, it.Ts = "i"+strconv.FormatInt(t.seq, 10), t.seq, ts
	t.items = append(t.items, it)
	t.openText = -1
	return it
}

// Apply returns the new or changed item, or false when nothing visible changed.
func (t *Timeline) Apply(e Event, ts int64) (proto.Item, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	switch e.Kind {
	case "user":
		return t.append(proto.Item{Kind: "user", Text: e.Text}, ts), true

	case "assistant_text", "thinking":
		kind := "assistant"
		if e.Kind == "thinking" {
			kind = "thinking"
		}
		if t.openText >= 0 && t.items[t.openText].Kind == kind {
			t.items[t.openText].Text += e.Text
			return t.items[t.openText], true
		}
		it := t.append(proto.Item{Kind: kind, Text: e.Text}, ts)
		t.openText = len(t.items) - 1
		return it, true

	case "tool_start":
		if e.Name == "TodoWrite" {
			if tasks, ok := taskItems(e.Input); ok {
				t.folded[e.ToolUseID] = true
				return t.append(proto.Item{Kind: "tasks", Tasks: tasks}, ts), true
			}
		}
		if e.Name == "ExitPlanMode" {
			if plan, ok := planText(e.Input); ok {
				t.folded[e.ToolUseID] = true
				return t.append(proto.Item{Kind: "plan", Text: plan}, ts), true
			}
		}
		detail := Detail(e.Name, e.Input)
		if e.Detail != nil {
			detail = *e.Detail
		}
		it := t.append(proto.Item{Kind: "tool", Call: &proto.ToolCall{ToolUseID: e.ToolUseID, Name: e.Name, Detail: detail, Status: "running"}}, ts)
		t.tools[e.ToolUseID] = len(t.items) - 1
		return it, true

	case "tool_end":
		n, ok := t.tools[e.ToolUseID]
		if t.folded[e.ToolUseID] || !ok {
			return proto.Item{}, false
		}
		call := *t.items[n].Call
		call.Status = "error"
		if e.OK {
			call.Status = "ok"
		}
		d := ts - t.items[n].Ts
		call.DurationMs = &d
		if e.Output != nil {
			out := clamp(*e.Output)
			call.Output = &out
		}
		// Published items are shared with subscribers, so replace the call instead of mutating it.
		t.items[n].Call = &call
		return t.items[n], true

	case "result":
		return t.append(proto.Item{Kind: "result", OK: e.OK, DurationMs: e.DurationMs, Error: e.Error, Usage: e.Usage}, ts), true

	case "compacted":
		return t.append(proto.Item{Kind: "compact", Trigger: e.Trigger}, ts), true

	case "error":
		return t.append(proto.Item{Kind: "result", OK: false, Error: e.Error}, ts), true
	}
	return proto.Item{}, false
}

// Page returns up to limit items newer than sinceSeq, newest last.
func (t *Timeline) Page(sinceSeq int64, limit int) ([]proto.Item, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	var newer []proto.Item
	for _, it := range t.items {
		if it.Seq > sinceSeq {
			newer = append(newer, it)
		}
	}
	items := newer[len(newer)-min(len(newer), max(0, limit)):]
	return append([]proto.Item(nil), items...), len(items) < len(newer) || sinceSeq > 0
}

func clamp(out string) string {
	if len(out) <= proto.ToolOutputLimit {
		return out
	}
	cut := proto.ToolOutputLimit
	for cut > 0 && out[cut]&0xC0 == 0x80 {
		cut--
	}
	return out[:cut] + "\n… truncated"
}

func taskItems(raw json.RawMessage) ([]proto.TaskItem, bool) {
	var in struct {
		Todos []json.RawMessage `json:"todos"`
	}
	if json.Unmarshal(raw, &in) != nil || in.Todos == nil {
		return nil, false
	}
	items := make([]proto.TaskItem, len(in.Todos))
	for n, todo := range in.Todos {
		var entry toolInput
		_ = json.Unmarshal(todo, &entry)
		status := entry.get("status")
		if status != "in_progress" && status != "completed" {
			status = "pending"
		}
		items[n] = proto.TaskItem{Text: entry.get("content"), Status: status}
	}
	return items, true
}

func planText(raw json.RawMessage) (string, bool) {
	var in struct {
		Plan *string `json:"plan"`
	}
	if json.Unmarshal(raw, &in) != nil || in.Plan == nil {
		return "", false
	}
	return *in.Plan, true
}
