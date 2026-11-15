package factory

import "pocketd/internal/proto"

// Recover picks up every live Factory ticket where pocketd's restart left it. Call it once the restored sessions
// have come back or failed (daemon.WaitRestored) and Teams.Recover has run, so a lead still resuming isn't taken
// for gone and its Team reads as it will stay.
func (e *Engine) Recover() {
	if e == nil {
		return
	}
	tickets, _ := e.Board.Snapshot()
	var live []string
	for _, t := range tickets {
		if t.State != proto.TicketLive || t.Station == "" || t.AgentID == "" {
			continue
		}
		e.recall(t)
		// A given name doesn't survive pocketd's restart.
		e.rename(t, t.AgentID)
		live = append(live, t.ID)
	}
	e.recovered.Store(true)
	for _, id := range live {
		e.post(event{ticket: id})
	}
}

// recall reads back whether the lead of a Ticket at Spec was asked for its spec, from the lead's last turn.
func (e *Engine) recall(t proto.Ticket) {
	lead, ok := e.lead(t.AgentID)
	if !ok || t.Station != proto.StationSpec {
		return
	}
	if state, _ := e.spec(t); state != SpecNone {
		return
	}
	items, _ := lead.Timeline.Page(0, turnItems)
	turn := lead.Turn()
	state := Recalled(items, turn == "working" || turn == "needsYou")
	if state == SpecNone {
		return
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.specs == nil {
		e.specs = map[string]asked{}
	}
	e.specs[t.ID] = asked{state: state}
}
