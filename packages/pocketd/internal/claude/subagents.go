package claude

import (
	"context"
	"encoding/json"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

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
	dir     string
	mu      sync.Mutex
	calls   map[string]string // transcript → the call that spawned it, once its meta file says
	spawned map[string]bool   // calls Spawned took; true once it read their transcripts
	read    map[string]int64  // transcript → how far Spawned read it
}

// NewSubagents follows the sub-agents of the conversation at path.
func NewSubagents(path string) *Subagents {
	return &Subagents{
		dir:     filepath.Join(strings.TrimSuffix(path, ".jsonl"), "subagents"),
		calls:   map[string]string{},
		spawned: map[string]bool{},
		read:    map[string]int64{},
	}
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

// Tail tails each sub-agent transcript of a call Spawned read, from where it
// stopped. fn gets each line's events, from one goroutine per sub-agent. A
// send on kick scans at once, as for a call that just spawned one. It
// returns when ctx ends and every tail has read what was written.
func (s *Subagents) Tail(ctx context.Context, kick <-chan struct{}, fn func([]timeline.Event)) {
	var wg sync.WaitGroup
	defer wg.Wait()
	tailing := map[string]bool{}
	for {
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
	return names
}

// callOf is the tool call that spawned the sub-agent whose transcript is at
// path, or "" until its meta file says.
func (s *Subagents) callOf(path string) string {
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
