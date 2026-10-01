package main

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/devices"
)

func legacyStore(t *testing.T) *devices.Store {
	t.Helper()
	s, err := devices.Open(filepath.Join(t.TempDir(), "devices.json"), "old-token")
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
	dir := t.TempDir()
	s, err := devices.Open(filepath.Join(dir, "devices.json"), "old-token")
	if err != nil {
		t.Fatal(err)
	}
	s.StartGrace(time.Now().Add(-devices.Grace))
	os.Chmod(dir, 0o500)
	t.Cleanup(func() { os.Chmod(dir, 0o700) })
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
