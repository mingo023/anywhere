package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
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

func writeScript(t *testing.T, dir string) string {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(dir, ".anywhere"), 0o755); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, SetupScript)
	if err := os.WriteFile(path, []byte("echo set up\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestARepoWithoutConfigRunsItsSetupScriptTheWorktreesFirst(t *testing.T) {
	p, tree := t.TempDir(), t.TempDir()
	inProject := writeScript(t, p)
	if got, want := RepoSetup(p, tree), "bash '"+inProject+"'"; got != want {
		t.Fatalf("only the checkout's: got %q, want %q", got, want)
	}
	inTree := writeScript(t, tree)
	if got, want := RepoSetup(p, tree), "bash '"+inTree+"'"; got != want {
		t.Fatalf("both: got %q, want %q", got, want)
	}
}

func TestTheConfigsSetupWinsOverTheSetupScript(t *testing.T) {
	p, tree := t.TempDir(), t.TempDir()
	writeScript(t, tree)
	writeConfig(t, p, `{"setup": ["make"]}`)
	if got := RepoSetup(p, tree); got != "make" {
		t.Fatalf("got %q", got)
	}
}

func TestTheSetupScriptsPathIsQuotedForTheShell(t *testing.T) {
	p := filepath.Join(t.TempDir(), "it's here")
	path := writeScript(t, p)
	got := RepoSetup(p, t.TempDir())
	out, err := exec.Command("sh", "-c", "set -- "+strings.TrimPrefix(got, "bash ")+`; printf %s "$1"`).Output()
	if err != nil || string(out) != path {
		t.Fatalf("%q reads back as %q (%v), want %q", got, out, err, path)
	}
}

func TestScriptEnvNamesTheProjectAndTheTreeInPlaceOfInheritedOnes(t *testing.T) {
	env := []string{"HOME=/h", RootPathVar + "=/old", TreeNameVar + "=old"}
	got := ScriptEnv(env, "/repos/app", "/trees/app/calm-otter")
	want := []string{"HOME=/h", RootPathVar + "=/repos/app", TreePathVar + "=/trees/app/calm-otter", TreeNameVar + "=calm-otter"}
	if !slices.Equal(got, want) {
		t.Fatalf("got %q, want %q", got, want)
	}
	if env[1] != RootPathVar+"=/old" {
		t.Fatalf("changed its input: %q", env)
	}
}
