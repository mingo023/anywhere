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
	for _, root := range roots(home) {
		got, err := os.ReadFile(skillFile(root, "zoes"))
		if err != nil || !bytes.Equal(got, skill) {
			t.Fatalf("%s: %v", root, err)
		}
	}
}

func TestInstallRemovesTheAnywhereSkillAnOlderBuildWrote(t *testing.T) {
	home := t.TempDir()
	claude, codex := roots(home)[0], roots(home)[1]
	for _, root := range roots(home) {
		os.MkdirAll(filepath.Join(root, "anywhere"), 0o755)
		os.WriteFile(skillFile(root, "anywhere"), []byte("old"), 0o644)
	}
	os.WriteFile(filepath.Join(codex, "anywhere", "notes.md"), []byte("mine"), 0o644)
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(claude, "anywhere")); !os.IsNotExist(err) {
		t.Errorf("claude's anywhere folder stayed: %v", err)
	}
	if _, err := os.Stat(skillFile(codex, "anywhere")); !os.IsNotExist(err) {
		t.Errorf("codex's old skill stayed: %v", err)
	}
	if got, _ := os.ReadFile(filepath.Join(codex, "anywhere", "notes.md")); string(got) != "mine" {
		t.Errorf("a file the user put beside the old skill went: %q", got)
	}
}

func TestRefreshOverwritesAnInstalledCopy(t *testing.T) {
	home := t.TempDir()
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	p := skillFile(roots(home)[0], "zoes")
	os.WriteFile(p, []byte("old"), 0o644)
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(p); !bytes.Equal(got, skill) {
		t.Fatalf("got %s", got)
	}
}

func TestRefreshMovesTheAnywhereSkillToZoesOnlyWhereItWasInstalled(t *testing.T) {
	home := t.TempDir()
	claude, codex := roots(home)[0], roots(home)[1]
	os.MkdirAll(filepath.Join(claude, "anywhere"), 0o755)
	os.WriteFile(skillFile(claude, "anywhere"), []byte("old"), 0o644)
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(skillFile(claude, "zoes")); !bytes.Equal(got, skill) {
		t.Errorf("claude's zoes skill: %q", got)
	}
	if _, err := os.Stat(filepath.Join(claude, "anywhere")); !os.IsNotExist(err) {
		t.Errorf("claude's anywhere folder stayed: %v", err)
	}
	if _, err := os.Stat(codex); !os.IsNotExist(err) {
		t.Errorf("refresh added codex's skills: %v", err)
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
