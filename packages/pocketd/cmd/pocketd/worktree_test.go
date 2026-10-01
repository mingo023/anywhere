package main

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/registry"
)

// TestMain pins git's config for the git the code under test runs itself, so
// the owner's hooks, templates or signing can't change results.
func TestMain(m *testing.M) {
	for k, v := range map[string]string{"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
		"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"} {
		os.Setenv(k, v)
	}
	os.Exit(m.Run())
}

// scratch registers a fresh repo named pocket in a desktop.json next to a
// scratch socket, and returns the repo.
func scratch(t *testing.T) string {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	p := filepath.Join(dir, "pocket")
	for _, args := range [][]string{{"init", "-q", "-b", "main", p}, {"-C", p, "commit", "-q", "--allow-empty", "-m", "init"}} {
		cmd := exec.Command("git", args...)
		cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1", "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	f := registry.File{Projects: []string{p}, Repos: map[string]registry.Repo{p: {Worktrees: filepath.Join(dir, "wt")}}}
	raw, _ := json.Marshal(f)
	os.WriteFile(filepath.Join(dir, "desktop.json"), raw, 0o600)
	os.WriteFile(filepath.Join(p, ".env"), []byte("A=1"), 0o600)
	t.Setenv("POCKETD_SOCK", filepath.Join(dir, "pocketd.sock"))
	return p
}

func worktreeCLI(args ...string) (code int, stdout, stderr string) {
	var out, errs bytes.Buffer
	code = worktreeMain(args, &out, &errs)
	return code, out.String(), errs.String()
}

func TestAWorktreeTheCLICreatesIsListedThenRemoved(t *testing.T) {
	p := scratch(t)
	wt := filepath.Join(filepath.Dir(p), "wt", "calm-otter")
	if code, out, errs := worktreeCLI("create", "--project", p, "calm-otter"); code != 0 || out != wt+"\n" || errs != "" {
		t.Fatalf("create: %d %q %q", code, out, errs)
	}
	if _, err := os.Stat(filepath.Join(wt, ".env")); err != nil {
		t.Fatal(err)
	}
	t.Chdir(wt)
	code, out, _ := worktreeCLI("list")
	if lines := strings.Split(strings.TrimSpace(out), "\n"); code != 0 || len(lines) != 2 ||
		strings.Join(strings.Fields(lines[0]), " ") != "pocket pocket "+p+" (main)" ||
		strings.Join(strings.Fields(lines[1]), " ") != "pocket calm-otter "+wt+" on calm-otter" {
		t.Fatalf("list: %d\n%s", code, out)
	}
	if code, _, errs := worktreeCLI("remove", wt); code != 0 {
		t.Fatalf("remove: %d %s", code, errs)
	}
	if _, err := os.Stat(wt); !os.IsNotExist(err) {
		t.Fatalf("folder kept: %v", err)
	}
}

func TestWorktreeListJSONIsTheProjectListShape(t *testing.T) {
	p := scratch(t)
	code, out, _ := worktreeCLI("list", "--json", p)
	var got []map[string]any
	if err := json.Unmarshal([]byte(out), &got); code != 0 || err != nil || len(got) != 1 || got[0]["name"] != "pocket" {
		t.Fatalf("%d %v\n%s", code, err, out)
	}
}

func TestListingOnlyGoneFoldersPrintsAnEmptyJSONList(t *testing.T) {
	p := scratch(t)
	os.RemoveAll(p)
	if code, out, _ := worktreeCLI("list", "--json", p); code != 0 || out != "[]\n" {
		t.Fatalf("%d %q", code, out)
	}
}

func TestWorktreeErrorsExitOneAndUsageExitsTwo(t *testing.T) {
	p := scratch(t)
	t.Chdir(t.TempDir())
	for _, c := range []struct {
		args []string
		code int
		errs string
	}{
		{[]string{"list"}, 1, "pocketd worktree: Unknown project: "},
		{[]string{"create", "--project", p, "MAIN"}, 1, "pocketd worktree: A worktree or branch with this name already exists\n"},
		{[]string{"remove", p}, 1, "pocketd worktree: The main worktree can't be removed\n"},
		{[]string{"create"}, 2, ""},
		{[]string{"prune"}, 2, "usage: pocketd worktree"},
	} {
		code, _, errs := worktreeCLI(c.args...)
		if code != c.code || !strings.HasPrefix(errs, c.errs) {
			t.Errorf("%v: %d %q", c.args, code, errs)
		}
	}
}

func TestRemovingAPathOutsideEveryProjectIsAnUnknownWorktree(t *testing.T) {
	scratch(t)
	if code, _, errs := worktreeCLI("remove", t.TempDir()); code != 1 || !strings.HasPrefix(errs, "pocketd worktree: Not a worktree of this project: ") {
		t.Fatalf("%d %q", code, errs)
	}
}
