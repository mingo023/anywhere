package team

import (
	"strings"

	"pocketd/internal/proto"
)

// Repair is the prompt that gives a lead its one more try at a plan err refused.
func Repair(err error) string {
	return "Your plan block can't be used: " + err.Error() + ". Send the corrected " + openPlan + " block only."
}

// Planned is t once its lead's planning turn ended, having said text. A usable plan is proposed. The first unusable
// one keeps t planning and gives the repair prompt to send; one after the repair, or a failed turn, fails t.
func Planned(t proto.Team, text string, failed bool) (proto.Team, string) {
	if failed {
		t.State, t.Error = "failed", "The lead's turn failed"
		return t, ""
	}
	tasks, err := Parse(text)
	if err == nil {
		err = Validate(tasks)
	}
	switch {
	case err == nil:
		t.State, t.Tasks, t.Error = "proposed", tasks, ""
	case !t.Repaired:
		t.Repaired = true
		return t, Repair(err)
	default:
		t.State, t.Error = "failed", err.Error()
	}
	return t, ""
}

// Reply is the last turn in items, oldest first: what the agent said in it, whether it has ended, and whether it
// failed. A turn starts at a prompt; with none in items, none has started. Sub-agents' items aren't the agent's.
func Reply(items []proto.Item) (text string, ended, failed bool) {
	start := -1
	for i := len(items) - 1; i >= 0; i-- {
		if items[i].Kind == "user" && items[i].ParentToolUseID == "" {
			start = i
			break
		}
	}
	if start < 0 {
		return "", false, false
	}
	var said []string
	for _, it := range items[start+1:] {
		if it.ParentToolUseID != "" {
			continue
		}
		switch it.Kind {
		case "assistant":
			said = append(said, it.Text)
		case "result":
			ended, failed = true, !it.OK
		}
	}
	return strings.Join(said, "\n"), ended, failed
}
