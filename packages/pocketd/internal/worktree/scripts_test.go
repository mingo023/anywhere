package worktree

import (
	"os"
	"path/filepath"
	"testing"
)

func writeConfig(t *testing.T, dir, raw string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(dir, ".anywhere"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ConfigFile), []byte(raw), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestARepoSetupIsItsCommandsALineEach(t *testing.T) {
	p, tree := t.TempDir(), t.TempDir()
	writeConfig(t, tree, `{"setup": ["pnpm install", "cp \"$HOME/.env\" .env"], "teardown": ["docker compose down"]}`)
	if got, want := RepoSetup(p, tree), "pnpm install\ncp \"$HOME/.env\" .env"; got != want {
		t.Fatalf("got %q", got)
	}
}

func TestTheWorktreesOwnConfigWinsOverTheProjects(t *testing.T) {
	p, tree := t.TempDir(), t.TempDir()
	writeConfig(t, p, `{"setup": ["make old"]}`)
	writeConfig(t, tree, `{"setup": ["make new"]}`)
	if got := RepoSetup(p, tree); got != "make new" {
		t.Fatalf("got %q", got)
	}
}

func TestTheProjectsConfigStandsInWhenTheWorktreeNamesNoSetup(t *testing.T) {
	for name, inTree := range map[string]string{"missing": "", "no setup": `{"teardown": ["x"]}`, "broken": `{"setup": "not a list"}`} {
		p, tree := t.TempDir(), t.TempDir()
		writeConfig(t, p, `{"setup": ["make"]}`)
		if inTree != "" {
			writeConfig(t, tree, inTree)
		}
		if got := RepoSetup(p, tree); got != "make" {
			t.Errorf("%s: got %q", name, got)
		}
	}
}

func TestARepoWithoutConfigHasNoSetup(t *testing.T) {
	p, tree := t.TempDir(), t.TempDir()
	if got := RepoSetup(p, tree); got != "" {
		t.Fatalf("got %q", got)
	}
}
