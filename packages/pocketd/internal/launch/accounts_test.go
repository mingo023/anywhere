package launch

import (
	"path/filepath"
	"strconv"
	"testing"

	"pocketd/internal/accounts"
)

// Without this, the skills scan would read ~/.antigravity.
func TestAntigravityHasNoConfigDir(t *testing.T) {
	l := launcher(t, `["/p"]`)
	home := t.TempDir()
	l.d.Accounts = &accounts.Store{Root: filepath.Join(home, "accounts"), Home: home}
	if dir, err := l.ConfigDir("antigravity", "", "/p"); err != nil || dir != "" {
		t.Fatalf("%q, %v", dir, err)
	}
	if dir, _ := l.ConfigDir("claude", "", "/p"); dir != filepath.Join(home, ".claude") {
		t.Fatalf("claude's is %q", dir)
	}
}

func TestHomeStandsForASessionWithNoProjectButAnotherFolderIsRefused(t *testing.T) {
	l := launcher(t, `["/p"]`)
	home := t.TempDir()
	t.Setenv("HOME", home)
	l.d.Accounts = &accounts.Store{Root: filepath.Join(home, "accounts"), Home: home}
	if dir, err := l.ConfigDir("claude", "", home); err != nil || dir != filepath.Join(home, ".claude") {
		t.Fatalf("home: %q, %v", dir, err)
	}
	if _, err := l.ConfigDir("claude", "", "/etc"); err == nil {
		t.Fatal("/etc was taken")
	}
}

func TestHomeIsAProjectOnceItIsOne(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	if launcher(t, `[`+strconv.Quote(home)+`]`).Projectless(home) {
		t.Fatal("home as a project was taken for none")
	}
	if !launcher(t, `["/p"]`).Projectless(home) {
		t.Fatal("home stood for a project")
	}
}
