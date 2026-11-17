package automation

import (
	"slices"
	"strings"

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

// Entry looks up a queued run's message: its text now and its queue state, "" once it is gone.
type Entry func(agentID, entryID string) (text, state string)

// TurnWatcher is AgentWatcher for a run that put its prompt into a session that already had turns.
// after is the session's timeline seq before the prompt went in: nothing at or
// before it says anything about the run. Seq keeps counting through a clear.
// A prompt that queued behind other messages has its entry: its turn is the first with the entry's text now.
func TurnWatcher(reg *agent.Registry, after int64, prompt, entry string, queued Entry) Watcher {
	since := int64(-1)
	// shown is whether the turn was there at the first look: only a turn shown before a restart can have been cut by it.
	shown := false
	return func(id string) (Seen, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return Seen{}, false
		}
		first := since < 0
		if first {
			// A crash can lose the timeline's last items, and seq goes on from the last one kept.
			_, top := a.Timeline.State()
			since = min(after, top)
		}
		lost := since < after
		state := ""
		if entry != "" {
			var text string
			if text, state = queued(id, entry); state == "cancelled" {
				return Seen{Cancel: "Its queued message was removed"}, true
			} else if state != "" {
				prompt = text
			}
		}
		own := func(it proto.Item) bool {
			return it.Kind == "user" && (entry == "" && !lost || slices.Equal(strings.Fields(it.Text), strings.Fields(prompt)))
		}
		sum := a.Summary()
		items, _ := a.Timeline.Page(since, 500)
		i := slices.IndexFunc(items, own)
		if first {
			shown = i >= 0
		}
		// An entry already sent, or gone, when the watch begins on an idle session will start no turn, so a turn
		// with its text not shown by then was lost.
		sent := entry != "" && first && (state == "sent" || state == "") && sum.Status != "working" && sum.Status != "needsYou"
		if i < 0 && (sent || lost && entry == "" && slices.ContainsFunc(items, func(it proto.Item) bool { return it.Kind == "user" })) {
			return Seen{Fail: "Its turn was lost in a crash"}, true
		}
		if i >= 0 {
			items = items[i:]
		} else {
			items = nil
		}
		seen := Seen{Status: sum.Status, Failed: sum.Failed}
		seen.Before = !slices.ContainsFunc(items, func(it proto.Item) bool { return it.Kind == "assistant" || it.Kind == "result" })
		seen.Cut = shown && seen.Before && sum.Restore == proto.RestoreInterrupted
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
