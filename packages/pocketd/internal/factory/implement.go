package factory

import (
	"cmp"
	"errors"
	"slices"

	"pocketd/internal/board"
	"pocketd/internal/proto"
)

// implement is a Ticket at Implement as its Team reads. Workers at work keep
// it working. An Ask of the lead or of a worker waits on the user. Work that
// the lead called ready, or that the user moved on, goes to Checks.
func implement(f Facts) proto.Ticket {
	t, tm := f.Ticket, f.Team
	if tm == nil || tm.State != "running" && tm.State != "review" {
		return t
	}
	asking := f.Lead.Turn == "needsYou" || slices.ContainsFunc(tm.Tasks, func(x proto.TeamTask) bool { return x.State == "needsYou" })
	switch {
	case asking:
		return asks(t)
	case tm.State == "running":
		if why := stuck(tm); why != "" {
			return failed(t, why)
		}
		return working(t)
	case tm.Ready || f.Advance:
		t.Station = proto.StationChecks
		return working(t)
	case f.Lead.Turn == "working":
		return working(t)
	}
	// Every task handed back and the lead stopped without a verdict.
	t.Status, t.Why, t.Gate = proto.TicketIdle, "", ""
	return t
}

// stuck is why a running Team can't go on: no worker works, and a task failed or was stopped.
func stuck(tm *proto.Team) string {
	why := ""
	for _, x := range tm.Tasks {
		switch x.State {
		case "running", "needsYou":
			return ""
		case "failed":
			reason := "Task " + x.ID + " failed"
			if x.Summary != "" {
				reason += ": " + x.Summary
			}
			why = cmp.Or(why, reason)
		case "stopped":
			why = cmp.Or(why, "Task "+x.ID+" was stopped")
		}
	}
	return why
}

// ErrNotIdle refuses a move to Checks while the Ticket's workers haven't all handed back.
var ErrNotIdle = errors.New("This ticket's workers haven't all handed back")

// Advance records the user's Move to Checks anyway and queues the step that moves the Ticket.
func (e *Engine) Advance(ticket string) error {
	if e == nil {
		return ErrOff
	}
	t, ok := e.ticket(ticket)
	if !ok {
		return board.ErrUnknown
	}
	tm, found, _ := e.Teams.Store.Get(t.AgentID)
	if t.State != proto.TicketLive || t.Station != proto.StationImplement || !found || tm.State != "review" {
		return ErrNotIdle
	}
	e.mu.Lock()
	if e.advance == nil {
		e.advance = map[string]bool{}
	}
	e.advance[ticket] = true
	e.mu.Unlock()
	e.note(ticket, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationImplement, Actor: "you", Kind: "approved", Text: "Moved to Checks without the lead's verdict"})
	e.post(event{ticket: ticket})
	return nil
}

// advancing is whether the user moved t on. A Ticket that left Implement forgets it.
func (e *Engine) advancing(t proto.Ticket) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	if t.Station != proto.StationImplement {
		delete(e.advance, t.ID)
		return false
	}
	return e.advance[t.ID]
}
