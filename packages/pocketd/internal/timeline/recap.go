package timeline

import (
	"unicode/utf8"

	"pocketd/internal/proto"
)

const recapText = 2000

// Recap is what the last turn of items did; items are in order, sub-agents' included.
func Recap(items []proto.Item) proto.Recap {
	var own []proto.Item
	for _, it := range items {
		if it.ParentToolUseID == "" {
			own = append(own, it)
		}
	}
	turn := own[max(0, lastIndexFunc(own, isPrompt)):]
	var r proto.Recap
	if len(turn) > 0 && turn[0].Kind == "user" {
		r.Asked = clip(turn[0].Text)
	}
	read, edited := map[string]bool{}, map[string]bool{}
	for _, it := range turn {
		if it.Call == nil {
			continue
		}
		switch it.Call.Detail.Kind {
		case "read":
			read[it.Call.Detail.Path] = true
		case "edit", "write":
			edited[it.Call.Detail.Path] = true
		}
	}
	r.Read, r.Edited = len(read), len(edited)
	for i := len(turn) - 1; i >= 0; i-- {
		if turn[i].Kind == "assistant" {
			r.Said = clip(turn[i].Text)
			break
		}
	}
	return r
}

// isPrompt is a prompt to show: a wake is told by the delivery before it instead.
func isPrompt(it proto.Item) bool { return it.Kind == "user" && it.Origin != "wake" }

func lastIndexFunc(items []proto.Item, f func(proto.Item) bool) int {
	for i := len(items) - 1; i >= 0; i-- {
		if f(items[i]) {
			return i
		}
	}
	return -1
}

func clip(s string) string {
	if utf8.RuneCountInString(s) <= recapText {
		return s
	}
	return string([]rune(s)[:recapText])
}

func (t *Timeline) Recap() proto.Recap {
	t.mu.Lock()
	defer t.mu.Unlock()
	return Recap(t.items)
}
