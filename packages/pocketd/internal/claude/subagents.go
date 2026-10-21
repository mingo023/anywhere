package claude

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
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
	events := l.events()
	for i := range events {
		events[i].ParentToolUseID = parent
	}
	// Only the sub-agent's answers name the model it runs on; its parent's call shows it.
	if l.Type == "assistant" && l.Message.Model != "" && l.Message.Model != "<synthetic>" {
		events = append([]timeline.Event{{Kind: "tool_model", ToolUseID: parent, Name: l.Message.Model}}, events...)
	}
	return events
}

// TailSubagents tails each sub-agent transcript Claude writes beside the
// conversation at path once spawned has seen the tool call that started it,
// so its items never come before their parent's. fn gets each line's events,
// from one goroutine per sub-agent. A send on kick scans at once, as for a
// call that just spawned one. It returns when ctx ends and every tail has
// read what was written.
func TailSubagents(ctx context.Context, path string, kick <-chan struct{}, spawned func(toolUseID string) bool, fn func([]timeline.Event)) {
	dir := filepath.Join(strings.TrimSuffix(path, ".jsonl"), "subagents")
	var wg sync.WaitGroup
	defer wg.Wait()
	parents := map[string]string{}
	tailing := map[string]bool{}
	for {
		names, _ := filepath.Glob(filepath.Join(dir, "agent-*.jsonl"))
		for _, name := range names {
			parent, known := parents[name]
			if !known {
				if parent, known = spawnedBy(name); known {
					parents[name] = parent
				}
			}
			// Forks and skills run as sub-agents too, but without a tool call to nest under.
			if tailing[name] || parent == "" || !spawned(parent) {
				continue
			}
			tailing[name] = true
			wg.Add(1)
			go func() {
				defer wg.Done()
				Tail(ctx, name, func(raw []byte) { fn(MapSubagent(raw, parent)) })
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
