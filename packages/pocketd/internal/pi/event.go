package pi

import (
	"cmp"
	"encoding/json"
	"fmt"
	"slices"
	"strconv"
	"strings"
	"time"

	"pocketd/internal/teamrules"
	"pocketd/internal/timeline"
)

// message is one of pi's AgentMessages, as an event or its history tells it.
type message struct {
	Role         string          `json:"role"`
	Content      json.RawMessage `json:"content"` // text, or blocks
	StopReason   string          `json:"stopReason"`
	ErrorMessage string          `json:"errorMessage"`
	Usage        *struct {
		Input     int   `json:"input"`
		Output    int   `json:"output"`
		CacheRead int   `json:"cacheRead"`
		Total     int64 `json:"totalTokens"`
	} `json:"usage"`
	// A toolResult's.
	ToolCallID string `json:"toolCallId"`
	IsError    bool   `json:"isError"`
	// A custom message's: whether pi's own TUI shows it.
	Display bool `json:"display"`
}

type content struct {
	Type      string          `json:"type"` // text, thinking or toolCall
	Text      string          `json:"text"`
	Thinking  string          `json:"thinking"`
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Arguments json.RawMessage `json:"arguments"`
}

func (m message) blocks() []content {
	var blocks []content
	json.Unmarshal(m.Content, &blocks)
	return blocks
}

// text is the text of content: a string, or the text blocks of a list.
func text(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	var blocks []content
	json.Unmarshal(raw, &blocks)
	var parts []string
	for _, b := range blocks {
		if b.Type == "text" {
			parts = append(parts, b.Text)
		}
	}
	return strings.Join(parts, "")
}

// event is what the events of a run carry, beside their type.
type event struct {
	Message message `json:"message"`
	Update  struct {
		Type     string   `json:"type"`
		Index    int      `json:"contentIndex"`
		Delta    string   `json:"delta"`
		Content  string   `json:"content"`
		ToolCall *content `json:"toolCall"`
	} `json:"assistantMessageEvent"`
	ToolCallID   string          `json:"toolCallId"`
	ToolName     string          `json:"toolName"`
	Args         json.RawMessage `json:"args"`
	Result       json.RawMessage `json:"result"`
	IsError      bool            `json:"isError"`
	WillRetry    bool            `json:"willRetry"`
	IsTerminal   *bool           `json:"isTerminal"` // omp's, false for a run it goes on from
	Aborted      bool            `json:"aborted"`
	Reason       string          `json:"reason"`
	ErrorMessage string          `json:"errorMessage"`
	Attempt      int             `json:"attempt"`
	MaxAttempts  int             `json:"maxAttempts"`
	Success      bool            `json:"success"`
	FinalError   string          `json:"finalError"`
	Name         string          `json:"name"`
	Level        string          `json:"level"`
	Error        string          `json:"error"`
}

// Event takes what pi tells of its runs, and what its extensions ask.
func (s *Session) Event(kind string, raw json.RawMessage) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if kind == "extension_ui_request" {
		if s.ready {
			s.ui(raw)
		} else {
			s.held = append(s.held, func() { s.ui(raw) })
		}
		return
	}
	var e event
	json.Unmarshal(raw, &e)
	// What pi said before its state was read, it said to a pocketd before
	// this one: its history has it, but for a run that ended since its state said it ran.
	if !s.ready {
		switch kind {
		case "agent_start":
			s.ended = false
		case "agent_end":
			s.ended = !e.WillRetry && (e.IsTerminal == nil || *e.IsTerminal)
		case "agent_settled":
			s.ended = true
		}
		return
	}
	if s.f.omp && s.ompEvent(kind, raw) {
		return
	}
	switch kind {
	// pi starts a run by itself for a prompt it held, or for an extension.
	case "agent_start":
		if s.active = true; !s.running {
			s.running, s.began = true, time.Now()
			s.sink.Working()
		}
	case "message_start":
		switch e.Message.Role {
		case "user":
			s.echo(text(e.Message.Content))
		case "assistant":
			s.done = map[int]bool{}
		}
	case "message_update":
		switch u := e.Update; u.Type {
		case "text_delta":
			s.chunk("assistant", u.Index, u.Delta)
		case "thinking_delta":
			s.chunk("thinking", u.Index, u.Delta)
		case "text_end":
			s.said("assistant", u.Index, u.Content)
		case "thinking_end":
			s.said("thinking", u.Index, u.Content)
		case "toolcall_end":
			if u.ToolCall != nil {
				s.show(u.ToolCall.ID, u.ToolCall.Name, u.ToolCall.Arguments)
			}
		}
	case "message_end":
		if e.Message.Role == "assistant" {
			s.assistant(e.Message)
		}
	case "tool_execution_start":
		s.show(e.ToolCallID, e.ToolName, e.Args)
	case "tool_execution_end":
		s.show(e.ToolCallID, e.ToolName, e.Args)
		s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: e.ToolCallID, OK: !e.IsError, Output: output(e.Result)})
	case "auto_retry_start":
		s.retrying, s.failure = true, nil
		s.notice(strings.TrimSuffix(fmt.Sprintf("%s is trying again (%d/%d): %s", s.name, e.Attempt, e.MaxAttempts, e.ErrorMessage), ": "))
	case "auto_retry_end":
		if s.retrying = false; !e.Success && e.FinalError != "" {
			s.failure = &e.FinalError
		}
	case "compaction_start":
		if !s.compacting {
			s.compacting = true
			s.sink.Compacting()
		}
	case "compaction_end":
		// pi compacted to try the reply that overflowed again.
		if e.WillRetry {
			s.failure = nil
		}
		s.compacted(len(e.Result) > 0 && string(e.Result) != "null" && !e.Aborted, e.Result, e.Reason, e.ErrorMessage)
	// A run's end is the turn's unless pi goes on by itself: to try again, or
	// to compact a context that overflowed. A pi that tells when it settles
	// ends its turns there, and takes no new turn before; an older one ends
	// them here, but for a turn being stopped, which abort's response ends.
	case "agent_end":
		if e.WillRetry {
			s.failure = nil
		} else if !s.settles && !s.retrying && !s.compacting && !s.stopping {
			s.settle(false)
		}
	case "agent_settled":
		s.settles = true
		s.settle(e.Aborted)
	case "session_info_changed":
		if e.Name != "" {
			s.sink.SetTitle(e.Name)
		}
	case "thinking_level_changed":
		if e.Level != "" {
			s.sink.SetEffort(e.Level)
		}
	case "extension_error":
		s.notice(plain(e.Error))
	}
}

// ompEvent takes the events omp has and pi doesn't. It reports whether kind was one.
func (s *Session) ompEvent(kind string, raw json.RawMessage) bool {
	var e struct {
		ID            string          `json:"id"`
		Command       string          `json:"command"`
		Success       bool            `json:"success"`
		Error         json.RawMessage `json:"error"`
		SessionID     string          `json:"sessionId"`
		Title         string          `json:"title"`
		Model         model           `json:"model"`
		ThinkingLevel string          `json:"thinkingLevel"`
		Text          string          `json:"text"`
		AgentInvoked  *bool           `json:"agentInvoked"`
		IsTerminal    *bool           `json:"isTerminal"`
		Message       message         `json:"message"`
	}
	json.Unmarshal(raw, &e)
	switch kind {
	case "available_commands_update":
		s.commands = commands(raw, false)
	// omp moved to another session, as a fork of this one, or titled the one it is on.
	case "session_info_update":
		if e.SessionID != "" {
			s.sink.Bound(e.SessionID)
		}
		if e.Title != "" {
			s.sink.SetTitle(e.Title)
		}
	case "config_update":
		if name := e.Model.name(); name != "" {
			s.sink.SetModel(name)
		}
		if e.ThinkingLevel != "" {
			s.sink.SetEffort(e.ThinkingLevel)
		}
	case "command_output":
		s.notice(plain(e.Text))
	// A prompt omp took as a command of its own runs no agent, so nothing else ends its turn.
	case "prompt_result":
		if e.AgentInvoked != nil && !*e.AgentInvoked {
			s.handled(e.ID)
		}
	// omp answers a prompt again when what it took it for fails.
	case "response":
		if e.Command == "prompt" && !e.Success {
			s.take(e.ID)
			s.fail(e.ID, text(e.Error))
		}
	// omp ends a run it goes on from.
	case "agent_end":
		return e.IsTerminal != nil && !*e.IsTerminal
	// What an extension put in the conversation, which omp tells as a message.
	case "message_start", "message_end":
		if e.Message.Role != "custom" {
			return false
		}
		if kind == "message_start" && e.Message.Display {
			s.notice(plain(text(e.Message.Content)))
		}
	default:
		return false
	}
	return true
}

// echo is a prompt pi read: the oldest it took, as the user wrote it rather
// than as pi expanded a skill or a template in it.
func (s *Session) echo(said string) {
	if len(s.sent) > 0 {
		said = s.sent[0].text
		s.sent = s.sent[1:]
	}
	s.user(said)
}

func (s *Session) user(said string) {
	s.flush()
	s.sink.Apply(timeline.Event{Kind: "user", Text: teamrules.Strip(said)})
}

// chunk adds a delta to the block pi is writing, which drafts in the chat until it is whole.
func (s *Session) chunk(kind string, index int, delta string) {
	if s.done[index] {
		return
	}
	if !s.msg.open || s.msg.kind != kind || s.msg.index != index {
		s.flush()
		s.blocks++
		s.msg = block{kind: kind, index: index, open: true}
	}
	if delta == "" {
		return
	}
	s.msg.text += delta
	s.sink.Apply(timeline.Event{Kind: kind + "_draft", Text: delta, Draft: strconv.Itoa(s.blocks)})
}

// said writes block index of pi's message whole, in place of what drafted of it.
func (s *Session) said(kind string, index int, whole string) {
	if s.done[index] {
		return
	}
	if s.msg.open && s.msg.index == index {
		whole = cmp.Or(whole, s.msg.text)
	} else {
		s.flush()
	}
	s.msg = block{}
	s.done[index] = true
	if whole == "" {
		return
	}
	if kind == "assistant" {
		kind = "assistant_text"
	}
	s.sink.Apply(timeline.Event{Kind: kind, Text: whole})
}

// flush writes the block pi was writing, now that something else follows it.
func (s *Session) flush() {
	if b := s.msg; b.open {
		s.said(b.kind, b.index, b.text)
	}
}

// assistant takes a reply pi ended: the blocks that didn't stream, what it
// cost, and the error it ended with, which is the turn's unless pi tries again.
func (s *Session) assistant(m message) {
	for i, b := range m.blocks() {
		s.block(i, b)
	}
	s.flush()
	s.done = map[int]bool{}
	if u := m.Usage; u != nil {
		s.usage.InputTokens += u.Input
		s.usage.OutputTokens += u.Output
		s.usage.CacheReadTokens += u.CacheRead
		if u.Total > 0 {
			s.sink.SetTokens(u.Total, s.window)
		}
	}
	if m.StopReason == "error" {
		s.failure = &m.ErrorMessage
	}
}

func (s *Session) block(index int, b content) {
	switch b.Type {
	case "text":
		s.said("assistant", index, b.Text)
	case "thinking":
		s.said("thinking", index, b.Thinking)
	case "toolCall":
		s.show(b.ID, b.Name, b.Arguments)
	}
}

// show starts the call named id in the timeline, once.
func (s *Session) show(id, name string, args json.RawMessage) {
	if id == "" || s.tools[id] {
		return
	}
	s.flush()
	s.tools[id] = true
	args = object(args)
	if input, ok := todos(name, args); ok {
		s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: id, Name: "TodoWrite", Input: input})
		return
	}
	name, detail := describe(name, args)
	s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: id, Name: name, Detail: &detail})
}

// replay puts the history pi holds in the timeline, a result after each turn
// that ended: all of them, unless pi is in the last. A last turn pi left
// without ending it went with the pocketd before this one, so it ends
// interrupted, as do the calls it left open. It reports whether the last one
// failed.
func (s *Session) replay(history []message, streaming bool) (failed bool) {
	var last *message  // the reply the turn being read ended with
	var calls []string // the calls of the turn being read that have no result
	open := false      // the turn being read has no result
	end := func() {
		if last == nil {
			return
		}
		e := timeline.Event{Kind: "result"}
		switch last.StopReason {
		// pi read the prompt that follows in the same turn, between its tools.
		case "toolUse":
			last = nil
			return
		case "error":
			e.Error = cmp.Or(last.ErrorMessage, s.name+"'s turn failed")
		case "aborted":
			e.Error = "interrupted"
		default:
			e.OK = true
		}
		failed, last, calls, open = last.StopReason == "error", nil, nil, false
		s.sink.Apply(e)
	}
	for _, m := range history {
		switch m.Role {
		case "user":
			end()
			open = true
			s.user(text(m.Content))
		case "assistant":
			s.done = map[int]bool{}
			for i, b := range m.blocks() {
				s.block(i, b)
				if b.Type == "toolCall" {
					calls = append(calls, b.ID)
				}
			}
			last, open = &m, true
		case "toolResult":
			calls = slices.DeleteFunc(calls, func(id string) bool { return id == m.ToolCallID })
			s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: m.ToolCallID, OK: !m.IsError, Output: nonEmpty(text(m.Content))})
		case "compactionSummary":
			s.sink.Apply(timeline.Event{Kind: "compacted", Trigger: "auto"})
		}
	}
	s.done = map[int]bool{}
	if streaming {
		return failed
	}
	if end(); open {
		for _, id := range calls {
			out := "Interrupted"
			s.sink.Apply(timeline.Event{Kind: "tool_end", ToolUseID: id, Output: &out})
		}
		s.sink.Apply(timeline.Event{Kind: "result", Error: "interrupted"})
	}
	return failed
}

// output is what a call's result says.
func output(result json.RawMessage) *string {
	var r struct {
		Content json.RawMessage `json:"content"`
	}
	json.Unmarshal(result, &r)
	return nonEmpty(text(r.Content))
}

func nonEmpty(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}
