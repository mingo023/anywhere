//go:build unix

package e2e

import (
	"crypto/sha256"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

var stateCaps = []string{"registry.v2", "locals.v1", proto.CapBoard, proto.CapAutomations}

var stateFrames = []string{"registry.list", "local.list", "board", "automations"}

// snapshot is the raw first frame of each of stateFrames that a new owner gets on hello.
func snapshot(h *Harness) map[string]string {
	h.t.Helper()
	o := h.Owner(stateCaps...)
	out := map[string]string{}
	o.WaitFor("every snapshot", func(m Message) bool {
		if slices.Contains(stateFrames, m.Type) && out[m.Type] == "" {
			out[m.Type] = m.Raw
		}
		return len(out) == len(stateFrames)
	})
	return out
}

// jsonFiles is the sha256 of each JSON file a pocketd from before pocketd.db kept, by path under home.
func jsonFiles(t *testing.T, home string) map[string][32]byte {
	t.Helper()
	out := map[string][32]byte{}
	for _, rel := range []string{"state/registry.json", "state/locals.json", "state/automations.json", "state/board.json", "state/hosts.json",
		"state/names.json", "state/host.json", "devices.json", "push.json", "config.json"} {
		if b, err := os.ReadFile(filepath.Join(home, rel)); err == nil {
			out[rel] = sha256.Sum256(b)
		}
	}
	return out
}

func sameFiles(t *testing.T, when string, want, got map[string][32]byte) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("%s: files %d, want %d", when, len(got), len(want))
	}
	for rel, sum := range want {
		if got[rel] != sum {
			t.Fatalf("%s: %s changed", when, rel)
		}
	}
}

// The upgrade gate for pocketd.db: the state a pocketd from before it kept in its JSON files comes back in the same
// frames, a project's removal takes its Tickets, Automations and Locals with it, and the files stay as they were.
func TestAnUpgradeFromMainServesItsStateInTheSameFramesAndRemovingAProjectTakesItsRowsButNoFile(t *testing.T) {
	old := jsonEraPocketd(t)
	h := Start(t, launchReady(""), func(h *Harness) {
		h.exe = filepath.Join(h.Home, "bin", "pocketd")
		copyFile(t, old, h.exe)
	})
	other := filepath.Join(h.Home, "other")
	tree := filepath.Join(h.Home, "wt", "repo-fox")
	os.MkdirAll(other, 0o755)
	if out, err := exec.Command("git", "-C", h.Repo, "worktree", "add", "-q", "-b", "fox", tree).CombinedOutput(); err != nil {
		t.Fatalf("worktree add: %v: %s", err, out)
	}

	o := h.Owner(stateCaps...)
	o.Send(map[string]any{"type": "project.add", "id": "add", "path": other, "repo": map[string]any{"name": "Other"}})
	o.WaitFor("the project added", func(m Message) bool { return m.ID == "add" && m.Type == "ack" })
	for id, folder := range map[string]string{"t1": h.Repo, "t3": other} {
		d := ticketDraft(h, id)
		d["folder"] = folder
		if a := o.Board("save", id, map[string]any{"ticket": d}); a.Type != "ack" {
			t.Fatalf("ticket %s: %s", id, a.Raw)
		}
	}
	if a := o.Automation("save", "a1", map[string]any{"automation": draft(h, "hello")}); a.Type != "ack" {
		t.Fatalf("automation: %s", a.Raw)
	}
	o.Send(map[string]any{"type": "local.create", "id": "l", "localId": "local-1", "project": other, "name": "Local 2"})
	o.WaitFor("the local", func(m Message) bool { return m.Type == "local.list" && containsLocal(m.Raw, "local-1") })
	h.Shutdown()
	// A client can save a Ticket only in a project's own folder; one in a worktree comes from an agent's MCP call.
	inWorktree(t, filepath.Join(h.Home, "state", "board.json"), tree)
	h.up()

	before := snapshot(h)
	h.Shutdown()
	files := jsonFiles(t, h.Home)

	h.replaceBinary(filepath.Join(binDir, "pocketd"))
	h.up()
	after := snapshot(h)
	for _, kind := range stateFrames {
		if after[kind] != before[kind] {
			t.Fatalf("%s after the upgrade:\n%s\nwant\n%s", kind, after[kind], before[kind])
		}
	}
	sameFiles(t, "after the import", files, jsonFiles(t, h.Home))

	owner, watcher := h.Owner(stateCaps...), h.Owner(stateCaps...)
	remove(owner, watcher, "rm", other, "the board and locals without other's", func(m Message) (bool, bool) {
		return m.Type == "board" && len(m.Tickets) == 2, m.Type == "local.list" && !containsLocal(m.Raw, "local-1")
	})
	remove(owner, watcher, "rm2", h.Repo, "an empty board and no automations", func(m Message) (bool, bool) {
		return m.Type == "board" && len(m.Tickets) == 0, m.Type == "automations" && len(m.Automations) == 0
	})
	sameFiles(t, "after a change", files, jsonFiles(t, h.Home))

	h.Restart()
	again := snapshot(h)
	if !strings.Contains(again["board"], `"tickets":[]`) || !strings.Contains(again["automations"], `"automations":[]`) || !strings.Contains(again["local.list"], `"locals":[]`) {
		t.Fatalf("a removed project's rows came back: %v", again)
	}
}

// remove has owner remove path and waits, on both phones, for the two frames lists picks out. The lists go
// out before the owner's ack, so its wait takes the ack too.
func remove(owner, watcher *Phone, id, path, what string, lists func(Message) (bool, bool)) {
	owner.t.Helper()
	owner.Send(map[string]any{"type": "project.remove", "id": id, "path": path})
	for _, p := range []*Phone{owner, watcher} {
		acked, a, b := p == watcher, false, false
		p.WaitFor(what, func(m Message) bool {
			x, y := lists(m)
			acked, a, b = acked || m.ID == id && m.Type == "ack", a || x, b || y
			return acked && a && b
		})
	}
}

// inWorktree adds a copy of board.json's first Ticket whose folder is tree.
func inWorktree(t *testing.T, path, tree string) {
	t.Helper()
	var f map[string]any
	b, err := os.ReadFile(path)
	if err == nil {
		err = json.Unmarshal(b, &f)
	}
	if err != nil {
		t.Fatal(err)
	}
	tickets := f["tickets"].([]any)
	copied := map[string]any{}
	for k, v := range tickets[0].(map[string]any) {
		copied[k] = v
	}
	last := f["lastNumber"].(float64) + 1
	copied["id"], copied["folder"], copied["number"], f["lastNumber"] = "in-worktree", tree, last, last
	f["tickets"] = append(tickets, copied)
	if b, err = json.Marshal(f); err == nil {
		err = os.WriteFile(path, b, 0o600)
	}
	if err != nil {
		t.Fatal(err)
	}
}

func containsLocal(raw, id string) bool {
	var l proto.LocalList
	if json.Unmarshal([]byte(raw), &l) != nil {
		return false
	}
	for _, x := range l.Locals {
		if x.ID == id {
			return true
		}
	}
	return false
}
