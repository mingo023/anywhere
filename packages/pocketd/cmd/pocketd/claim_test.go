package main

import (
	"os/exec"
	"path/filepath"
	"reflect"
	"slices"
	"testing"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/names"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

func TestTheFoldersOnRecordAreNamedWorktreesTicketsAndRunsWorktreesAndTerminalsFolders(t *testing.T) {
	home, now := t.TempDir(), time.Now()
	named := names.Open(home)
	named.Rename("/wt/named", "Fix login")
	terminals := terminal.NewManager()
	t.Cleanup(func() { terminals.CloseAll(0) })
	if _, err := terminals.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Cwd: t.TempDir()}); err != nil {
		t.Fatal(err)
	}
	tickets := board.Open(home)
	ticket, err := tickets.Save(proto.Ticket{Title: "Fix", Prompt: "fix", Provider: "claude", Folder: "/repo"}, false, now)
	if err != nil {
		t.Fatal(err)
	}
	tickets.Update(ticket.ID, func(x *proto.Ticket) { x.Worktree = "/wt/ticket" })
	autos := automation.Open(home)
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
	f := registry.New(registry.Path()).Load()
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
