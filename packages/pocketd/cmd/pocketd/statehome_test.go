package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/devices"
	"pocketd/internal/orch"
	"pocketd/internal/registry"
	"pocketd/internal/state"
	"pocketd/internal/statedb"
)

func TestDeletingTheOrchestratorDatabaseKeepsProjectsAndDevices(t *testing.T) {
	home, project := t.TempDir(), t.TempDir()
	db, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	if err := reg.AddProject(project, nil); err != nil {
		t.Fatal(err)
	}
	devs, err := devices.Open(db, "")
	if err != nil {
		t.Fatal(err)
	}
	phone, _, err := devs.Add("iPhone", "ios", devices.PhoneScopes)
	if err != nil {
		t.Fatal(err)
	}
	orchestrator, err := orch.OpenPath(filepath.Join(home, "state", "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	orchestrator.Close()
	db.Close()
	for _, f := range []string{"", "-wal", "-shm"} {
		os.Remove(filepath.Join(home, "state", "orchestrator.db") + f)
	}

	db, err = statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	reg, _ = registry.Open(db)
	devs, _ = devices.Open(db, "")
	if got := reg.Load().Projects; len(got) != 1 || got[0] != project || !devs.Has(phone.ID) {
		t.Fatalf("projects %v, has the phone %v", got, devs.Has(phone.ID))
	}
}

func TestALegacyTokenInConfigJSONIsAdoptedOnceAndTheFileStaysAsItWas(t *testing.T) {
	home := t.TempDir()
	original := []byte(`{"token": "old-token", "port": 4517, "profiles": {"work": {}}}`)
	path := filepath.Join(home, "config.json")
	if err := os.WriteFile(path, original, 0o600); err != nil {
		t.Fatal(err)
	}
	start := func(adopt func(devs *devices.Store) func(string) error) *devices.Store {
		db, err := statedb.Open(home)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { db.Close() })
		devs, err := devices.Open(db, "")
		if err != nil {
			t.Fatal(err)
		}
		if _, err := config.Load(db, adopt(devs)); err != nil {
			t.Fatal(err)
		}
		return devs
	}
	devs := start(func(d *devices.Store) func(string) error { return d.EnsureLegacy })
	if d, ok := devs.Lookup("old-token"); !ok || d.ID != devices.LegacyID {
		t.Fatalf("the old token isn't a device: %+v %v", d, ok)
	}

	again := start(func(*devices.Store) func(string) error {
		return func(string) error { t.Error("the token was adopted again"); return nil }
	})
	if _, ok := again.Lookup("old-token"); !ok {
		t.Fatal("the legacy device didn't survive the restart")
	}
	if now, _ := os.ReadFile(path); !bytes.Equal(now, original) {
		t.Fatalf("config.json became %s", now)
	}
}

func TestTheHostIDIsTheSameAfterARestartAndAfterTheImport(t *testing.T) {
	home := t.TempDir()
	os.MkdirAll(filepath.Join(home, "state"), 0o700)
	os.WriteFile(filepath.Join(home, "state", "host.json"), []byte(`{"id":"host-from-before"}`), 0o600)
	for range 2 {
		db, err := statedb.Open(home)
		if err != nil {
			t.Fatal(err)
		}
		if id, err := state.HostID(db); err != nil || id != "host-from-before" {
			t.Fatalf("%q %v", id, err)
		}
		db.Close()
	}
	fresh := t.TempDir()
	var ids []string
	for range 2 {
		db, err := statedb.Open(fresh)
		if err != nil {
			t.Fatal(err)
		}
		id, _ := state.HostID(db)
		ids = append(ids, id)
		db.Close()
	}
	if ids[0] == "" || ids[0] != ids[1] {
		t.Fatalf("%v", ids)
	}
}
