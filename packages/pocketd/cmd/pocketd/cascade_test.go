package main

import (
	"encoding/json"
	"path/filepath"
	"slices"
	"testing"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/locals"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb/dbtest"
)

func TestRemovingAProjectDeletesItsTicketsAutomationsRunsAndLocalsAndEverySubscriberHearsIt(t *testing.T) {
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	gone, kept := t.TempDir(), t.TempDir()
	for _, dir := range []string{gone, kept} {
		if err := reg.AddProject(dir, nil); err != nil {
			t.Fatal(err)
		}
	}
	localStore, err := locals.Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	tickets, err := board.Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	autos, err := automation.Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	reloadOnRemove(reg, localStore, tickets, autos)

	now := time.Now()
	for _, dir := range []string{gone, kept} {
		if err := localStore.Create("l-"+filepath.Base(dir), dir, "Local"); err != nil {
			t.Fatal(err)
		}
		if _, err := tickets.Save(proto.Ticket{Title: "t", Prompt: "p", Provider: "claude", Folder: dir}, false, now); err != nil {
			t.Fatal(err)
		}
		a, err := autos.Save(proto.Automation{Name: "a", Prompt: "p", Provider: "claude", Folder: dir,
			Schedule: proto.Schedule{Kind: "interval", EveryMin: 60}, Enabled: true}, now)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := autos.Begin(a.ID, now); err != nil {
			t.Fatal(err)
		}
	}
	lists, stop := localStore.Subscribe()
	defer stop()
	boardChanged, autosChanged := tickets.Changed(), autos.Changed()

	if err := reg.RemoveProject(gone); err != nil {
		t.Fatal(err)
	}

	select {
	case raw := <-lists:
		keptJSON, _ := json.Marshal(kept)
		want := `{"type":"local.list","locals":[{"id":"l-` + filepath.Base(kept) + `","project":` + string(keptJSON) + `,"name":"Local"}]}`
		if string(raw) != want {
			t.Errorf("got %s\nwant %s", raw, want)
		}
	case <-time.After(time.Second):
		t.Error("the local list wasn't sent")
	}
	for name, ch := range map[string]<-chan struct{}{"board": boardChanged, "automations": autosChanged} {
		select {
		case <-ch:
		default:
			t.Errorf("%s subscribers weren't woken", name)
		}
	}
	ts, _ := tickets.Snapshot()
	as, runs := autos.Snapshot()
	if len(ts) != 1 || ts[0].Folder != kept || len(as) != 1 || as[0].Folder != kept || len(runs) != 1 || runs[0].AutomationID != as[0].ID {
		t.Errorf("tickets %+v automations %+v runs %+v", ts, as, runs)
	}
	for _, table := range []string{"tickets", "automations", "runs", "locals"} {
		var n int
		if err := db.QueryRow(`SELECT COUNT(*) FROM ` + table).Scan(&n); err != nil || n != 1 {
			t.Errorf("%s has %d rows, %v", table, n, err)
		}
	}
	if !slices.Equal(reg.Load().Projects, []string{kept}) {
		t.Errorf("projects %v", reg.Load().Projects)
	}
}
