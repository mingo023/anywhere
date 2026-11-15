package factory

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// checking is a Ticket at Checks whose lead is idle, changed by edit.
func checking(edit func(*Facts)) Facts {
	f := Facts{
		Ticket: proto.Ticket{ID: "t1", Title: "Fix it", State: proto.TicketLive, Status: proto.TicketWorking, Station: proto.StationChecks, AgentID: "lead"},
		Config: proto.FactoryConfig{Enabled: true, Fix: 2, Checks: []string{"make test"}},
		Lead:   Session{Present: true, Turn: "idle"},
	}
	edit(&f)
	return f
}

func did(acts []Action) string { return strings.Join(kinds(acts), ",") }

func TestATeamInReviewIsMergedBeforeAnythingElse(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "review"} }))
	if got.Status != proto.TicketWorking || got.Why != "Merging the worker branches" || did(acts) != "integrate" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATeamThatIsMergingIsWaitedFor(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "merging"} }))
	if got.Status != proto.TicketWorking || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAConflictFailsTheTicketWithItsFilesAndUsesNoAttempt(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {
		f.Team = &proto.Team{Lead: "lead", State: "conflict", Conflict: []string{"a.rs", "b.rs"}}
		f.Local = LocalChecks{Head: "aaa", Error: "Conflict in a.rs, b.rs"}
	}))
	if got.Status != proto.TicketFailed || got.Station != proto.StationChecks || got.Why != "Conflict in a.rs, b.rs" || got.Fix != 0 || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATicketWithNoTeamSkipsTheMergeAndOpensItsPR(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {}))
	if got.Why != "Opening the draft PR" || did(acts) != "openPR" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAMergedTeamIsNotMergedAgain(t *testing.T) {
	_, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "merged"} }))
	if did(acts) != "openPR" {
		t.Fatalf("got %v", acts)
	}
}

func TestAnOpenPRIsTakenAsTheTicketsAndOnlyPushedTo(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.PR = &proto.PR{Number: 57, State: "OPEN"} }))
	if got.PR != 57 || did(acts) != "openPR" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATicketWhosePRIsPushedRestsUntilTheChecksShip(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {
		f.Ticket.PR = 57
		f.Local = LocalChecks{Head: "aaa", Pushed: true}
	}))
	if got.Status != proto.TicketWorking || got.Why != "Draft PR open" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestWorkThatIsReadyMovesToChecksAndMergesInTheSameStep(t *testing.T) {
	got, acts := Next(Facts{
		Ticket: proto.Ticket{ID: "t1", State: proto.TicketLive, Status: proto.TicketWorking, Station: proto.StationImplement, AgentID: "lead"},
		Lead:   Session{Present: true, Turn: "idle"},
		Team:   &proto.Team{Lead: "lead", State: "review", Ready: true},
	})
	if got.Station != proto.StationChecks || did(acts) != "integrate" {
		t.Fatalf("got %+v %v", got, acts)
	}
}
