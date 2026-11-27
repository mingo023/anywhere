package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

func writeConfig(t *testing.T, dir, raw string) {
	t.Helper()
	if err := os.MkdirAll(ConfigDirIn(dir), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(ConfigPath(dir, ConfigFile), []byte(raw), 0o644); err != nil {
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
	if err := os.MkdirAll(ConfigDirIn(dir), 0o755); err != nil {
		t.Fatal(err)
	}
	path := ConfigPath(dir, SetupScript)
	if err := os.WriteFile(path, []byte("echo set up\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestARepoWithoutConfigRunsItsSetupScriptTheWorktreesFirst(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("a setup script never runs on Windows, where sessions run PowerShell")
	}
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
	if runtime.GOOS == "windows" {
		t.Skip("a setup script never runs on Windows, where sessions run PowerShell")
	}
	p := filepath.Join(t.TempDir(), "it's here")
	path := writeScript(t, p)
	got := RepoSetup(p, t.TempDir())
	out, err := exec.Command("sh", "-c", "set -- "+strings.TrimPrefix(got, "bash ")+`; printf %s "$1"`).Output()
	if err != nil || string(out) != path {
		t.Fatalf("%q reads back as %q (%v), want %q", got, out, err, path)
	}
}

func TestScriptEnvNamesTheProjectAndTheTreeInPlaceOfInheritedOnes(t *testing.T) {
	env := []string{"HOME=/h", RootPathVar + "=/old", TreeNameVar + "=old", ZoesTreePathVar + "=/old", ZoesRootPathVar + "="}
	got := ScriptEnv(env, "/repos/app", "/trees/app/calm-otter")
	want := []string{
		"HOME=/h",
		RootPathVar + "=/repos/app", ZoesRootPathVar + "=/repos/app",
		TreePathVar + "=/trees/app/calm-otter", ZoesTreePathVar + "=/trees/app/calm-otter",
		TreeNameVar + "=calm-otter", ZoesTreeNameVar + "=calm-otter",
	}
	if !slices.Equal(got, want) {
		t.Fatalf("got %q, want %q", got, want)
	}
	if env[1] != RootPathVar+"=/old" {
		t.Fatalf("changed its input: %q", env)
	}
}

func put(t *testing.T, root, dir, file string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(root, dir), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, dir, file), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestTheConfigFolderIsZoesUnlessOnlyAnywhereHoldsConfig(t *testing.T) {
	for _, c := range []struct {
		name  string
		files [][2]string
		want  string
	}{
		{"a repo with neither", nil, ConfigDir},
		{"only .anywhere", [][2]string{{LegacyConfigDir, ConfigFile}}, LegacyConfigDir},
		{"only .zoes", [][2]string{{ConfigDir, SetupScript}}, ConfigDir},
		{"both", [][2]string{{LegacyConfigDir, ConfigFile}, {ConfigDir, TeardownScript}}, ConfigDir},
		{"an empty .zoes", [][2]string{{LegacyConfigDir, ConfigFile}, {ConfigDir, "notes.txt"}}, LegacyConfigDir},
	} {
		root := t.TempDir()
		for _, f := range c.files {
			put(t, root, f[0], f[1])
		}
		if got, want := ConfigDirIn(root), filepath.Join(root, c.want); got != want {
			t.Errorf("%s: got %q, want %q", c.name, got, want)
		}
	}
}

func TestAReposSetupAndTeardownComeFromWhicheverFolderHoldsItsConfig(t *testing.T) {
	legacy, current, both := t.TempDir(), t.TempDir(), t.TempDir()
	writeIn(t, legacy, filepath.Join(LegacyConfigDir, ConfigFile), `{"setup":["old up"],"teardown":["old down"]}`)
	writeIn(t, current, filepath.Join(ConfigDir, ConfigFile), `{"setup":["new up"],"teardown":["new down"]}`)
	writeIn(t, both, filepath.Join(LegacyConfigDir, ConfigFile), `{"setup":["old up"],"teardown":["old down"]}`)
	writeIn(t, both, filepath.Join(ConfigDir, ConfigFile), `{"setup":["new up"],"teardown":["new down"]}`)
	for name, c := range map[string]struct{ root, setup, teardown string }{
		"only .anywhere": {legacy, "old up", "old down"},
		"only .zoes":     {current, "new up", "new down"},
		"both":           {both, "new up", "new down"},
	} {
		if got := RepoSetup(c.root, t.TempDir()); got != c.setup {
			t.Errorf("%s setup: got %q", name, got)
		}
		if got := Teardown("", c.root, t.TempDir()); got != c.teardown {
			t.Errorf("%s teardown: got %q", name, got)
		}
	}
}

func TestALegacyReposScriptsRunFromItsAnywhereFolder(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("a script never runs on Windows, where sessions run PowerShell")
	}
	root := t.TempDir()
	writeIn(t, root, filepath.Join(LegacyConfigDir, SetupScript), "exit 0")
	writeIn(t, root, filepath.Join(LegacyConfigDir, TeardownScript), "exit 0")
	if got, want := RepoSetup(root, t.TempDir()), "bash '"+filepath.Join(root, LegacyConfigDir, SetupScript)+"'"; got != want {
		t.Errorf("setup: got %q, want %q", got, want)
	}
	if got, want := Teardown("", root, t.TempDir()), "bash '"+filepath.Join(root, LegacyConfigDir, TeardownScript)+"'"; got != want {
		t.Errorf("teardown: got %q, want %q", got, want)
	}
}
