package factory

import (
	"os"
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func kindsOf(events []proto.FactoryEvent) []string {
	var out []string
	for _, ev := range events {
		out = append(out, ev.Actor+":"+ev.Kind+"@"+ev.Station)
	}
	return out
}

func TestTheActivityIsReadBackInTheOrderItWasWritten(t *testing.T) {
	home := t.TempDir()
	if got, err := ReadActivity(home, "t1"); err != nil || got != nil {
		t.Fatalf("no file: %v, %v", got, err)
	}
	first := proto.FactoryEvent{At: 1, Station: proto.StationSpec, Actor: "anywhere", Kind: "moved"}
	second := proto.FactoryEvent{At: 2, Station: proto.StationPlan, Actor: "you", Kind: "approved", Text: "2 workers"}
	for _, ev := range []proto.FactoryEvent{first, second} {
		if err := appendEvent(home, "t1", ev); err != nil {
			t.Fatal(err)
		}
	}
	got, err := ReadActivity(home, "t1")
	if err != nil || !slices.Equal(got, []proto.FactoryEvent{first, second}) {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestALineCutShortByACrashIsSkipped(t *testing.T) {
	home := t.TempDir()
	appendEvent(home, "t1", proto.FactoryEvent{At: 1, Station: proto.StationSpec, Actor: "anywhere", Kind: "moved"})
	f, _ := os.OpenFile(filepath.Join(home, "state", "factory", "t1", "activity.jsonl"), os.O_APPEND|os.O_WRONLY, 0o600)
	f.WriteString(`{"at":2,"stat`)
	f.Close()
	if got, err := ReadActivity(home, "t1"); err != nil || len(got) != 1 {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestAChangeOfATicketIsNotedAsWhatHappened(t *testing.T) {
	gate := at(proto.StationPlan)
	gate.Gate, gate.Status = proto.GatePlan, proto.TicketNeedsYou
	team := &proto.Team{Tasks: make([]proto.TeamTask, 3)}
	failedAt := func(station, why string) proto.Ticket {
		x := at(station)
		x.Status, x.Why = proto.TicketFailed, why
		return x
	}
	for name, c := range map[string]struct {
		before, after proto.Ticket
		want          []string
	}{
		"spec saved":          {at(proto.StationSpec), at(proto.StationPlan), []string{"anywhere:moved@plan"}},
		"the gate goes up":    {at(proto.StationPlan), gate, nil},
		"you approve":         {gate, at(proto.StationImplement), []string{"you:approved@plan", "anywhere:moved@implement"}},
		"you ask for changes": {gate, at(proto.StationPlan), []string{"you:changes@plan"}},
		"the station fails":   {at(proto.StationSpec), failedAt(proto.StationSpec, "The lead's turn failed"), []string{"anywhere:failed@spec"}},
		"it stays failed":     {failedAt(proto.StationSpec, "x"), failedAt(proto.StationSpec, "x"), nil},
		"the plan fails":      {gate, failedAt(proto.StationPlan, "bad plan"), []string{"anywhere:failed@plan"}},
	} {
		got := Notes(c.before, c.after, team, 7)
		if !slices.Equal(kindsOf(got), c.want) {
			t.Errorf("%s: got %v, want %v", name, kindsOf(got), c.want)
		}
	}
	approved := Notes(gate, at(proto.StationImplement), team, 7)[0]
	if approved.Text != "3 workers" || approved.At != 7 {
		t.Fatalf("got %+v", approved)
	}
}
