package factory

import (
	"strconv"
	"strings"
	"unicode"

	"pocketd/internal/proto"
)

// Key is a Ticket's short id as the desktop writes it: the first three letters
// or digits of its project's name in upper case, a hyphen, its number. Empty
// when it has no number or its project's name has neither.
func Key(project string, number int64) string {
	var prefix []rune
	for _, r := range project {
		if len(prefix) < 3 && (unicode.IsLetter(r) || unicode.IsDigit(r)) {
			prefix = append(prefix, unicode.ToUpper(r))
		}
	}
	if number <= 0 || len(prefix) == 0 {
		return ""
	}
	return string(prefix) + "-" + strconv.FormatInt(number, 10)
}

// led is the live Factory Ticket whose lead is agentID.
func (e *Engine) led(agentID string) (proto.Ticket, bool) {
	tickets, _ := e.Board.Snapshot()
	for _, t := range tickets {
		if agentID != "" && t.AgentID == agentID && t.State == proto.TicketLive && t.Station != "" {
			return t, true
		}
	}
	return proto.Ticket{}, false
}

// label names lead's work for a list a person reads: its Ticket's key, or its title when it has no number.
func (e *Engine) label(lead string) string {
	t, ok := e.led(lead)
	if !ok {
		return "A team's task"
	}
	if key := Key(e.Teams.Projects.Load().Name(t.Folder), t.Number); key != "" {
		return key
	}
	return strings.TrimSpace(t.Title)
}
