package events

import (
	"bytes"
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

var update = flag.Bool("update", false, "rewrite testdata")

func yes() *bool { v := true; return &v }
func no() *bool  { v := false; return &v }

func golden(t *testing.T, name string, got []byte) {
	t.Helper()
	path := filepath.Join("testdata", name)
	if *update {
		if err := os.WriteFile(path, got, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	want, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("%s differs; got:\n%s", name, got)
	}
}

func TestEventSchemaMatchesGolden(t *testing.T) {
	home := t.TempDir()
	l, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	ts := "2026-09-30T10:00:00.000Z"
	for _, e := range []Event{
		{Kind: "start", Version: "5091a01c0ffe", PID: 42, Service: yes()},
		{Kind: "stop", Reason: "signal"},
		{Kind: "tick"},
		{Kind: "status", Agent: "a1", Provider: "claude", From: "working", To: "needsYou"},
		{Kind: "seen", Agent: "a1", Principal: "device"},
		{Kind: "answer", Agent: "a1", Principal: "device", Decision: "allow"},
		{Kind: "prompt", Agent: "a1", Provider: "codex", Origin: "phone", Ack: "ok"},
		{Kind: "create", Origin: "desktop", Provider: "claude", OK: no(), Code: "spawn_failed"},
		{Kind: "restore", Agent: "a1", OK: yes(), MS: 1200, Outcome: "interrupted"},
		{Kind: "restore", Agent: "a2", OK: no(), MS: 30000, Outcome: "failed", Reason: "resume_timeout"},
		{Kind: "push", Agent: "a1", Alert: "needsYou", Acted: yes()},
		{Kind: "refusal", Surface: "ops", Code: "scope_denied"},
	} {
		e.TS = ts
		l.Emit(e)
	}
	l.Close()
	got, _ := os.ReadFile(filepath.Join(home, "events.jsonl"))
	golden(t, "schema.jsonl", got)
}

func TestLogRotatesAtTenMiB(t *testing.T) {
	home := t.TempDir()
	l, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	big := Event{Kind: "status", Agent: strings.Repeat("a", 1000)}
	for range 11 << 10 {
		l.Emit(big)
	}
	old, err := os.Stat(filepath.Join(home, "events.jsonl.1"))
	if err != nil || old.Size() > 10<<20 || old.Size() < 9<<20 {
		t.Fatalf("rotated file: %v %v", old, err)
	}
	if cur, _ := os.Stat(filepath.Join(home, "events.jsonl")); cur.Size() >= 2<<20 {
		t.Fatalf("current file %d bytes", cur.Size())
	}
}

func TestEventsFileIs0600(t *testing.T) {
	home := t.TempDir()
	l, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	l.Emit(Event{Kind: "tick"})
	l.Close()
	st, err := os.Stat(filepath.Join(home, "events.jsonl"))
	if err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("%v %v", st.Mode(), err)
	}
}

func TestANilLogDropsEvents(t *testing.T) {
	var l *Log
	l.Emit(Event{Kind: "tick"})
}

func TestStatsOnFixture(t *testing.T) {
	f, err := os.Open(filepath.Join("testdata", "events.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	since, _ := time.Parse(Layout, "2026-09-30T00:00:00.000Z")
	golden(t, "stats.txt", []byte(Stats(f, since).Text()))
}

func TestAnIdleGapBeforeWorkResumesIsNotUnreachable(t *testing.T) {
	lines := []string{
		`{"ts":"2026-09-30T10:00:00.000Z","kind":"start"}`,
		`{"ts":"2026-09-30T10:00:10.000Z","kind":"status","agent":"a1","from":"idle","to":"working"}`,
		`{"ts":"2026-09-30T10:01:10.000Z","kind":"tick"}`,
		`{"ts":"2026-09-30T10:01:30.000Z","kind":"status","agent":"a1","from":"working","to":"done"}`,
		`{"ts":"2026-09-30T10:02:10.000Z","kind":"tick"}`,
		`{"ts":"2026-09-30T10:03:10.000Z","kind":"tick"}`,
		`{"ts":"2026-09-30T15:00:00.000Z","kind":"status","agent":"a1","from":"done","to":"working"}`,
		`{"ts":"2026-09-30T15:01:00.000Z","kind":"tick"}`,
	}
	if got := Stats(strings.NewReader(strings.Join(lines, "\n")), time.Time{}).UnreachableMinutes; got != 0 {
		t.Fatalf("%.1f min unreachable, want 0: the Mac slept while nothing worked", got)
	}
}

func TestStatsWithNoEventsShowsDashes(t *testing.T) {
	text := Stats(strings.NewReader(""), time.Time{}).Text()
	for _, row := range []string{"Done → Seen           —", "Push precision        —", "Refusals              —"} {
		if !strings.Contains(text, row) {
			t.Errorf("missing %q in\n%s", row, text)
		}
	}
}

func TestANewTurnAfterDoneIsNotASighting(t *testing.T) {
	lines := []string{
		`{"ts":"2026-09-30T10:00:00.000Z","kind":"status","agent":"a1","from":"working","to":"done"}`,
		`{"ts":"2026-09-30T10:00:30.000Z","kind":"status","agent":"a1","from":"done","to":"working"}`,
		`{"ts":"2026-09-30T10:05:00.000Z","kind":"status","agent":"a1","from":"working","to":"done"}`,
		`{"ts":"2026-09-30T10:05:10.000Z","kind":"status","agent":"a1","from":"done","to":"idle"}`,
	}
	got := Stats(strings.NewReader(strings.Join(lines, "\n")), time.Time{}).Seen
	if got.N != 1 || got.P50 != 10_000 {
		t.Fatalf("%+v, want one sighting of 10s", got)
	}
}
