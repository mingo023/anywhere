package acp

import (
	"encoding/json"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// brain is where agy keeps each session's state, its sub-agents' descriptors among it.
var brain = func() string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".gemini", "antigravity-cli", "brain")
}()

// launchOf is the held launch whose sub-agent made the call id, which agy
// names "<sub-agent>:<call>"; "" for the session's own. It matches by the
// descriptor's role first: sub-agents launched together race to their first
// calls, so which comes first tells nothing.
func (s *Session) launchOf(id string) string {
	child, _, ok := strings.Cut(id, ":")
	if !ok {
		return ""
	}
	if launch, seen := s.children[child]; seen {
		return launch
	}
	claimed := slices.Collect(maps.Values(s.children))
	open := slices.DeleteFunc(slices.Clone(s.launches), func(l string) bool { return s.tools[l] == nil || slices.Contains(claimed, l) })
	launch := ""
	if len(open) > 0 {
		launch = open[0]
		if role := s.role(child); role != "" {
			if i := slices.IndexFunc(open, func(l string) bool { return s.tools[l].role() == role }); i >= 0 {
				launch = open[i]
			}
		}
	}
	s.children[child] = launch
	return launch
}

// role is the role of the sub-agent child in the descriptor agy writes as it
// launches one; "" if it has none.
func (s *Session) role(child string) string {
	raw, _ := os.ReadFile(filepath.Join(brain, s.id, ".system_generated", "subagents", child+".json"))
	var d struct {
		Descriptor struct {
			Role string `json:"role"`
		} `json:"subagentDescriptor"`
	}
	json.Unmarshal(raw, &d)
	return d.Descriptor.Role
}

// role is the role an invoke_subagent launched its sub-agent as.
func (t *tool) role() string {
	var in struct{ Role string }
	json.Unmarshal(t.RawInput, &in)
	return in.Role
}

// endLaunches ends the sub-agents launched this turn with it: agy answers the
// prompt only once they are done, and tells of their end nothing else.
func (s *Session) endLaunches(e timeline.Event, adopted bool) {
	if !s.f.childCalls {
		return
	}
	out := "Interrupted"
	for id, t := range s.tools {
		if slices.Contains(s.launches, t.parent) {
			delete(s.tools, id)
			if t.started {
				s.record(timeline.Event{Kind: "tool_end", ToolUseID: id, Output: &out})
			}
		}
	}
	for _, id := range s.launches {
		if s.tools[id] != nil {
			delete(s.tools, id)
			s.record(timeline.Event{Kind: "tool_end", ToolUseID: id, OK: e.OK})
		}
	}
	s.launches = nil
	if adopted {
		end := timeline.Event{OK: e.OK}
		if e.Error == "interrupted" {
			end.Output = &out
		}
		s.endShownLaunches(end)
	}
}

// endShownLaunches ends as end the launches a pocketd before this one held
// open, which only its timeline still knows of, and their calls interrupted.
func (s *Session) endShownLaunches(end timeline.Event) {
	if !s.f.childCalls || s.Showing == nil {
		return
	}
	out := "Interrupted"
	running := func(it proto.Item) bool { return it.Call != nil && it.Call.Status == "running" }
	shown := s.Showing()
	var launches []string
	for _, it := range shown {
		if running(it) && it.Call.Detail.Kind == "task" && it.ParentToolUseID == "" {
			launches = append(launches, it.Call.ToolUseID)
		}
	}
	for _, it := range shown {
		if running(it) && slices.Contains(launches, it.ParentToolUseID) {
			s.record(timeline.Event{Kind: "tool_end", ToolUseID: it.Call.ToolUseID, Output: &out})
		}
	}
	end.Kind = "tool_end"
	for _, id := range launches {
		end.ToolUseID = id
		s.record(end)
	}
}
