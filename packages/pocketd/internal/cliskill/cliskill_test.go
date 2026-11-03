package cliskill

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestInstallWritesTheSkillForClaudeAndCodex(t *testing.T) {
	home := t.TempDir()
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	for _, dir := range []string{".claude", ".agents"} {
		got, err := os.ReadFile(filepath.Join(home, dir, "skills", "anywhere", "SKILL.md"))
		if err != nil || !bytes.Equal(got, skill) {
			t.Fatalf("%s: %v", dir, err)
		}
	}
}

func TestRefreshOverwritesAnInstalledCopy(t *testing.T) {
	home := t.TempDir()
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	p := paths(home)[0]
	os.WriteFile(p, []byte("old"), 0o644)
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(p); !bytes.Equal(got, skill) {
		t.Fatalf("got %s", got)
	}
}

func TestRefreshAddsNothingToAHomeThatNeverInstalled(t *testing.T) {
	home := t.TempDir()
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if entries, _ := os.ReadDir(home); len(entries) != 0 {
		t.Fatalf("got %v", entries)
	}
}
