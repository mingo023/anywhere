package factory

import (
	"errors"
	"fmt"
	"strings"

	"pocketd/internal/board"
	"pocketd/internal/proto"
)

// noteLines is how many lines of a failed command's output the fix note carries.
const noteLines = 200

// FixNote is the note mailed to the lead after failed checks. hint is the user's text, "" for an automatic note.
// checks are the commands of the newest record. pr is the PR when its GitHub checks or its base failed, else nil.
func FixNote(attempt, limit int, hint string, checks []proto.FactoryCheck, pr *proto.PR) string {
	var b strings.Builder
	fmt.Fprintf(&b, "<anywhere_factory_fix attempt=\"%d\" of=\"%d\"", attempt, limit)
	if hint = strings.TrimSpace(hint); hint != "" {
		b.WriteString(" by=\"you\">\n" + hint + "\n")
	} else {
		b.WriteString(">\n")
	}
	b.WriteString("The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.\n")
	for _, c := range checks {
		if c.Exit != 0 {
			fmt.Fprintf(&b, "$ %s   (exit %d)\n%s", c.Command, c.Exit, lastLines(c.Output, noteLines))
		}
	}
	if pr != nil {
		if pr.Mergeable == "CONFLICTING" {
			fmt.Fprintf(&b, "The pull request conflicts with %s. Merge origin/%s into this branch and resolve the conflicts.\n", pr.Base, pr.Base)
		}
		for _, run := range pr.Runs {
			if run.Outcome == "failed" {
				fmt.Fprintf(&b, "GitHub check failed: %s %s\n", run.Name, run.URL)
			}
		}
	}
	b.WriteString("</anywhere_factory_fix>")
	return b.String()
}

// lastLines is the last n lines of text, ending with a newline when text is not empty.
func lastLines(text string, n int) string {
	text = strings.TrimRight(text, "\n")
	if text == "" {
		return ""
	}
	lines := strings.Split(text, "\n")
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return strings.Join(lines, "\n") + "\n"
}

// ErrNoHint refuses a hint for a Ticket that did not fail at Checks, or a blank one.
var ErrNoHint = errors.New("A hint needs a ticket that failed at Checks, and some text")

// Hint sends text to the lead of a Ticket that failed at Checks, as one more attempt beyond the cap.
func (e *Engine) Hint(ticket, text string) error {
	if e == nil {
		return ErrOff
	}
	text = strings.TrimSpace(text)
	t, ok := e.ticket(ticket)
	if !ok {
		return board.ErrUnknown
	}
	if !hintable(t) || text == "" {
		return ErrNoHint
	}
	e.later(func() {
		// The Ticket may have moved on since the check above.
		if t, ok := e.ticket(ticket); ok && hintable(t) {
			rec, fresh := e.current(t)
			rec.Hint = text
			e.save(ticket, rec, fresh)
			e.post(event{ticket: ticket})
		}
	})
	return nil
}

func hintable(t proto.Ticket) bool {
	return t.State == proto.TicketLive && t.Station == proto.StationChecks && t.Status == proto.TicketFailed
}
