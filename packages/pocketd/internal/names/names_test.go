package names

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"
	"time"
)

func next(t *testing.T, msgs <-chan []byte) map[string]any {
	t.Helper()
	select {
	case raw := <-msgs:
		var m map[string]any
		json.Unmarshal(raw, &m)
		return m
	case <-time.After(time.Second):
		t.Fatal("nothing published")
		return nil
	}
}

func TestNamingNamesAWorktreeUntilTheUserRenamesIt(t *testing.T) {
	n := Open(t.TempDir())
	if !n.Auto("/w", "First") || !n.Auto("/w", "Refined") {
		t.Fatal("auto refused")
	}
	n.Rename("/w", "Mine")
	if n.Auto("/w", "Later") || n.Titles()["/w"] != "Mine" {
		t.Fatalf("%v", n.Titles())
	}
}

func TestAnEmptyRenameClearsTheNameAndKeepsNamingOff(t *testing.T) {
	n := Open(t.TempDir())
	n.Auto("/w", "First")
	n.Rename("/w", "  ")
	if _, ok := n.Titles()["/w"]; ok || n.Auto("/w", "Again") {
		t.Fatalf("%v", n.Titles())
	}
}

func TestNamesSurviveARestart(t *testing.T) {
	home := t.TempDir()
	Open(home).Auto("/w", "Fix login")
	if got := Open(home).Titles(); !reflect.DeepEqual(got, map[string]string{"/w": "Fix login"}) {
		t.Fatalf("%v", got)
	}
}

func TestEachChangeSendsEveryName(t *testing.T) {
	n := Open(t.TempDir())
	n.Auto("/a", "A")
	msgs, stop := n.Subscribe()
	defer stop()
	n.Rename("/b", "B")
	if m := next(t, msgs); m["type"] != "worktree.names" || !reflect.DeepEqual(m["names"], map[string]any{"/a": "A", "/b": "B"}) {
		t.Fatalf("%v", m)
	}
	n.Failed("a1")
	if m := next(t, msgs); m["type"] != "naming.failed" || m["agentId"] != "a1" {
		t.Fatalf("%v", m)
	}
}

func TestTitlesAreTrimmedAndClipped(t *testing.T) {
	n := Open(t.TempDir())
	n.Rename("/w", "  "+strings.Repeat("é", 200)+" ")
	if got := n.Titles()["/w"]; got != strings.Repeat("é", MaxTitle) {
		t.Fatalf("%d runes", len([]rune(got)))
	}
}

func TestANewWorktreeAtAForgottenPathStartsUnnamed(t *testing.T) {
	n := Open(t.TempDir())
	n.Rename("/w", "Mine")
	n.Forget("/w")
	if !n.Auto("/w", "New") || n.Titles()["/w"] != "New" {
		t.Fatalf("%v", n.Titles())
	}
}
