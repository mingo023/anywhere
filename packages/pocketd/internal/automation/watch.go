package automation

import (
	"slices"

	"pocketd/internal/agent"
	"pocketd/internal/proto"
)

// AgentWatcher looks at agents in reg. The summary is the last assistant text
// once the agent is not working.
func AgentWatcher(reg *agent.Registry) Watcher {
	return func(id string) (Seen, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return Seen{}, false
		}
		sum := a.Summary()
		seen := Seen{Status: sum.Status, Failed: sum.Failed}
		if sum.Status == "done" || sum.Status == "idle" {
			items, _ := a.Timeline.Page(0, 500)
			seen.Summary = lastAssistantText(items)
			seen.Answered = Answered(items)
		}
		return seen, true
	}
}

// TurnWatcher is AgentWatcher for a run that put its prompt into a session that already had turns.
// after is the session's timeline seq before the prompt went in: nothing at or
// before it says anything about the run. Seq keeps counting through a clear.
func TurnWatcher(reg *agent.Registry, after int64) Watcher {
	return func(id string) (Seen, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return Seen{}, false
		}
		sum := a.Summary()
		items, _ := a.Timeline.Page(after, 500)
		if i := slices.IndexFunc(items, func(it proto.Item) bool { return it.Kind == "user" }); i >= 0 {
			items = items[i:]
		} else {
			items = nil
		}
		seen := Seen{Status: sum.Status, Failed: sum.Failed}
		seen.Before = !slices.ContainsFunc(items, func(it proto.Item) bool { return it.Kind == "assistant" || it.Kind == "result" })
		if sum.Status == "done" || sum.Status == "idle" {
			seen.Summary = lastAssistantText(items)
			seen.Answered = Answered(items)
		}
		return seen, true
	}
}

// Answered is whether an assistant item follows the last user item.
func Answered(items []proto.Item) bool {
	for i := len(items) - 1; i >= 0; i-- {
		switch items[i].Kind {
		case "assistant":
			return true
		case "user":
			return false
		}
	}
	return false
}

func lastAssistantText(items []proto.Item) string {
	for i := len(items) - 1; i >= 0; i-- {
		if items[i].Kind == "assistant" {
			return Plain(items[i].Text, maxSummary)
		}
	}
	return ""
}
