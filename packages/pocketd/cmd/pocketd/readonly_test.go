package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/push"
	"pocketd/internal/registry"
	"pocketd/internal/statedb"
)

// seedDaemon is what a daemon in home has saved: a project, a push registration and an owner port.
func seedDaemon(t *testing.T, home, project string) {
	t.Helper()
	db, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	if err := reg.AddProject(project, nil); err != nil {
		t.Fatal(err)
	}
	store, _ := push.Open(db)
	store.Register("phone-1", push.Registration{Key: bytes.Repeat([]byte{1}, 32), Env: "production"}, time.Unix(1761300000, 0))
	if _, err := db.Exec(`INSERT INTO config (key, value) VALUES ('ownerListen', '"127.0.0.1:4519"')`); err != nil {
		t.Fatal(err)
	}
}

// readsAsTheDaemonSavedIt runs the three read-only commands against home.
func readsAsTheDaemonSavedIt(t *testing.T, home, project string) {
	t.Helper()
	f, err := loadRegistry(home)
	if err != nil || len(f.Projects) != 1 || f.Projects[0] != project {
		t.Fatalf("worktree reads %+v %v", f.Projects, err)
	}
	var status bytes.Buffer
	if err := pushStatus(home, []string{"--json"}, &status, time.Now()); err != nil || !strings.Contains(status.String(), "phone-1") {
		t.Fatalf("push status %v %s", err, status.String())
	}
	var pair bytes.Buffer
	if err := ownerPair(home, []string{"--json"}, &pair); err != nil {
		t.Fatalf("owner pair: %v", err)
	}
	var tok ownerToken
	if err := json.Unmarshal(pair.Bytes(), &tok); err != nil || tok.Port != 4519 {
		t.Fatalf("owner pair %v %s", err, pair.Bytes())
	}
}

func TestTheReadOnlyCommandsReadWhileTheDaemonHoldsTheDatabaseOpen(t *testing.T) {
	home, project := t.TempDir(), t.TempDir()
	seedDaemon(t, home, project)
	daemon, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer daemon.Close()
	readsAsTheDaemonSavedIt(t, home, project)

	reg, _ := registry.Open(daemon)
	later := t.TempDir()
	if err := reg.AddProject(later, nil); err != nil {
		t.Fatal(err)
	}
	if f, err := loadRegistry(home); err != nil || len(f.Projects) != 2 {
		t.Fatalf("a project the daemon added after the first read: %+v %v", f.Projects, err)
	}
}

func TestTheReadOnlyCommandsReadWhileTheDaemonIsStopped(t *testing.T) {
	home, project := t.TempDir(), t.TempDir()
	seedDaemon(t, home, project)
	readsAsTheDaemonSavedIt(t, home, project)
}

func TestTheReadOnlyCommandsSeeEmptyStateAndMakeNoDatabaseBeforeADaemonRan(t *testing.T) {
	home := t.TempDir()
	if f, err := loadRegistry(home); err != nil || len(f.Projects) != 0 {
		t.Fatalf("worktree reads %+v %v", f.Projects, err)
	}
	var status bytes.Buffer
	if err := pushStatus(home, nil, &status, time.Now()); err != nil || !strings.Contains(status.String(), "No phone is registered") {
		t.Fatalf("push status %v %s", err, status.String())
	}
	if err := ownerPair(home, []string{"--json"}, &bytes.Buffer{}); err == nil || !strings.Contains(err.Error(), "pocketd config set ownerListen") {
		t.Fatalf("owner pair says %v", err)
	}
	if _, err := os.Stat(statedb.Path(home)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("a read made %s: %v", statedb.Path(home), err)
	}
	if entries, _ := os.ReadDir(home); len(entries) != 0 {
		t.Fatalf("a read wrote %v", entries)
	}
}

func TestOwnerPairBeforeAnyServeTakesOwnerListenFromConfigJSONAndLeavesItAlone(t *testing.T) {
	home := t.TempDir()
	original := []byte(`{"port": 4600, "ownerListen": "127.0.0.1:4519"}`)
	os.WriteFile(filepath.Join(home, "config.json"), original, 0o600)
	var pair bytes.Buffer
	if err := ownerPair(home, []string{"--json"}, &pair); err != nil {
		t.Fatal(err)
	}
	var tok ownerToken
	if err := json.Unmarshal(pair.Bytes(), &tok); err != nil || tok.Port != 4519 {
		t.Fatalf("%v %s", err, pair.Bytes())
	}
	if now, _ := os.ReadFile(filepath.Join(home, "config.json")); !bytes.Equal(now, original) {
		t.Fatalf("config.json became %s", now)
	}
	if _, err := os.Stat(statedb.Path(home)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("made %s: %v", statedb.Path(home), err)
	}
}

func TestTheReadOnlyCommandsCannotWriteTheDatabase(t *testing.T) {
	home := t.TempDir()
	seedDaemon(t, home, t.TempDir())
	db, err := openReadOnly(home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if _, err := db.Exec(`DELETE FROM projects`); err == nil {
		t.Fatal("a read-only handle deleted the projects")
	}
}
