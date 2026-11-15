package factory

import (
	"math"
	"testing"
	"time"

	"pocketd/internal/board"
	"pocketd/internal/proto"
)

var metricsNow = time.UnixMilli(1_800_000_000_000)

// act is an event by anywhere, min minutes before now.
func act(min int, kind, station string) proto.FactoryEvent {
	return proto.FactoryEvent{At: metricsNow.Add(-time.Duration(min) * time.Minute).UnixMilli(), Station: station, Actor: "anywhere", Kind: kind}
}

func actedBy(actor string, e proto.FactoryEvent) proto.FactoryEvent {
	e.Actor = actor
	return e
}

func nearly(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func TestNoActivityGivesNoNumbersOverFourteenDays(t *testing.T) {
	if got := Compute(nil, metricsNow); got != (proto.Metrics{Days: 14}) {
		t.Fatalf("got %+v", got)
	}
}

func TestMergedPerDayCountsTicketsMergedInTheLastFourteenDays(t *testing.T) {
	var logs [][]proto.FactoryEvent
	for i := range 7 {
		logs = append(logs, []proto.FactoryEvent{act(600*i+10, "moved", proto.StationSpec), act(600*i, "merged", proto.StationMerged)})
	}
	logs = append(logs, []proto.FactoryEvent{act(15*24*60, "merged", proto.StationMerged)})
	if got := Compute(logs, metricsNow); !nearly(got.MergedPerDay, 0.5) {
		t.Fatalf("got %v", got.MergedPerDay)
	}
}

func TestStartToReadyIsTheMedianFromTheStartToThePRGate(t *testing.T) {
	ready := func(minutes int) []proto.FactoryEvent {
		return []proto.FactoryEvent{act(minutes+100, "moved", proto.StationSpec), act(minutes+50, "moved", proto.StationPlan), act(100, "gate", proto.StationPR), act(90, "gate", proto.StationPR)}
	}
	if got := Compute([][]proto.FactoryEvent{ready(60), ready(120), ready(600)}, metricsNow); got.StartToReady != 7200 {
		t.Fatalf("odd count: got %d", got.StartToReady)
	}
	if got := Compute([][]proto.FactoryEvent{ready(60), ready(120)}, metricsNow); got.StartToReady != 5400 {
		t.Fatalf("even count: got %d", got.StartToReady)
	}
}

func TestAWaitAtAGateRunsFromTheGateToYourDecision(t *testing.T) {
	plan := []proto.FactoryEvent{
		act(100, "gate", proto.StationPlan), actedBy("you", act(90, "changes", proto.StationPlan)),
		act(70, "gate", proto.StationPlan), actedBy("you", act(68, "approved", proto.StationPlan)), act(68, "moved", proto.StationImplement),
		act(40, "gate", proto.StationPR), act(5, "moved", proto.StationMerged), actedBy("you", act(5, "merged", proto.StationMerged)),
	}
	failed := []proto.FactoryEvent{act(50, "gate", proto.StationPlan), act(45, "failed", proto.StationPlan), act(30, "gate", proto.StationPR)}
	got := Compute([][]proto.FactoryEvent{plan, failed}, metricsNow)
	if got.WaitPlan != 360 {
		t.Fatalf("plan: got %d, want the median of 600 and 120", got.WaitPlan)
	}
	if got.WaitPR != 2100 {
		t.Fatalf("pr: got %d, want 35 minutes; a wait still open is no sample", got.WaitPR)
	}
}

func TestAnAskWaitsUntilYouAnswerOldestFirst(t *testing.T) {
	log := []proto.FactoryEvent{
		actedBy("worker:t1", act(10, "asked", proto.StationImplement)),
		actedBy("lead", act(9, "asked", proto.StationImplement)),
		actedBy("you", act(6, "allowed", proto.StationImplement)),
		actedBy("reviewer", act(6, "denied", proto.StationPR)),
		actedBy("you", act(5, "denied", proto.StationImplement)),
		actedBy("worker:t2", act(4, "asked", proto.StationImplement)),
	}
	if got := Compute([][]proto.FactoryEvent{log}, metricsNow); got.WaitAsk != 240 {
		t.Fatalf("got %d, want the median of 240 and 240", got.WaitAsk)
	}
}

func TestAutoFixSuccessIsTheShareOfFirstFixesThatRepairedTheTicket(t *testing.T) {
	fixes := func(n int, end string) []proto.FactoryEvent {
		log := []proto.FactoryEvent{act(100, "check", proto.StationChecks)}
		for i := range n {
			log = append(log, act(90-i, "fix", proto.StationChecks))
		}
		switch end {
		case "repaired":
			log = append(log, act(50, "moved", proto.StationPR))
		case "failed":
			log = append(log, act(50, "failed", proto.StationChecks))
		}
		return log
	}
	logs := [][]proto.FactoryEvent{fixes(1, "repaired"), fixes(2, "repaired"), fixes(2, "failed"), fixes(1, "")}
	got := Compute(logs, metricsNow)
	if !nearly(got.FixRate, 2.0/3) || !nearly(got.FixMean, 5.0/3) {
		t.Fatalf("got rate %v and mean %v", got.FixRate, got.FixMean)
	}
}

func TestAHintEndsAFixThatDidNotRepair(t *testing.T) {
	log := []proto.FactoryEvent{act(30, "fix", proto.StationChecks), act(20, "fix", proto.StationChecks), actedBy("you", act(10, "hint", proto.StationChecks)), act(5, "moved", proto.StationPR)}
	if got := Compute([][]proto.FactoryEvent{log}, metricsNow); got.FixRate != 0 || got.FixMean != 2 {
		t.Fatalf("got rate %v and mean %v", got.FixRate, got.FixMean)
	}
}

func TestMetricsReadTheLogsOfTheProjectsTicketsOnly(t *testing.T) {
	home := t.TempDir()
	e := &Engine{Home: home, Board: board.Open(home), Now: func() time.Time { return metricsNow }}
	mine, err := e.Board.Save(proto.Ticket{Title: "Mine", Prompt: "p", Provider: "claude", Folder: "/app"}, false, metricsNow)
	if err != nil {
		t.Fatal(err)
	}
	other, _ := e.Board.Save(proto.Ticket{Title: "Other", Prompt: "p", Provider: "claude", Folder: "/other"}, false, metricsNow)
	for _, id := range []string{mine.ID, other.ID} {
		if err := appendEvent(home, id, act(60, "merged", proto.StationMerged)); err != nil {
			t.Fatal(err)
		}
	}
	if got := e.Metrics("/app"); !nearly(got.MergedPerDay, 1.0/14) || got.Days != 14 {
		t.Fatalf("got %+v", got)
	}
}
