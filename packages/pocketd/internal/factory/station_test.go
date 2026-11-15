package factory

import (
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

const goodSpec = "Here is the spec.\n<anywhere_factory_spec>\n## Brief\nSends wait in a queue.\n## Acceptance criteria\n- [ ] A send survives a restart\n## Risks\n- None\n</anywhere_factory_spec>"

var here = Session{Present: true, Turn: "idle"}

func at(station string) proto.Ticket {
	return proto.Ticket{ID: "t1", Title: "Queue sends", Prompt: "Keep sends in a queue", State: proto.TicketLive, Status: proto.TicketWorking, AgentID: "lead", TerminalID: "term", Station: station}
}

func kinds(acts []Action) []string {
	var out []string
	for _, a := range acts {
		out = append(out, a.Kind)
	}
	return out
}

func TestATicketNewAtSpecAsksTheLeadForASpec(t *testing.T) {
	x := at(proto.StationSpec)
	x.Status = proto.TicketStarting
	next, acts := Next(Facts{Ticket: x, Lead: here, Spec: SpecNone})
	if next.Status != proto.TicketWorking || next.Station != proto.StationSpec || !slices.Equal(kinds(acts), []string{ActAskSpec}) {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAValidSpecIsSavedAndTheLeadIsAskedForAPlan(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationSpec), Lead: here, Spec: SpecAsked, Reply: Turn{Text: goodSpec, Ended: true}})
	if next.Station != proto.StationPlan || next.Status != proto.TicketWorking || !slices.Equal(kinds(acts), []string{ActSaveSpec, ActAskPlan}) {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
	if !strings.HasPrefix(acts[0].Text, "## Brief") || strings.Contains(acts[0].Text, "anywhere_factory_spec") {
		t.Fatalf("saved %q", acts[0].Text)
	}
}

func TestASpecWithoutCriteriaGetsOneRepairPrompt(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationSpec), Lead: here, Spec: SpecAsked, Reply: Turn{Text: "I looked around.", Ended: true}})
	if next.Station != proto.StationSpec || next.Status != proto.TicketWorking || !slices.Equal(kinds(acts), []string{ActRepairSpec}) {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
	if !strings.HasPrefix(acts[0].Text, "Your spec block can't be used: ") {
		t.Fatalf("repair %q", acts[0].Text)
	}
}

func TestASecondUnusableSpecFailsTheTicketAtSpecWithTheReason(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationSpec), Lead: here, Spec: SpecRepaired, Reply: Turn{Text: "Still nothing.", Ended: true}})
	if next.Station != proto.StationSpec || next.Status != proto.TicketFailed || !strings.Contains(next.Why, "<anywhere_factory_spec>") || len(acts) != 0 {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestASpecTurnThatFailedFailsTheTicketAtSpec(t *testing.T) {
	next, _ := Next(Facts{Ticket: at(proto.StationSpec), Lead: here, Spec: SpecAsked, Reply: Turn{Ended: true, Failed: true}})
	if next.Status != proto.TicketFailed || next.Why != "The lead's turn failed" || next.Station != proto.StationSpec {
		t.Fatalf("got %+v", next)
	}
}

func TestATicketThatFailedAtSpecStaysFailed(t *testing.T) {
	x := at(proto.StationSpec)
	x.Status, x.Why = proto.TicketFailed, "The lead's turn failed"
	next, acts := Next(Facts{Ticket: x, Lead: here, Spec: SpecFailed})
	if !same(next, x) || len(acts) != 0 {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestALeadThatAsksYouPutsTheAskGateOnTheTicket(t *testing.T) {
	asking := Session{Present: true, Turn: "needsYou"}
	next, _ := Next(Facts{Ticket: at(proto.StationSpec), Lead: asking, Spec: SpecAsked})
	if next.Gate != proto.GateAsk || next.Status != proto.TicketNeedsYou {
		t.Fatalf("at spec: %+v", next)
	}
	next, _ = Next(Facts{Ticket: at(proto.StationPlan), Lead: asking, Team: &proto.Team{State: "planning"}})
	if next.Gate != proto.GateAsk || next.Status != proto.TicketNeedsYou {
		t.Fatalf("at plan: %+v", next)
	}
}

func TestAProjectWithoutTheSpecStationPlansAtOnce(t *testing.T) {
	x := at(proto.StationPlan)
	x.Status = proto.TicketStarting
	next, acts := Next(Facts{Ticket: x, Lead: here})
	if next.Status != proto.TicketWorking || !slices.Equal(kinds(acts), []string{ActAskPlan}) {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAProposedPlanWaitsOnYouAtThePlanGate(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationPlan), Lead: here, Team: &proto.Team{State: "proposed"}})
	if next.Gate != proto.GatePlan || next.Status != proto.TicketNeedsYou || next.Station != proto.StationPlan || !slices.Equal(kinds(acts), []string{ActAwait}) {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAskingForChangesClearsTheGateAndKeepsTheTicketAtPlan(t *testing.T) {
	x := at(proto.StationPlan)
	x.Gate, x.Status = proto.GatePlan, proto.TicketNeedsYou
	next, acts := Next(Facts{Ticket: x, Lead: here, Team: &proto.Team{State: "planning"}})
	if next.Gate != "" || next.Status != proto.TicketWorking || next.Station != proto.StationPlan || len(acts) != 0 {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAPlanThatStartedMovesTheTicketToImplement(t *testing.T) {
	x := at(proto.StationPlan)
	x.Gate, x.Status = proto.GatePlan, proto.TicketNeedsYou
	next, _ := Next(Facts{Ticket: x, Lead: here, Team: &proto.Team{State: "running"}})
	if next.Station != proto.StationImplement || next.Gate != "" || next.Status != proto.TicketWorking {
		t.Fatalf("got %+v", next)
	}
}

func TestATeamThatFailedToPlanFailsTheTicketAtPlan(t *testing.T) {
	next, _ := Next(Facts{Ticket: at(proto.StationPlan), Lead: here, Team: &proto.Team{State: "failed", Error: "a plan needs 2 to 4 tasks, not 1"}})
	if next.Status != proto.TicketFailed || next.Why != "a plan needs 2 to 4 tasks, not 1" || next.Station != proto.StationPlan {
		t.Fatalf("got %+v", next)
	}
}

func TestALeadThatIsGoneFailsTheTicketAndLetsItStartAgain(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationPlan), Team: &proto.Team{State: "proposed"}})
	if next.Status != proto.TicketFailed || next.Why != "The lead's session closed" || next.AgentID != "" || next.TerminalID != "" || next.Station != proto.StationPlan || len(acts) != 0 {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestATicketWithoutAStationOrPastChecksIsLeftAlone(t *testing.T) {
	for _, x := range []proto.Ticket{at(""), at(proto.StationPR), at(proto.StationMerged)} {
		if next, acts := Next(Facts{Ticket: x, Team: &proto.Team{State: "review"}}); !same(next, x) || len(acts) != 0 {
			t.Fatalf("%q: got %+v %v", x.Station, next, kinds(acts))
		}
	}
}

func implementing(state string, tasks ...proto.TeamTask) *proto.Team {
	return &proto.Team{Lead: "lead", State: state, Tasks: tasks}
}

func task(id, state string) proto.TeamTask { return proto.TeamTask{ID: id, State: state} }

func TestATicketWhoseWorkersWorkReadsWorkingAtImplement(t *testing.T) {
	next, acts := Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: implementing("running", task("t1", "running"), task("t2", "waiting"))})
	if next.Station != proto.StationImplement || next.Status != proto.TicketWorking || next.Gate != "" || len(acts) != 0 {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAWorkersAskPutsTheAskGateOnTheTicketAndAnAnswerLiftsIt(t *testing.T) {
	asking := implementing("running", task("t1", "needsYou"), task("t2", "running"))
	next, _ := Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: asking})
	if next.Gate != proto.GateAsk || next.Status != proto.TicketNeedsYou || next.Station != proto.StationImplement {
		t.Fatalf("while it asks: %+v", next)
	}
	again, _ := Next(Facts{Ticket: next, Lead: here, Team: implementing("running", task("t1", "running"), task("t2", "running"))})
	if again.Gate != "" || again.Status != proto.TicketWorking {
		t.Fatalf("after the answer: %+v", again)
	}
}

func TestTheLeadsAskAtImplementIsAnAskToo(t *testing.T) {
	for _, state := range []string{"running", "review"} {
		next, _ := Next(Facts{Ticket: at(proto.StationImplement), Lead: Session{Present: true, Turn: "needsYou"}, Team: implementing(state, task("t1", "done"))})
		if next.Gate != proto.GateAsk || next.Status != proto.TicketNeedsYou {
			t.Fatalf("%s: %+v", state, next)
		}
	}
}

func TestAFailedOrStoppedTaskWithNoWorkerLeftFailsTheTicketWithItsReason(t *testing.T) {
	broken := task("t1", "failed")
	broken.Summary = "merge conflict in a.txt"
	next, _ := Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: implementing("running", broken, task("t2", "waiting"))})
	if next.Status != proto.TicketFailed || next.Why != "Task t1 failed: merge conflict in a.txt" || next.Station != proto.StationImplement {
		t.Fatalf("failed: %+v", next)
	}
	next, _ = Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: implementing("running", task("t1", "done"), task("t2", "stopped"))})
	if next.Status != proto.TicketFailed || next.Why != "Task t2 was stopped" {
		t.Fatalf("stopped: %+v", next)
	}
	next, _ = Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: implementing("running", broken, task("t2", "running"))})
	if next.Status != proto.TicketWorking {
		t.Fatalf("another worker still works: %+v", next)
	}
}

func TestWorkThatIsReadyMovesTheTicketToChecks(t *testing.T) {
	ready := implementing("review", task("t1", "done"), task("t2", "done"))
	ready.Ready = true
	next, acts := Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: ready})
	if next.Station != proto.StationChecks || next.Status != proto.TicketWorking || next.Gate != "" || did(acts) != "integrate" {
		t.Fatalf("got %+v %v", next, kinds(acts))
	}
}

func TestAReviewWithoutAVerdictReadsWorkingWhileTheLeadWorksAndIdleAfter(t *testing.T) {
	review := implementing("review", task("t1", "done"))
	next, _ := Next(Facts{Ticket: at(proto.StationImplement), Lead: Session{Present: true, Turn: "working"}, Team: review})
	if next.Status != proto.TicketWorking || next.Station != proto.StationImplement {
		t.Fatalf("while the lead reviews: %+v", next)
	}
	next, _ = Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: review})
	if next.Status != proto.TicketIdle || next.Station != proto.StationImplement || next.Gate != "" {
		t.Fatalf("after the lead stopped: %+v", next)
	}
	next, _ = Next(Facts{Ticket: next, Lead: here, Team: review, Advance: true})
	if next.Station != proto.StationChecks || next.Status != proto.TicketWorking {
		t.Fatalf("after Move to Checks anyway: %+v", next)
	}
}

func TestAdvanceDoesNothingWhileWorkersStillWork(t *testing.T) {
	next, _ := Next(Facts{Ticket: at(proto.StationImplement), Lead: here, Team: implementing("running", task("t1", "running")), Advance: true})
	if next.Station != proto.StationImplement {
		t.Fatalf("got %+v", next)
	}
}

func TestALeadThatIsGoneAtImplementFailsTheTicket(t *testing.T) {
	next, _ := Next(Facts{Ticket: at(proto.StationImplement), Team: implementing("running", task("t1", "running"))})
	if next.Status != proto.TicketFailed || next.Why != "The lead's session closed" || next.AgentID != "" {
		t.Fatalf("got %+v", next)
	}
}
