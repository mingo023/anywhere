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

// pushed is a Ticket at Checks whose draft PR is open and pushed, changed by edit.
func pushed(edit func(*Facts)) Facts {
	return checking(func(f *Facts) {
		f.Ticket.PR = 57
		f.Local = LocalChecks{Head: "aaa", Pushed: true}
		edit(f)
	})
}

// green is the PR of commit aaa with one passed check.
func green() *proto.PR {
	return &proto.PR{Number: 57, State: "OPEN", Draft: true, Base: "main", Head: "aaa", Mergeable: "MERGEABLE",
		Checks: proto.Checks{Passed: 1}, Runs: []proto.CheckRun{{Name: "ci", Outcome: "passed"}}}
}

func TestACommitWhoseCommandsHaveNotRunRunsThem(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {}))
	if got.Status != proto.TicketWorking || got.Why != "Checks · 0/1" || did(acts) != "runChecks" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestRunningCommandsShowHowManyAreDone(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) { f.Local.State, f.Local.Done, f.Local.Total = LocalRunning, 2, 3 }))
	if got.Why != "Checks · 2/3" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailedCommandFailsTheTicketAtChecks(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.Local.Failed = LocalFailed, "cargo test"
	}))
	if got.Status != proto.TicketFailed || got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "cargo test") || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestPassedCommandsWaitForGitHubToKnowTheCommit(t *testing.T) {
	for name, pr := range map[string]*proto.PR{"no PR yet": nil, "an older commit": {Number: 57, State: "OPEN", Head: "old"}} {
		got, acts := Next(pushed(func(f *Facts) { f.Local.State, f.PR = LocalPassed, pr }))
		if got.Status != proto.TicketWorking || got.Station != proto.StationChecks || got.Why != "Asking GitHub" || len(acts) != 0 {
			t.Fatalf("%s: got %+v %v", name, got, acts)
		}
	}
}

func TestPendingGitHubChecksAreWaitedFor(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks = proto.Checks{Passed: 1, Pending: 2}
	}))
	if got.Status != proto.TicketWorking || got.Station != proto.StationChecks || got.Why != "GitHub checks · 1/3" {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedGitHubCheckFailsTheTicketWithItsName(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks = proto.Checks{Passed: 1, Failed: 1}
		f.PR.Runs = append(f.PR.Runs, proto.CheckRun{Name: "e2e", Outcome: "failed", URL: "https://x/1"})
	}))
	if got.Status != proto.TicketFailed || !strings.HasPrefix(got.Why, "e2e") {
		t.Fatalf("got %+v", got)
	}
}

func TestAPRThatConflictsWithItsBaseFailsTheChecks(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Mergeable = "CONFLICTING"
	}))
	if got.Status != proto.TicketFailed || !strings.HasPrefix(got.Why, "Conflicts with main") {
		t.Fatalf("got %+v", got)
	}
}

func TestAClosedPRFailsTheTicket(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.State = "CLOSED"
	}))
	if got.Status != proto.TicketFailed || got.Why != "The pull request was closed" {
		t.Fatalf("got %+v", got)
	}
}

func TestPassedCommandsAndPassedGitHubChecksMoveTheTicketToPRReview(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Ticket.Hint, f.Ticket.Fix = true, 1
		f.Local.State, f.PR = LocalPassed, green()
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking || got.Why != "" || got.Hint || got.Fix != 1 || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestSkippedGitHubChecksCountAsPassed(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, []proto.CheckRun{{Name: "label", Outcome: "skipped"}}
	}))
	if got.Station != proto.StationPR {
		t.Fatalf("got %+v", got)
	}
}

func TestWithNoGitHubCheckTheTicketWaitsUntilGitHubHadTimeToReportOne(t *testing.T) {
	none := func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, nil
	}
	got, _ := Next(pushed(none))
	if got.Station != proto.StationChecks || got.Status != proto.TicketWorking || got.Why != "Looking for GitHub checks" {
		t.Fatalf("got %+v", got)
	}
	got, _ = Next(pushed(func(f *Facts) { none(f); f.Local.Quiet = true }))
	if got.Station != proto.StationPR {
		t.Fatalf("after the wait: got %+v", got)
	}
}

func TestATicketWithNoCommandsAndNoGitHubChecksStillPasses(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Checks = nil
		f.Local.State, f.Local.Quiet, f.PR = LocalPassed, true, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, nil
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedTicketWhoseGitHubChecksPassLaterPasses(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Ticket.Status, f.Ticket.Why, f.Ticket.Fix = proto.TicketFailed, "e2e · Auto-fix 2 of 2 used", 2
		f.Local.State, f.PR = LocalPassed, green()
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking || got.Fix != 2 {
		t.Fatalf("got %+v", got)
	}
}
