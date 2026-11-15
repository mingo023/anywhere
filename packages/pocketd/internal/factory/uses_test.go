package factory

import (
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func TestAProfileIsUsedByARoleAPlanAtTheGateAndATaskThatStillRuns(t *testing.T) {
	roles := []Role{
		{"anywhere", proto.FactoryConfig{Lead: "claude-opus", Workers: "go-backend", Review: "codex"}},
		{"site", proto.FactoryConfig{Lead: "go-backend", Workers: "claude-sonnet", Review: "go-backend"}},
	}
	teams := []proto.Team{
		{Lead: "a1", State: "proposed", Tasks: []proto.TeamTask{{ID: "t1", Profile: "go-backend", State: "planned"}, {ID: "t2", Profile: "codex", State: "planned"}}},
		{Lead: "b2", State: "running", Tasks: []proto.TeamTask{{ID: "t1", Profile: "go-backend", State: "done"}, {ID: "t2", Profile: "go-backend", State: "needsYou"}, {ID: "t3", Profile: "go-backend", State: "waiting"}}},
		{Lead: "c3", State: "merged", Tasks: []proto.TeamTask{{ID: "t1", Profile: "go-backend", State: "merged"}}},
	}
	label := func(lead string) string {
		return map[string]string{"a1": "ANY-12", "b2": "ANY-14", "c3": "ANY-9"}[lead]
	}
	got := Users("go-backend", roles, teams, label)
	want := []string{"anywhere Workers role", "site Lead role", "site Reviewer role", "ANY-12 t1", "ANY-14 t2", "ANY-14 t3"}
	if !slices.Equal(got, want) {
		t.Fatalf("got %q", got)
	}
	if got := Users("claude-sonnet", nil, teams, label); len(got) != 0 {
		t.Fatalf("a profile nothing names: %q", got)
	}
}

func TestATicketsKeyIsTheDesktops(t *testing.T) {
	for _, c := range []struct {
		name   string
		number int64
		want   string
	}{{"anywhere", 12, "ANY-12"}, {"my-app", 3, "MYA-3"}, {"a1", 7, "A1-7"}, {"anywhere", 0, ""}, {"--", 4, ""}} {
		if got := Key(c.name, c.number); got != c.want {
			t.Errorf("Key(%q, %d) = %q, want %q", c.name, c.number, got, c.want)
		}
	}
}
