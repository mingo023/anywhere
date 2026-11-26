package worktree

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func writeIn(t *testing.T, dir, name, body string) {
	t.Helper()
	os.MkdirAll(filepath.Join(dir, filepath.Dir(name)), 0o755)
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
}

func TestTheProjectSettingsTeardownWinsOverTheReposConfig(t *testing.T) {
	project, tree := t.TempDir(), t.TempDir()
	writeIn(t, tree, ConfigFile, `{"teardown":["make down"]}`)
	if got := Teardown("docker compose down", project, tree); got != "docker compose down" {
		t.Fatalf("got %q", got)
	}
	if got := Teardown("", project, tree); got != "make down" {
		t.Fatalf("without settings, got %q", got)
	}
}

func TestTheProjectCheckoutsConfigStandsInWhenTheTreesNamesNoTeardown(t *testing.T) {
	project, tree := t.TempDir(), t.TempDir()
	writeIn(t, tree, ConfigFile, `{"setup":["make"]}`)
	writeIn(t, project, ConfigFile, `{"teardown":["a","b"]}`)
	if got := Teardown("", project, tree); got != "a\nb" {
		t.Fatalf("got %q", got)
	}
}

func TestTheConfigsTeardownWinsOverTheTeardownScriptAndTheTreesScriptOverTheProjects(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("a teardown script never runs on Windows, where sessions run PowerShell")
	}
	project, tree := t.TempDir(), t.TempDir()
	writeIn(t, project, TeardownScript, "exit 0")
	if got := Teardown("", project, tree); got != "bash '"+filepath.Join(project, TeardownScript)+"'" {
		t.Fatalf("project's script, got %q", got)
	}
	writeIn(t, tree, TeardownScript, "exit 0")
	if got := Teardown("", project, tree); got != "bash '"+filepath.Join(tree, TeardownScript)+"'" {
		t.Fatalf("tree's script, got %q", got)
	}
	writeIn(t, project, ConfigFile, `{"teardown":["make down"]}`)
	if got := Teardown("", project, tree); got != "make down" {
		t.Fatalf("config, got %q", got)
	}
}

func TestARepoWithoutConfigOrScriptHasNoTeardown(t *testing.T) {
	project, tree := t.TempDir(), t.TempDir()
	writeIn(t, tree, ConfigFile, `{"teardown": not json`)
	if got := Teardown("", project, tree); got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestTheTeardownIsToldTheProjectAndTheTree(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("no /bin/sh on Windows")
	}
	project, tree := t.TempDir(), t.TempDir()
	if err := RunTeardown("/bin/sh", `echo "$ANYWHERE_ROOT_PATH $ANYWHERE_WORKSPACE_PATH $ANYWHERE_WORKSPACE_NAME" > seen`, project, tree, os.Environ(), 10*time.Second); err != nil {
		t.Fatal(err)
	}
	raw, _ := os.ReadFile(filepath.Join(tree, "seen"))
	if got, want := strings.TrimSpace(string(raw)), project+" "+tree+" "+filepath.Base(tree); got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestAFailedTeardownSaysTheEndOfItsOutputAndOneThatHangsFailsAtItsLimit(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("no /bin/sh on Windows")
	}
	tree := t.TempDir()
	var failed TeardownFailed
	if err := RunTeardown("/bin/sh", "echo going down; exit 3", tree, tree, os.Environ(), 10*time.Second); !errors.As(err, &failed) || failed.Tail != "going down" {
		t.Fatalf("got %v", err)
	}
	start := time.Now()
	if err := RunTeardown("/bin/sh", "sleep 30", tree, tree, os.Environ(), 300*time.Millisecond); !errors.As(err, &failed) || time.Since(start) > 5*time.Second {
		t.Fatalf("got %v after %v", err, time.Since(start))
	}
}

func TestTheTeardownIsSkippedOnceTheFolderIsGone(t *testing.T) {
	if err := RunTeardown("/bin/sh", "exit 1", t.TempDir(), filepath.Join(t.TempDir(), "gone"), os.Environ(), time.Second); err != nil {
		t.Fatal(err)
	}
}

func TestATeardownAtItsLimitStopsWhatItStartedToo(t *testing.T) {
	tree := t.TempDir()
	if err := RunTeardown("/bin/sh", `(sleep 1; touch late) & wait`, tree, tree, os.Environ(), 200*time.Millisecond); err == nil {
		t.Fatal("the teardown didn't time out")
	}
	time.Sleep(1500 * time.Millisecond)
	if _, err := os.Stat(filepath.Join(tree, "late")); err == nil {
		t.Fatal("a job the teardown started ran on past its limit")
	}
}
