package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"slices"
	"strconv"
	"testing"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/config"
	"pocketd/internal/names"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb"
	"pocketd/internal/statedb/dbtest"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

func TestTheFoldersOnRecordAreNamedWorktreesTicketsAndRunsWorktreesAndTerminalsFolders(t *testing.T) {
	now := time.Now()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}
	if err := registry.Insert(tx, registry.State{Projects: []proto.RegistryProject{{ID: "p1", Path: "/repo"}}}); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(); err != nil {
		t.Fatal(err)
	}
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	named, err := names.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	named.Rename("/wt/named", "Fix login")
	terminals := terminal.NewManager()
	t.Cleanup(func() { terminals.CloseAll(0) })
	if _, err := terminals.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: t.TempDir()}); err != nil {
		t.Fatal(err)
	}
	tickets, err := board.Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	ticket, err := tickets.Save(proto.Ticket{Title: "Fix", Prompt: "fix", Provider: "claude", Folder: "/repo"}, false, now)
	if err != nil {
		t.Fatal(err)
	}
	tickets.Update(ticket.ID, func(x *proto.Ticket) { x.Worktree = "/wt/ticket" })
	autos, err := automation.Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	auto, err := autos.Save(proto.Automation{Name: "Review", Prompt: "review", Provider: "claude", Folder: "/repo",
		Schedule: proto.Schedule{Kind: "days", Days: []int{1}, Time: "09:00"}, Enabled: true}, now)
	if err != nil {
		t.Fatal(err)
	}
	run, err := autos.Begin(auto.ID, now)
	if err != nil {
		t.Fatal(err)
	}
	autos.Update(run.ID, func(x *proto.Run) { x.Worktree = "/wt/run" })
	got := usedFolders(named, terminals, tickets, autos)
	for _, want := range []string{"/wt/named", terminals.List()[0].Cwd, "/wt/ticket", "/wt/run"} {
		if !slices.Contains(got, want) {
			t.Fatalf("%q isn't in %q", want, got)
		}
	}
}

func TestWorktreesOnRecordAreClaimedTheFirstTimeOnly(t *testing.T) {
	p := scratch(t)
	f := openRegistry(t, config.Home()).Load()
	early, late := filepath.Join(worktree.Dir(f, p), "early"), filepath.Join(worktree.Dir(f, p), "late")
	for _, w := range []string{early, late} {
		if out, err := exec.Command("git", "-C", p, "worktree", "add", "-q", "-b", filepath.Base(w), w).CombinedOutput(); err != nil {
			t.Fatalf("%v\n%s", err, out)
		}
	}
	home := t.TempDir()
	claimWorktrees(home, f, []string{early})
	claimWorktrees(home, f, []string{late})
	var got []string
	for _, w := range worktree.Projects(f)[0].Worktrees {
		got = append(got, w.Name)
	}
	if !reflect.DeepEqual(got, []string{"pocket", "early"}) {
		t.Fatalf("got %v", got)
	}
}

func TestTheWorktreesAgentsMadeBeforeAreClaimedOnceAndCleanedUp(t *testing.T) {
	p := scratch(t)
	reg := openRegistry(t, config.Home())
	add := func(name string) string {
		c, err := worktree.Add(reg.Load(), p, name, "")
		if err != nil {
			t.Fatal(err)
		}
		return c.Path
	}
	subagent, isolated := add("subagent"), add("isolated")
	started := time.Now().Add(time.Second).UnixMilli()
	home := t.TempDir()
	os.WriteFile(filepath.Join(home, "desktop.json"), []byte(`{"subtrees":{"`+subagent+`":{"started":`+strconv.FormatInt(started, 10)+`},"//devbox/x":{"started":1}}}`), 0o600)
	later := add("later")
	var reaped []string
	reap := func(tree string) { reaped = append(reaped, tree) }
	claimAgentWorktrees(home, reg, []orch.Session{{Cwd: isolated, CreatedAt: started}}, reap)
	want := []string{subagent, isolated}
	if got := reg.Load().AgentWorktrees; !slices.Equal(slices.Sorted(slices.Values(got)), slices.Sorted(slices.Values(want))) {
		t.Fatalf("recorded %v, want %v", got, want)
	}
	if !slices.Equal(slices.Sorted(slices.Values(reaped)), slices.Sorted(slices.Values(want))) {
		t.Fatalf("cleaned up %v", reaped)
	}
	claimAgentWorktrees(home, reg, []orch.Session{{Cwd: later, CreatedAt: time.Now().UnixMilli()}}, reap)
	if len(reaped) != 2 || slices.Contains(reg.Load().AgentWorktrees, later) {
		t.Fatal("claimed a second time")
	}
}

func openRegistry(t *testing.T, home string) *registry.Registry {
	t.Helper()
	db, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	r, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	return r
}
