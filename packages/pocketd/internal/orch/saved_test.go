package orch

import (
	"maps"
	"testing"
)

func TestWhatARestartBringsBackIsTheLastSetSaved(t *testing.T) {
	s := openTest(t)
	if err := s.SetRestorable(map[string][]byte{"a": []byte(`{"agentId":"a"}`), "b": []byte(`{"agentId":"b","createdAt":7}`)}); err != nil {
		t.Fatal(err)
	}
	want := map[string][]byte{"b": []byte(`{"agentId":"b","createdAt":7}`)}
	if err := s.SetRestorable(want); err != nil {
		t.Fatal(err)
	}
	got, err := s.Restorable()
	if err != nil || !maps.EqualFunc(got, want, func(a, b []byte) bool { return string(a) == string(b) }) {
		t.Fatalf("restorable %q, %v", got, err)
	}
	if b, _ := s.Session("b"); b == nil || b.CreatedAt != 7 {
		t.Fatalf("session b = %+v", b)
	}
}

func TestAStartClosesTheSessionsItBroughtBackNeitherRestoredNorHandedDown(t *testing.T) {
	s := openTest(t)
	execAll(t, s,
		`INSERT INTO sessions (id, status) VALUES ('back', 'idle'), ('gone', 'working'), ('host/x', 'idle')`,
		`INSERT INTO sessions (id, status, closed_at) VALUES ('closed', 'closed', 5)`,
	)
	closed, err := s.CloseAbsent(func(id string) bool { return id == "back" })
	if err != nil || len(closed) != 1 || closed[0] != "gone" {
		t.Fatalf("closed %q, %v", closed, err)
	}
	open, err := s.OpenSessions()
	if err != nil || len(open) != 1 || open[0].ID != "back" {
		t.Fatalf("open %+v, %v", open, err)
	}
	if gone, _ := s.Session("gone"); gone.Status != "closed" || gone.ClosedAt == 0 {
		t.Fatalf("gone = %+v", gone)
	}
	if remote, _ := s.Session("host/x"); remote.Status != "idle" {
		t.Fatalf("a host's session was closed: %+v", remote)
	}
}
