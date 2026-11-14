package factory

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// left is a live Ticket as board.json holds it after a restart, with its lead back as a session.
func (w *world) left(station string, back bool) proto.Ticket {
	w.t.Helper()
	x := w.ticket(true)
	w.e.Board.Update(x.ID, func(t *proto.Ticket) {
		t.Station, t.Status, t.AgentID, t.TerminalID = station, proto.TicketWorking, leadID, "term-1"
	})
	if back {
		w.reg.Add(leadID, "/w/x", "claude", w.chat)
	}
	return w.got(x.ID)
}

func TestALeadStillComingBackIsNotTakenForGone(t *testing.T) {
	w := newWorld(t)
	x := w.left(proto.StationPlan, false)
	w.e.Status(leadID, "", "idle")
	w.settle()
	if g := w.got(x.ID); g.Status != proto.TicketWorking || g.AgentID != leadID {
		t.Fatalf("before Recover: %+v", g)
	}
	w.e.Recover()
	w.settle()
	if g := w.got(x.ID); g.Status != proto.TicketFailed || g.Why != "The lead's session closed" || g.AgentID != "" || g.Station != proto.StationPlan {
		t.Fatalf("after Recover: %+v", g)
	}
}

func TestAfterARestartATicketAtThePlanGateStillWaitsOnYou(t *testing.T) {
	w := newWorld(t)
	x := w.left(proto.StationPlan, true)
	w.e.Board.Update(x.ID, func(t *proto.Ticket) { t.Gate, t.Status = proto.GatePlan, proto.TicketNeedsYou })
	if err := w.teams.Store.Put(proto.Team{Lead: leadID, State: "proposed", Tasks: make([]proto.TeamTask, 2)}); err != nil {
		t.Fatal(err)
	}
	lead := w.lead()
	lead.Record(timeline.Event{Kind: "user", Text: "plan"})
	lead.Record(timeline.Event{Kind: "assistant_text", Text: teamPlan})
	lead.Record(timeline.Event{Kind: "result", OK: true})
	w.e.Recover()
	w.settle()
	if g := w.got(x.ID); g.Gate != proto.GatePlan || g.Status != proto.TicketNeedsYou || g.Station != proto.StationPlan {
		t.Fatalf("got %+v", g)
	}
	if s := lead.Summary().Status; s != "needsYou" {
		t.Fatalf("the lead reads %q, so the wait was not put back", s)
	}
	if acts, _ := ReadActivity(w.home, x.ID); len(acts) != 0 {
		t.Fatalf("a restart noted %v", kindsOf(acts))
	}
}

func TestASpecTurnThatEndedWhilePocketdWasDownIsReadAfterTheRestart(t *testing.T) {
	w := newWorld(t)
	x := w.left(proto.StationSpec, true)
	lead := w.lead()
	lead.Record(timeline.Event{Kind: "user", Text: Rules + "\n\n" + SpecRequest(x)})
	lead.Record(timeline.Event{Kind: "assistant_text", Text: goodSpec})
	lead.Record(timeline.Event{Kind: "result", OK: true})
	w.e.Recover()
	w.settle()
	sent := w.chat.sent()
	if g := w.got(x.ID); g.Station != proto.StationPlan || len(sent) != 1 || !strings.Contains(sent[0], "Plan this ticket.") {
		t.Fatalf("got %+v, sent %q", g, sent)
	}
}

func TestATicketAtSpecWhoseLeadHasNoTurnIsAskedAgain(t *testing.T) {
	w := newWorld(t)
	x := w.left(proto.StationSpec, true)
	w.e.Recover()
	w.settle()
	sent := w.chat.sent()
	if g := w.got(x.ID); g.Station != proto.StationSpec || g.Status != proto.TicketWorking || len(sent) != 1 || !strings.Contains(sent[0], "<anywhere_factory_spec_request>") {
		t.Fatalf("got %+v, sent %q", g, sent)
	}
}
