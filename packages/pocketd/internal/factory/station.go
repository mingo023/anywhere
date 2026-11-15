package factory

import "pocketd/internal/proto"

// Session is what Next knows of a chat session.
type Session struct {
	Present bool
	// Turn is Agent.Turn(): the status without the wait a gate puts on it.
	Turn string
}

// SpecState is how far the Spec station got.
type SpecState string

const (
	SpecNone     SpecState = "none"
	SpecAsked    SpecState = "asked"
	SpecRepaired SpecState = "repaired"
	SpecSaved    SpecState = "saved"
	SpecFailed   SpecState = "failed"
)

// Turn is the lead's answer to the engine's last prompt.
type Turn struct {
	Text          string
	Ended, Failed bool
}

// Facts is everything Next decides from.
type Facts struct {
	Ticket proto.Ticket
	Config proto.FactoryConfig
	Lead   Session
	Team   *proto.Team
	Spec   SpecState
	Reply  Turn
	// Advance is the user's Move to Checks anyway for a Ticket whose Team is in review.
	Advance bool
}

const (
	ActAskSpec    = "askSpec"
	ActRepairSpec = "repairSpec"
	ActSaveSpec   = "saveSpec"
	ActAskPlan    = "askPlan"
	ActAwait      = "await"
)

// Action is one thing the engine does after it wrote the Ticket.
type Action struct {
	Kind string
	Text string
}

// First is the station a Ticket starts at.
func First(c proto.FactoryConfig) string {
	if c.Spec {
		return proto.StationSpec
	}
	return proto.StationPlan
}

// Next is the Ticket as it should read now, and what to do. It never looks outside f.
func Next(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	if t.State != proto.TicketLive || t.Station == "" || t.AgentID == "" {
		return t, nil
	}
	if t.Station != proto.StationSpec && t.Station != proto.StationPlan && t.Station != proto.StationImplement {
		return t, nil
	}
	if !f.Lead.Present {
		// Without an agent the Board offers Start again, which launches a new lead in the same worktree.
		t = failed(t, "The lead's session closed")
		t.AgentID, t.TerminalID = "", ""
		return t, nil
	}
	switch t.Station {
	case proto.StationSpec:
		return spec(f)
	case proto.StationImplement:
		return implement(f), nil
	}
	return plan(f)
}

func working(t proto.Ticket) proto.Ticket {
	t.Status, t.Why, t.Gate = proto.TicketWorking, "", ""
	return t
}

func asks(t proto.Ticket) proto.Ticket {
	t.Status, t.Why, t.Gate = proto.TicketNeedsYou, "", proto.GateAsk
	return t
}

func failed(t proto.Ticket, why string) proto.Ticket {
	t.Status, t.Why, t.Gate = proto.TicketFailed, why, ""
	return t
}

func spec(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	switch {
	case f.Spec == SpecFailed:
		return t, nil
	case f.Spec == SpecNone:
		return working(t), []Action{{Kind: ActAskSpec}}
	case f.Spec == SpecSaved:
		f.Ticket.Station = proto.StationPlan
		return plan(f)
	case f.Reply.Ended && f.Reply.Failed:
		return failed(t, "The lead's turn failed"), nil
	case !f.Reply.Ended && f.Lead.Turn == "needsYou":
		return asks(t), nil
	case !f.Reply.Ended:
		return working(t), nil
	}
	text, err := ParseSpec(f.Reply.Text)
	switch {
	case err == nil:
		f.Ticket.Station = proto.StationPlan
		next, acts := plan(f)
		return next, append([]Action{{Kind: ActSaveSpec, Text: text}}, acts...)
	case f.Spec == SpecAsked:
		return working(t), []Action{{Kind: ActRepairSpec, Text: RepairSpec(err)}}
	}
	return failed(t, "The lead sent no usable spec: "+err.Error()), nil
}

func plan(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	if f.Team == nil {
		// A plan request that could not be sent failed the Ticket. It is not sent again by itself.
		if t.Status == proto.TicketFailed {
			return t, nil
		}
		return working(t), []Action{{Kind: ActAskPlan}}
	}
	switch f.Team.State {
	case "planning":
		if f.Lead.Turn == "needsYou" {
			return asks(t), nil
		}
		return working(t), nil
	case "proposed":
		t.Status, t.Why, t.Gate = proto.TicketNeedsYou, "", proto.GatePlan
		return t, []Action{{Kind: ActAwait}}
	case "failed":
		return failed(t, f.Team.Error), nil
	}
	t.Station = proto.StationImplement
	return working(t), nil
}
