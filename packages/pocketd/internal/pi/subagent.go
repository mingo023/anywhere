package pi

import (
	"encoding/json"
	"maps"
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// delegation is what a call tells of the sub-agents it runs, the whole of it
// on every update: omp's task tool a snapshot of each, the subagent extension
// pi ships each one's messages so far.
type delegation struct {
	Progress []struct {
		ID          string `json:"id"`
		Status      string `json:"status"`
		Assignment  string `json:"assignment"`
		Model       string `json:"resolvedModel"`
		ToolCount   int    `json:"toolCount"`
		RecentTools []struct {
			Tool    string `json:"tool"`
			Args    string `json:"args"`
			ArgsKey string `json:"argsKey"`
			IsError bool   `json:"isError"`
		} `json:"recentTools"`
		CurrentTool    string   `json:"currentTool"`
		CurrentArgs    string   `json:"currentToolArgs"`
		CurrentArgsKey string   `json:"currentToolArgsKey"`
		RecentOutput   []string `json:"recentOutput"` // its last lines, newest first
	} `json:"progress"`
	Results []struct {
		// omp's.
		ID     string `json:"id"`
		Output string `json:"output"`
		// The extension's.
		Task       string    `json:"task"`
		ExitCode   int       `json:"exitCode"`
		StopReason string    `json:"stopReason"`
		Model      string    `json:"model"`
		Messages   []message `json:"messages"`
		Usage      struct {
			Context int64 `json:"contextTokens"`
		} `json:"usage"`
	} `json:"results"`
	Async *struct {
		State string `json:"state"`
	} `json:"async"`
	Mode string `json:"mode"` // the extension's
}

// task is a call that runs sub-agents, as far as the timeline has it.
type task struct {
	several bool // each sub-agent shows as a call of its own under it
	lasting bool // its sub-agents run on past it
	agents  map[int]*subAgent
}

// subAgent is how far a sub-agent's work is in the timeline.
type subAgent struct {
	row      string   // the call its work shows under
	messages int      // how many of its messages are there
	tools    int      // the number of the last of its tools that ended there
	running  string   // the tool it runs, shown running
	calls    []string // the tools its messages started that have no result yet
	model    string
	tokens   int64
	report   string
	ended    bool
}

// delegated shows the work of the sub-agents call to tool runs, as its result
// tells it: each under the call, or under a call of its own when the call
// runs several. A call whose sub-agents ran on in the background ends with
// them. It reports whether they run on past the result, which done says is the call's last.
func (s *Session) delegated(call, tool string, args, result json.RawMessage, done bool) (background bool) {
	if name, _ := describe(tool, nil); name != "task" {
		return false
	}
	var r struct {
		Content json.RawMessage `json:"content"`
		Details delegation      `json:"details"`
	}
	if json.Unmarshal(result, &r) != nil {
		return false
	}
	d := r.Details
	t := s.tasks[call]
	n := max(len(d.Progress), len(d.Results))
	if n == 0 {
		// A call stopped mid-way tells nothing of its sub-agents.
		if t != nil && done {
			for _, i := range slices.Sorted(maps.Keys(t.agents)) {
				s.endAgent(t, t.agents[i], false)
			}
		}
		return false
	}
	if t == nil {
		var in struct {
			Tasks, Chain []json.RawMessage
		}
		json.Unmarshal(object(args), &in)
		t = &task{several: max(n, len(in.Tasks), len(in.Chain)) > 1, agents: map[int]*subAgent{}}
		s.tasks[call] = t
	}
	// agent is the i-th sub-agent, which starts on brief.
	agent := func(i int, name, brief string) *subAgent {
		a := t.agents[i]
		if a != nil {
			return a
		}
		a = &subAgent{row: call}
		t.agents[i] = a
		if t.several {
			a.row = call + "/" + strconv.Itoa(i)
			s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: a.row, Name: "task", Detail: &proto.ToolDetail{Kind: "task", Description: name}, ParentToolUseID: call})
		}
		if brief != "" {
			s.sink.Apply(timeline.Event{Kind: "user", Text: brief, ParentToolUseID: a.row})
		}
		return a
	}
	model := func(a *subAgent, name string) {
		if name != "" && name != a.model {
			a.model = name
			s.sink.Apply(timeline.Event{Kind: "tool_model", ToolUseID: a.row, Name: name})
		}
	}
	for i, p := range d.Progress {
		a := agent(i, p.ID, p.Assignment)
		if a.ended {
			continue
		}
		model(a, p.Model)
		running := 0
		if p.CurrentTool != "" {
			running = 1
		}
		// toolCount counts the tools it ended and the one it runs; recentTools is the tail of those it ended, newest first.
		last := p.ToolCount - running
		for j, tool := range slices.Backward(p.RecentTools) {
			if n := last - j; n > a.tools {
				id := a.row + "/" + strconv.Itoa(n)
				s.subTool(a, id, tool.Tool, tool.ArgsKey, tool.Args)
				s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: id, OK: !tool.IsError})
				a.running = ""
			}
		}
		a.tools = max(a.tools, last)
		if running == 1 {
			s.subTool(a, a.row+"/"+strconv.Itoa(p.ToolCount), p.CurrentTool, p.CurrentArgsKey, p.CurrentArgs)
		}
		var lines []string
		for _, line := range slices.Backward(p.RecentOutput) {
			lines = append(lines, line)
		}
		a.report = strings.Join(lines, "\n")
		for _, res := range d.Results {
			if res.ID == p.ID && res.Output != "" {
				a.report = res.Output
			}
		}
		if p.Status != "completed" && p.Status != "failed" && p.Status != "aborted" {
			continue
		}
		if a.running != "" {
			s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: a.running, OK: p.Status == "completed"})
			a.running = ""
		}
		s.endAgent(t, a, p.Status == "completed")
	}
	if len(d.Progress) == 0 {
		for i, res := range d.Results {
			a := agent(i, res.Task, res.Task)
			model(a, res.Model)
			for _, m := range res.Messages[min(a.messages, len(res.Messages)):] {
				s.subMessage(a, m)
			}
			a.messages = len(res.Messages)
			if c := res.Usage.Context; c > 0 && c != a.tokens {
				a.tokens = c
				s.sink.Apply(timeline.Event{Kind: "tool_tokens", ToolUseID: a.row, Tokens: c})
			}
			// The extension tells a sub-agent's end only by the call's, or in a chain by the next one's start.
			if done || d.Mode == "chain" && i < len(d.Results)-1 {
				s.endAgent(t, a, res.ExitCode == 0 && res.StopReason != "error" && res.StopReason != "aborted")
			}
		}
	}
	if d.Async == nil {
		return false
	}
	if d.Async.State != "running" && t.lasting {
		t.lasting = false
		out := nonEmpty(text(r.Content))
		if !t.several {
			out = nonEmpty(t.agents[0].report)
		}
		s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: call, OK: d.Async.State == "completed", Output: out})
	}
	if done && d.Async.State == "running" {
		t.lasting = true
	}
	return d.Async.State == "running"
}

// endAgent ends a sub-agent of t, and the tools it left running.
func (s *Session) endAgent(t *task, a *subAgent, ok bool) {
	if a.ended {
		return
	}
	a.ended = true
	for _, id := range append(a.calls, a.running) {
		if id != "" {
			s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: id, OK: false})
		}
	}
	a.calls, a.running = nil, ""
	if t.several {
		s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: a.row, OK: ok, Output: nonEmpty(a.report)})
	}
}

// subTool shows the tool id a sub-agent runs, once.
func (s *Session) subTool(a *subAgent, id, name, key, args string) {
	if a.running == id {
		return
	}
	a.running = id
	input, _ := json.Marshal(map[string]string{key: args})
	name, detail := describe(name, input)
	s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: id, Name: name, Detail: &detail, ParentToolUseID: a.row})
}

// subMessage shows a message of a sub-agent's conversation, but for the prompt it started on.
func (s *Session) subMessage(a *subAgent, m message) {
	switch m.Role {
	case "assistant":
		for _, b := range m.blocks() {
			switch b.Type {
			case "text":
				a.report = b.Text
				s.sink.Apply(timeline.Event{Kind: "assistant_text", Text: b.Text, ParentToolUseID: a.row})
			case "thinking":
				s.sink.Apply(timeline.Event{Kind: "thinking", Text: b.Thinking, ParentToolUseID: a.row})
			case "toolCall":
				name, detail := describe(b.Name, object(b.Arguments))
				a.calls = append(a.calls, a.row+"/"+b.ID)
				s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: a.row + "/" + b.ID, Name: name, Detail: &detail, ParentToolUseID: a.row})
			}
		}
	case "toolResult":
		a.calls = slices.DeleteFunc(a.calls, func(id string) bool { return id == a.row+"/"+m.ToolCallID })
		s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: a.row + "/" + m.ToolCallID, OK: !m.IsError, Output: nonEmpty(text(m.Content))})
	}
}
