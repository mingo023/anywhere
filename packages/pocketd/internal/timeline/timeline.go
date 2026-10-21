// Package timeline folds provider events into the items the phone shows.
package timeline

import (
	"encoding/json"
	"slices"
	"strconv"
	"strings"
	"sync"

	"pocketd/internal/proto"
)

type Event struct {
	Kind       string // user, assistant_text, thinking, assistant_draft, thinking_draft, tool_start, tool_model, tool_background, tool_end, result, compacted, error
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
	// ParentToolUseID tags a sub-agent's event with the tool call that spawned it.
	ParentToolUseID string
	// TaskID names a call sent to the background, for the calls that later read or stop it.
	// On a tool_end without ToolUseID it names the call that ended.
	TaskID string
	// Draft names the streamed block a draft's text belongs to.
	Draft string
}

type Timeline struct {
	mu       sync.Mutex
	items    []proto.Item
	epoch    int64
	seq      int64
	resets   int64 // Clears so far, so a client knows to drop what it holds
	tools    map[string]int
	openText int
	folded   map[string]bool
	// lasting holds the calls that run on in the background past their turn.
	lasting map[string]bool
	// tasks maps a background call's task id to its index in items.
	tasks map[string]int
	// drafts holds, per kind, the items Claude's stream opened that its transcript hasn't written yet, oldest first.
	drafts map[string][]int
	// draft names the streamed block each open draft item grows from.
	draft map[int]string
}

func New() *Timeline {
	t := &Timeline{}
	t.reset()
	t.forget()
	return t
}

func (t *Timeline) forget() {
	t.lasting = map[string]bool{}
	t.tasks = map[string]int{}
	t.closeDrafts()
}

// closeDrafts stops waiting for the transcript to write the open drafts; a draft it never writes, as thinking it
// keeps to itself, stays as it streamed.
func (t *Timeline) closeDrafts() {
	t.drafts = map[string][]int{}
	t.draft = map[int]string{}
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
	running := t.tools
	t.reset()
	// A background call and its sub-agent's tools outlive the turn that spawned them.
	for id, n := range running {
		if t.items[n].ParentToolUseID != "" || t.lasting[id] {
			t.tools[id] = n
		}
	}
	return t.epoch
}

// Clear drops a finished conversation. seq keeps counting so a phone's sinceSeq never matches a new item.
func (t *Timeline) Clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.items = nil
	t.epoch++
	t.resets++
	t.reset()
	t.forget()
}

func (t *Timeline) Resets() int64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.resets
}

// Continue picks up a timeline from the image before an upgrade: a new
// epoch so phones refetch, and seq past every item they hold.
func (t *Timeline) Continue(epoch, seq int64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.epoch, t.seq = epoch+1, seq
}

func (t *Timeline) State() (epoch, maxSeq int64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.epoch, t.seq
}

func (t *Timeline) append(it proto.Item, parent string, ts int64) proto.Item {
	t.seq++
	it.ID, it.Seq, it.Ts, it.ParentToolUseID = "i"+strconv.FormatInt(t.seq, 10), t.seq, ts, parent
	t.items = append(t.items, it)
	t.openText = -1
	return it
}

// last is the newest item outside sub-agents, skipping compactions.
func (t *Timeline) last() proto.Item {
	for i := len(t.items) - 1; i >= 0; i-- {
		if it := t.items[i]; it.ParentToolUseID == "" && it.Kind != "compact" {
			return it
		}
	}
	return proto.Item{}
}

// Unanswered says whether the newest prompt has nothing after it yet.
func (t *Timeline) Unanswered() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.last().Kind == "user"
}

// Apply returns the new or changed item, or false when nothing visible changed.
func (t *Timeline) Apply(e Event, ts int64) (proto.Item, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	switch e.Kind {
	case "user":
		return t.append(proto.Item{Kind: "user", Text: e.Text}, e.ParentToolUseID, ts), true

	case "assistant_draft", "thinking_draft":
		kind := textKind(strings.TrimSuffix(e.Kind, "_draft"))
		open := t.drafts[kind]
		if n := len(open); n > 0 && t.draft[open[n-1]] == e.Draft {
			t.items[open[n-1]].Text += e.Text
			return t.items[open[n-1]], true
		}
		it := t.append(proto.Item{Kind: kind, Text: e.Text}, "", ts)
		t.drafts[kind] = append(open, len(t.items)-1)
		t.draft[len(t.items)-1] = e.Draft
		return it, true

	case "assistant_text", "thinking":
		kind := textKind(e.Kind)
		// The transcript writes the blocks the stream drafted in the order it streamed them.
		if open := t.drafts[kind]; e.ParentToolUseID == "" && len(open) > 0 {
			n := open[0]
			t.drafts[kind] = open[1:]
			delete(t.draft, n)
			t.items[n].Text = e.Text
			t.openText = -1
			return t.items[n], true
		}
		if t.openText >= 0 && t.items[t.openText].Kind == kind && t.items[t.openText].ParentToolUseID == e.ParentToolUseID {
			t.items[t.openText].Text += e.Text
			return t.items[t.openText], true
		}
		it := t.append(proto.Item{Kind: kind, Text: e.Text}, e.ParentToolUseID, ts)
		t.openText = len(t.items) - 1
		return it, true

	case "tool_start":
		if e.Name == "TodoWrite" {
			if tasks, ok := taskItems(e.Input); ok {
				t.folded[e.ToolUseID] = true
				return t.append(proto.Item{Kind: "tasks", Tasks: tasks}, e.ParentToolUseID, ts), true
			}
		}
		if e.Name == "ExitPlanMode" {
			if plan, ok := planText(e.Input); ok {
				t.folded[e.ToolUseID] = true
				return t.append(proto.Item{Kind: "plan", Text: plan}, e.ParentToolUseID, ts), true
			}
		}
		detail := Detail(e.Name, e.Input)
		if e.Detail != nil {
			detail = *e.Detail
		}
		if n, ok := t.tasks[taskOf(e.Name, e.Input)]; ok {
			if command := t.items[n].Call.Detail.Command; command != "" {
				detail = proto.ToolDetail{Kind: "shell", Command: command}
			}
		}
		it := t.append(proto.Item{Kind: "tool", Call: &proto.ToolCall{ToolUseID: e.ToolUseID, Name: e.Name, Detail: detail, Status: "running"}}, e.ParentToolUseID, ts)
		t.tools[e.ToolUseID] = len(t.items) - 1
		return it, true

	case "tool_model":
		n := t.call(e.ToolUseID)
		if n < 0 || t.items[n].Call.Detail.Kind != "task" || t.items[n].Call.Detail.Model == e.Name {
			return proto.Item{}, false
		}
		call := *t.items[n].Call
		call.Detail.Model = e.Name
		t.items[n].Call = &call
		return t.items[n], true

	case "tool_background":
		if n, ok := t.tools[e.ToolUseID]; ok {
			t.lasting[e.ToolUseID] = true
			if e.TaskID != "" {
				t.tasks[e.TaskID] = n
			}
		}
		return proto.Item{}, false

	case "tool_end":
		if n, ok := t.tasks[e.TaskID]; ok && e.ToolUseID == "" {
			e.ToolUseID = t.items[n].Call.ToolUseID
		}
		n, ok := t.tools[e.ToolUseID]
		if t.folded[e.ToolUseID] || !ok {
			return proto.Item{}, false
		}
		delete(t.lasting, e.ToolUseID)
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
		if e.ParentToolUseID == "" {
			t.closeDrafts()
		}
		// A stop pocketd ended for Claude may be marked again by Claude itself.
		if e.Error == "interrupted" && e.ParentToolUseID == "" && t.last().Kind == "result" && t.last().Error == "interrupted" {
			return proto.Item{}, false
		}
		return t.append(proto.Item{Kind: "result", OK: e.OK, DurationMs: e.DurationMs, Error: e.Error, Usage: e.Usage}, e.ParentToolUseID, ts), true

	case "compacted":
		return t.append(proto.Item{Kind: "compact", Trigger: e.Trigger}, e.ParentToolUseID, ts), true

	case "error":
		return t.append(proto.Item{Kind: "result", OK: false, Error: e.Error}, e.ParentToolUseID, ts), true
	}
	return proto.Item{}, false
}

func textKind(kind string) string {
	if kind == "thinking" {
		return "thinking"
	}
	return "assistant"
}

// call is the index of the tool call toolUseID names, or -1. A call's sub-agent outlives the epoch it began in.
func (t *Timeline) call(toolUseID string) int {
	if n, ok := t.tools[toolUseID]; ok {
		return n
	}
	for n := len(t.items) - 1; n >= 0; n-- {
		if c := t.items[n].Call; c != nil && c.ToolUseID == toolUseID {
			return n
		}
	}
	return -1
}

// Lasting returns the task ids of the calls running on in the background, sub-agents and commands, oldest first.
func (t *Timeline) Lasting() []string {
	t.mu.Lock()
	defer t.mu.Unlock()
	var ids []string
	for id, n := range t.tasks {
		if t.lasting[t.items[n].Call.ToolUseID] {
			ids = append(ids, id)
		}
	}
	slices.SortFunc(ids, func(a, b string) int { return t.tasks[a] - t.tasks[b] })
	return ids
}

// BackgroundAgents counts the sub-agents running on in the background past their turn.
func (t *Timeline) BackgroundAgents() int {
	t.mu.Lock()
	defer t.mu.Unlock()
	n := 0
	for id := range t.lasting {
		if i, ok := t.tools[id]; ok && t.items[i].Call.Detail.Kind == "task" {
			n++
		}
	}
	return n
}

// Page returns up to limit of the agent's own items newer than sinceSeq, newest last.
func (t *Timeline) Page(sinceSeq int64, limit int) ([]proto.Item, bool) {
	return t.page(sinceSeq, limit, false)
}

// PageWithSubagents is Page with its sub-agents' items too.
func (t *Timeline) PageWithSubagents(sinceSeq int64, limit int) ([]proto.Item, bool) {
	return t.page(sinceSeq, limit, true)
}

func (t *Timeline) page(sinceSeq int64, limit int, subagents bool) ([]proto.Item, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	var newer []proto.Item
	for _, it := range t.items {
		if it.Seq > sinceSeq && (subagents || it.ParentToolUseID == "") {
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
