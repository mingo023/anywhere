package main

import (
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/statedb/dbtest"
)

func legacyStore(t *testing.T) *devices.Store {
	t.Helper()
	s, err := devices.Open(dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db")), "old-token")
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func TestServeStartsTheGraceAndKeepsTheLegacyDeviceUntilItEnds(t *testing.T) {
	s := legacyStore(t)
	if err := endGrace(s, func(id, _ string) { t.Errorf("closed %s", id) }); err != nil {
		t.Fatal(err)
	}
	if list := s.List(); list[0].GraceEndsAt < time.Now().Add(devices.Grace-time.Minute).UnixMilli() {
		t.Fatalf("%+v", list[0])
	}
	if _, ok := s.Lookup("old-token"); !ok {
		t.Fatal("the old token stopped working in its grace")
	}
}

func TestServeAfterTheGraceForgetsTheLegacyDeviceAndClosesItsSockets(t *testing.T) {
	s := legacyStore(t)
	s.StartGrace(time.Now().Add(-devices.Grace))
	var closed []string
	if err := endGrace(s, func(id, reason string) { closed = append(closed, id+" "+reason) }); err != nil {
		t.Fatal(err)
	}
	if len(closed) != 1 || closed[0] != "legacy revoked" || s.Has(devices.LegacyID) {
		t.Fatalf("closed %v, has legacy %v", closed, s.Has(devices.LegacyID))
	}
}

func TestServeClosesTheLegacySocketsEvenWhenForgettingTheDeviceFailsToSave(t *testing.T) {
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	s, err := devices.Open(db, "old-token")
	if err != nil {
		t.Fatal(err)
	}
	s.StartGrace(time.Now().Add(-devices.Grace))
	db.Close()
	var closed []string
	if err := endGrace(s, func(id, reason string) { closed = append(closed, id+" "+reason) }); err != nil {
		t.Fatal(err)
	}
	if len(closed) != 1 || closed[0] != "legacy revoked" {
		t.Fatalf("closed %v", closed)
	}
	if _, ok := s.Lookup("old-token"); ok {
		t.Fatal("the old token works after its grace")
	}
}
