package automation

import (
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
			seen.Answered = answered(items)
		}
		return seen, true
	}
}

// answered is whether an assistant item follows the last user item.
func answered(items []proto.Item) bool {
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
			return plain(items[i].Text, maxSummary)
		}
	}
	return ""
}
