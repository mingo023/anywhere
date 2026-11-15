package factory

import (
	"errors"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/team"
)

const specAsk = `<anywhere_factory_spec_request>
Write a spec for this ticket. Don't edit files or run anything that changes them. Read what you need.
Reply with a short note for the user, then exactly one block:
<anywhere_factory_spec>
## Brief
Two to five sentences: what changes and why.
## Acceptance criteria
- [ ] One checkable statement per line
## Risks
- One line each, or "None"
</anywhere_factory_spec>
</anywhere_factory_spec_request>`

const (
	specOpen   = "<anywhere_factory_spec>"
	specClose  = "</anywhere_factory_spec>"
	specMark   = "<anywhere_factory_spec_request>"
	repairHead = "Your spec block can't be used: "
	criteria   = "## acceptance criteria"
)

// SpecRequest is the lead's first prompt for t.
func SpecRequest(t proto.Ticket) string {
	return specAsk + "\n\nTicket: " + t.Title + "\n\n" + t.Prompt
}

// ParseSpec is the markdown of the last spec block in text. It refuses a block without the
// "## Acceptance criteria" heading and a list item under it.
func ParseSpec(text string) (string, error) {
	end := strings.LastIndex(text, specClose)
	if end < 0 {
		return "", errors.New("there is no <anywhere_factory_spec> block")
	}
	start := strings.LastIndex(text[:end], specOpen)
	if start < 0 {
		return "", errors.New("there is no <anywhere_factory_spec> block")
	}
	body := strings.TrimSpace(text[start+len(specOpen) : end])
	if !hasCriteria(body) {
		return "", errors.New(`it needs a "## Acceptance criteria" heading with at least one list item under it`)
	}
	return body, nil
}

func hasCriteria(body string) bool {
	under := false
	for _, line := range strings.Split(body, "\n") {
		line = strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(line, "## "):
			under = strings.EqualFold(line, criteria)
		case under && (strings.HasPrefix(line, "- ") || strings.HasPrefix(line, "* ")) && strings.TrimSpace(line[2:]) != "":
			return true
		}
	}
	return false
}

// RepairSpec is the one prompt that asks for a corrected block.
func RepairSpec(err error) string {
	return repairHead + err.Error() + ". Send the corrected <anywhere_factory_spec> block only."
}

// PlanRequest is the text given to Teams.Prompt, which puts the team rules before it.
func PlanRequest(t proto.Ticket, spec string) string {
	text := "Plan this ticket.\n\nTicket: " + t.Title + "\n\n" + t.Prompt
	if spec == "" {
		return text
	}
	return text + "\n\nSpec:\n" + spec
}

// Recalled is where a Ticket at Spec stood, read from its lead's items after a restart. working is whether the
// lead's turn still goes. A turn the restart cut off counts as never asked, so the engine asks again.
func Recalled(items []proto.Item, working bool) SpecState {
	for i := len(items) - 1; i >= 0; i-- {
		it := items[i]
		if it.Kind != "user" || it.ParentToolUseID != "" {
			continue
		}
		_, ended, _ := team.Reply(items)
		switch {
		case !ended && !working:
			return SpecNone
		case strings.HasPrefix(it.Text, repairHead):
			return SpecRepaired
		case strings.Contains(it.Text, specMark):
			return SpecAsked
		}
		return SpecNone
	}
	return SpecNone
}
