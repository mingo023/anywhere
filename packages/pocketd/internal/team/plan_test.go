package team

import (
	"errors"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func planning() proto.Team {
	return proto.Team{Lead: "a1", State: "planning", Tasks: []proto.TeamTask{}}
}

func TestAUsablePlanIsProposed(t *testing.T) {
	got, repair := Planned(planning(), "Two tasks, the export after the function.\n"+twoTasks, false)
	if repair != "" || got.State != "proposed" || got.Error != "" || got.Repaired || !slices.Equal(ids(got.Tasks), []string{"t1", "t2"}) {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestTheFirstUnusablePlanAsksTheLeadForARepair(t *testing.T) {
	got, repair := Planned(planning(), "I'd split it in two.", false)
	want := "Your plan block can't be used: no <anywhere_team_plan> block. Send the corrected <anywhere_team_plan> block only."
	if repair != want || got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestTheRepairCarriesWhatValidateFound(t *testing.T) {
	_, repair := Planned(planning(), "echo: "+Wrap("add truncate"), false)
	if repair != "Your plan block can't be used: a plan needs 2 to 4 tasks, not 1. Send the corrected <anywhere_team_plan> block only." {
		t.Fatalf("repair %q", repair)
	}
}

func TestAnUnusablePlanAfterTheRepairFailsTheTeam(t *testing.T) {
	team := planning()
	team.Repaired = true
	got, repair := Planned(team, "Still thinking.", false)
	if repair != "" || got.State != "failed" || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestARepairedPlanIsProposed(t *testing.T) {
	team := planning()
	team.Repaired = true
	got, repair := Planned(team, twoTasks, false)
	if repair != "" || got.State != "proposed" || !got.Repaired || len(got.Tasks) != 2 {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestAFailedTurnFailsTheTeamWhateverItSaid(t *testing.T) {
	got, repair := Planned(planning(), twoTasks, true)
	if repair != "" || got.State != "failed" || got.Error != "The lead's turn failed" || len(got.Tasks) != 0 {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestRepairQuotesTheError(t *testing.T) {
	if got := Repair(errors.New("task t2: after names t9, which isn't a task")); got != "Your plan block can't be used: task t2: after names t9, which isn't a task. Send the corrected <anywhere_team_plan> block only." {
		t.Fatalf("got %q", got)
	}
}

func TestReplyIsWhatTheLastTurnSaid(t *testing.T) {
	text, ended, failed := Reply([]proto.Item{
		{Seq: 1, Kind: "user", Text: "earlier"},
		{Seq: 2, Kind: "assistant", Text: "earlier answer"},
		{Seq: 3, Kind: "result", OK: true},
		{Seq: 4, Kind: "user", Text: "plan it"},
		{Seq: 5, Kind: "assistant", Text: "Reading the code."},
		{Seq: 6, Kind: "tool"},
		{Seq: 7, Kind: "assistant", Text: "a sub-agent's words", ParentToolUseID: "toolu_1"},
		{Seq: 8, Kind: "assistant", Text: "Two tasks."},
		{Seq: 9, Kind: "result", OK: true},
	})
	if text != "Reading the code.\nTwo tasks." || !ended || failed {
		t.Fatalf("%q %v %v", text, ended, failed)
	}
}

func TestATurnWithoutItsResultHasNotEnded(t *testing.T) {
	text, ended, _ := Reply([]proto.Item{{Seq: 1, Kind: "user", Text: "plan it"}, {Seq: 2, Kind: "assistant", Text: "Two tasks."}})
	if text != "Two tasks." || ended {
		t.Fatalf("%q %v", text, ended)
	}
}

func TestAFailedResultEndsAFailedTurn(t *testing.T) {
	_, ended, failed := Reply([]proto.Item{{Seq: 1, Kind: "user", Text: "plan it"}, {Seq: 2, Kind: "result", OK: false, Error: "interrupted"}})
	if !ended || !failed {
		t.Fatalf("%v %v", ended, failed)
	}
}

func TestNoPromptIsNoTurn(t *testing.T) {
	text, ended, failed := Reply([]proto.Item{{Seq: 1, Kind: "assistant", Text: "hi"}, {Seq: 2, Kind: "result", OK: true}})
	if text != "" || ended || failed {
		t.Fatalf("%q %v %v", text, ended, failed)
	}
}
