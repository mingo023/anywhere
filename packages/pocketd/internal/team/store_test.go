package team

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func sample(lead, state string) proto.Team {
	return proto.Team{Lead: lead, State: state, UpdatedAt: 1790000000000, Tasks: []proto.TeamTask{
		{ID: "t1", Title: "A", Agent: "claude", Files: []string{"a.go"}, After: []string{}, Allow: []string{"go test ./..."}, Instructions: "Do A", State: "planned"},
	}}
}

func open(t *testing.T, home string) *Store {
	t.Helper()
	s, err := OpenStore(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

func TestATeamComesBackFromStateTeamDBAfterReopening(t *testing.T) {
	home := t.TempDir()
	s, err := OpenStore(home)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Put(sample("a1", "proposed")); err != nil {
		t.Fatal(err)
	}
	s.Close()
	if _, err := os.Stat(filepath.Join(home, "state", "team.db")); err != nil {
		t.Fatal(err)
	}

	got, ok, err := open(t, home).Get("a1")

	if err != nil || !ok || !reflect.DeepEqual(got, sample("a1", "proposed")) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
}

func TestPutReplacesALeadsTeam(t *testing.T) {
	s := open(t, t.TempDir())
	s.Put(sample("a1", "planning"))
	failed := sample("a1", "failed")
	failed.Error, failed.Repaired = "no <anywhere_team_plan> block", true

	if err := s.Put(failed); err != nil {
		t.Fatal(err)
	}

	got, _, _ := s.Get("a1")
	list, err := s.List()
	if !reflect.DeepEqual(got, failed) || err != nil || len(list) != 1 {
		t.Fatalf("got %+v, list %+v %v", got, list, err)
	}
}

func TestALeadWithoutATeamHasNone(t *testing.T) {
	got, ok, err := open(t, t.TempDir()).Get("zz")
	if ok || err != nil || !reflect.DeepEqual(got, proto.Team{}) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
}

func TestListHasEveryLeadsTeamByLead(t *testing.T) {
	s := open(t, t.TempDir())
	s.Put(sample("b2", "proposed"))
	s.Put(sample("a1", "planning"))

	list, err := s.List()

	if err != nil || len(list) != 2 || list[0].Lead != "a1" || list[1].Lead != "b2" {
		t.Fatalf("%+v %v", list, err)
	}
}
