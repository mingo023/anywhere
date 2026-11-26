package claude

import (
	"bytes"
	"context"
	"encoding/json"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

const scan = 500 * time.Millisecond

// MapSubagent returns the events one line of a sub-agent's transcript adds,
// tagged with parent, the tool call that spawned it.
func MapSubagent(raw []byte, parent string) []timeline.Event {
	var l line
	if json.Unmarshal(raw, &l) != nil {
		return nil
	}
	// A fork's transcript opens by replaying the call that spawned it and its result; only the parent's transcript runs that call.
	events := slices.DeleteFunc(l.events(), func(e timeline.Event) bool { return e.ToolUseID == parent })
	for i := range events {
		events[i].ParentToolUseID = parent
	}
	// Only the sub-agent's answers name the model it runs on; its parent's call shows it.
	if l.Type == "assistant" && l.Message.Model != "" && l.Message.Model != "<synthetic>" {
		head := []timeline.Event{{Kind: "tool_model", ToolUseID: parent, Name: l.Message.Model}}
		if u := l.Message.Usage; u != nil {
			head = append(head, timeline.Event{Kind: "tool_tokens", ToolUseID: parent, Tokens: u.InputTokens + u.CacheCreationInputTokens + u.CacheReadInputTokens})
		}
		events = append(head, events...)
	}
	return events
}

// Subagents follows the sub-agent transcripts Claude writes beside a
// conversation, each under the tool call that spawned it.
type Subagents struct {
	path    string
	dir     string
	mu      sync.Mutex
	calls   map[string]string // transcript → the call that spawned it, once its meta file says
	spawned map[string]bool   // calls Spawned took; true once it read their transcripts
	read    map[string]int64  // transcript → how far Spawned read it
	runs    map[string]string // Workflow call → the run it launched
	journal map[string]bool   // runs whose journal Tail follows
	settled map[string]bool   // workflow agent a restored timeline shows → whether it shows its result
}

// NewSubagents follows the sub-agents of the conversation at path.
func NewSubagents(path string) *Subagents {
	return &Subagents{
		path:    path,
		dir:     filepath.Join(strings.TrimSuffix(path, ".jsonl"), "subagents"),
		calls:   map[string]string{},
		spawned: map[string]bool{},
		read:    map[string]int64{},
		runs:    map[string]string{},
		journal: map[string]bool{},
		settled: map[string]bool{},
	}
}

// Workflow follows the agents of run, which Workflow call call launched. The
// journal Claude keeps of the run starts a call under it for each agent and
// ends it with the agent's result; Tail follows each agent's transcript under
// its own call, named by its agent id.
func (s *Subagents) Workflow(call, run string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.runs[call] = run
}

// Spawned passes fn what the sub-agents of call toolUseID wrote so far, then
// lets Tail follow them from there. Called right after the call is recorded,
// a conversation read again from its start keeps each sub-agent's items
// under their call instead of after the whole conversation.
func (s *Subagents) Spawned(toolUseID string, fn func([]timeline.Event)) {
	s.mu.Lock()
	_, taken := s.spawned[toolUseID]
	if !taken {
		s.spawned[toolUseID] = false
	}
	s.mu.Unlock()
	if taken {
		return
	}
	read := map[string]int64{}
	for _, name := range s.transcripts() {
		if s.callOf(name) == toolUseID {
			read[name] = readLines(name, func(raw []byte) { fn(MapSubagent(raw, toolUseID)) })
		}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	maps.Copy(s.read, read)
	s.spawned[toolUseID] = true
}

// Restored is Spawned for a timeline that already shows what sub-agents
// wrote up to some point, as one a restart took from the store: for each call
// in shown with a sub-agent transcript, and each task call still running,
// whose sub-agent may write one yet. fn gets only what they wrote past the
// items shown holds under their call.
func (s *Subagents) Restored(shown []proto.Item, fn func([]timeline.Event)) {
	written := map[string]bool{}
	for _, name := range s.transcripts() {
		written[s.callOf(name)] = true
	}
	runs := workflowRuns(s.path)
	under := map[string][]proto.Item{}
	var calls []string
	for _, it := range shown {
		if it.ParentToolUseID != "" {
			under[it.ParentToolUseID] = append(under[it.ParentToolUseID], it)
		}
		if c := it.Call; c != nil && runs[it.ParentToolUseID] != "" {
			s.mu.Lock()
			s.settled[c.ToolUseID] = c.Status != "running" && (c.Output == nil || *c.Output != timeline.WorkflowStopped)
			s.mu.Unlock()
		}
		if c := it.Call; c != nil && runs[c.ToolUseID] != "" {
			s.Workflow(c.ToolUseID, runs[c.ToolUseID])
		}
		if c := it.Call; c != nil && (written[c.ToolUseID] || c.Detail.Kind == "task" && c.Status == "running") {
			calls = append(calls, c.ToolUseID)
		}
	}
	for _, call := range calls {
		unseen := timeline.Unseen(under[call])
		s.Spawned(call, func(events []timeline.Event) { fn(unseen(events)) })
	}
}

// Tail tails each sub-agent transcript of a call Spawned read, from where it
// stopped. fn gets each line's events, from one goroutine per sub-agent. A
// send on kick scans at once, as for a call that just spawned one. It
// returns when ctx ends and every tail has read what was written.
func (s *Subagents) Tail(ctx context.Context, kick <-chan struct{}, fn func([]timeline.Event)) {
	var wg sync.WaitGroup
	defer wg.Wait()
	tailing := map[string]bool{}
	for {
		for call, run := range s.unfollowed() {
			wg.Add(1)
			go func() {
				defer wg.Done()
				tailFrom(ctx, filepath.Join(s.dir, "workflows", run, "journal.jsonl"), 0, func(raw []byte) { fn(s.journaled(raw, call)) })
			}()
		}
		for _, name := range s.transcripts() {
			call := s.callOf(name)
			s.mu.Lock()
			spawned, from := s.spawned[call], s.read[name]
			s.mu.Unlock()
			// Skills, and forks older Claude didn't tie to their call, run as sub-agents too, but without a tool call to nest under.
			if tailing[name] || call == "" || !spawned {
				continue
			}
			tailing[name] = true
			wg.Add(1)
			go func() {
				defer wg.Done()
				tailFrom(ctx, name, from, func(raw []byte) { fn(MapSubagent(raw, call)) })
			}()
		}
		select {
		case <-ctx.Done():
			return
		case <-kick:
		case <-time.After(scan):
		}
	}
}

func (s *Subagents) transcripts() []string {
	names, _ := filepath.Glob(filepath.Join(s.dir, "agent-*.jsonl"))
	agents, _ := filepath.Glob(filepath.Join(s.dir, "workflows", "*", "agent-*.jsonl"))
	return append(names, agents...)
}

// unfollowed is the Workflow calls, each with its run, whose journal Tail
// doesn't follow yet, marked followed.
func (s *Subagents) unfollowed() map[string]string {
	s.mu.Lock()
	defer s.mu.Unlock()
	runs := map[string]string{}
	for call, run := range s.runs {
		if !s.journal[run] {
			s.journal[run] = true
			runs[call] = run
		}
	}
	return runs
}

// journaled is what a line of a workflow's journal adds under its call: an
// agent's call when it starts, and its end with the agent's result. A restored
// timeline already shows some.
func (s *Subagents) journaled(raw []byte, call string) []timeline.Event {
	var l struct {
		Type    string          `json:"type"`
		AgentID string          `json:"agentId"`
		Label   string          `json:"label"`
		Phase   string          `json:"phase"`
		Result  json.RawMessage `json:"result"`
	}
	if json.Unmarshal(raw, &l) != nil || l.AgentID == "" {
		return nil
	}
	s.mu.Lock()
	settled, shown := s.settled[l.AgentID]
	s.mu.Unlock()
	switch {
	case l.Type == "started" && !shown:
		detail := proto.ToolDetail{Kind: "task", Description: l.Label, Phase: l.Phase}
		return []timeline.Event{{Kind: "tool_start", ToolUseID: l.AgentID, Name: "Agent", Detail: &detail, ParentToolUseID: call}}
	case l.Type == "result" && !settled:
		out := journalResult(l.Result)
		return []timeline.Event{{Kind: "tool_end", ToolUseID: l.AgentID, OK: true, Output: &out, ParentToolUseID: call}}
	}
	return nil
}

// journalResult is an agent's result as text: its words, or the JSON an agent
// asked for a schema answers with, indented.
func journalResult(raw json.RawMessage) string {
	var text string
	if json.Unmarshal(raw, &text) == nil {
		return text
	}
	var out bytes.Buffer
	if json.Indent(&out, raw, "", "  ") != nil {
		return string(raw)
	}
	return out.String()
}

// workflowRuns is the runs the Workflow calls of the conversation at path
// launched, by call.
func workflowRuns(path string) map[string]string {
	runs := map[string]string{}
	readLines(path, func(raw []byte) {
		if !bytes.Contains(raw, []byte(`"local_workflow"`)) {
			return
		}
		var l line
		if json.Unmarshal(raw, &l) != nil {
			return
		}
		if run := l.workflowRun(); run != "" {
			for _, b := range blocks(l.Message.Content) {
				if b.Type == "tool_result" {
					runs[b.ToolUseID] = run
				}
			}
		}
	})
	return runs
}

// callOf is the tool call that spawned the sub-agent whose transcript is at
// path, or "" until its meta file says. A workflow's agent runs under the
// call its journal starts for it, named by its id.
func (s *Subagents) callOf(path string) string {
	if filepath.Base(filepath.Dir(filepath.Dir(path))) == "workflows" {
		return strings.TrimSuffix(strings.TrimPrefix(filepath.Base(path), "agent-"), ".jsonl")
	}
	s.mu.Lock()
	call, known := s.calls[path]
	s.mu.Unlock()
	if known {
		return call
	}
	if call, known = spawnedBy(path); known {
		s.mu.Lock()
		s.calls[path] = call
		s.mu.Unlock()
	}
	return call
}

// spawnedBy reads the tool call that started the sub-agent whose transcript
// is at path from the meta file beside it; ok is false until that is written.
func spawnedBy(path string) (toolUseID string, ok bool) {
	raw, err := os.ReadFile(strings.TrimSuffix(path, ".jsonl") + ".meta.json")
	if err != nil {
		return "", false
	}
	var meta struct {
		ToolUseID string `json:"toolUseId"`
	}
	if json.Unmarshal(raw, &meta) != nil {
		return "", false
	}
	return meta.ToolUseID, true
}
