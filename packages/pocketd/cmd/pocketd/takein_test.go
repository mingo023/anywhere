package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/handoff"
	"pocketd/internal/orch"
	"pocketd/internal/state"
)

func withoutStore(t *testing.T) (*orchestrator, string) {
	t.Helper()
	home := t.TempDir()
	store, err := orch.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	return &orchestrator{store: store}, home
}

func TestAFirstStartTakesInASessionPerTerminalFromTerminalsJson(t *testing.T) {
	o, home := withoutStore(t)
	raw, err := os.ReadFile("testdata/terminals.json")
	if err != nil {
		t.Fatal(err)
	}
	path := state.Path(home)
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, raw, 0o600)

	f := o.restoring(path)

	if len(f.Terminals) != 2 {
		t.Fatalf("restoring %+v", f)
	}
	s, _ := o.store.Session("a-2")
	if s == nil || s.Conversation != "c2" || s.Access != "edits" || s.Model != "opus" || s.Effort != "high" || s.Agent != "claude" || s.Terminal != "t-2" || !s.Chat || !s.Pinned || s.CreatedAt != 1700000000000 {
		t.Fatalf("session %+v", s)
	}
	saved, _ := o.store.Restorable()
	var e state.Terminal
	json.Unmarshal(saved["a-2"], &e)
	if e.TranscriptPath != "/h/.claude/projects/-w-repo/c2.jsonl" || e.Launch == nil || !e.Launch.Plan {
		t.Fatalf("saved %s", saved["a-2"])
	}

	os.Remove(path)
	if again := o.restoring(path); len(again.Terminals) != 1 || again.Terminals[0].AgentID != "a-2" {
		t.Fatalf("without the file %+v", again)
	}
}

func TestAnUpgradeFromAPocketdWithoutTheStoreTakesInWhatItHandsDown(t *testing.T) {
	o, _ := withoutStore(t)
	f, err := handoff.Read("../../internal/handoff/testdata/format1.json")
	if err != nil {
		t.Fatal(err)
	}
	var handed []state.Terminal
	for _, e := range f.Terminals {
		handed = append(handed, e.Saved)
	}

	o.takeIn(handed)

	s, _ := o.store.Session("a-1")
	if s == nil || s.Conversation != "c1" || s.Terminal != "t-1" || s.Status != "working" || s.CreatedAt != 7 {
		t.Fatalf("session %+v", s)
	}
	if saved, _ := o.store.Restorable(); len(saved) != 1 || saved["a-1"] == nil {
		t.Fatalf("restorable %q", saved)
	}
}
