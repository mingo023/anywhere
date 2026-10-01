package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
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

func git(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

// repo is a fresh repo named pocket on branch main, at its real path: git
// reports worktrees by real path, and macOS temp dirs sit behind /var -> /private/var.
func repo(t *testing.T) string {
	t.Helper()
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(root, "pocket")
	if err := os.Mkdir(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	git(t, dir, "init", "-q", "-b", "main")
	git(t, dir, "commit", "-q", "--allow-empty", "-m", "init")
	return dir
}

func TestMainComesFirstThenTheOldestWorktree(t *testing.T) {
	p := repo(t)
	zeta, alpha, loose := filepath.Join(p, "..", "zeta"), filepath.Join(p, "..", "alpha"), filepath.Join(p, "..", "loose")
	git(t, p, "worktree", "add", "-q", "-b", "zeta", zeta)
	git(t, p, "worktree", "add", "-q", "-b", "alpha", alpha)
	git(t, p, "worktree", "add", "-q", "--detach", loose)
	got, err := List(p)
	want := []Worktree{
		{Name: "pocket", Path: p, Branch: "main", Main: true},
		{Name: "zeta", Path: filepath.Clean(zeta), Branch: "zeta"},
		{Name: "alpha", Path: filepath.Clean(alpha), Branch: "alpha"},
		{Name: "loose", Path: filepath.Clean(loose)},
	}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestAFolderThatIsNotARepoIsItsOwnMainWorktree(t *testing.T) {
	dir := t.TempDir()
	got, err := List(dir)
	if want := []Worktree{{Name: filepath.Base(dir), Path: dir, Main: true}}; err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestProjectsLeaveOutAFolderThatIsGone(t *testing.T) {
	p := repo(t)
	f := registry.File{Projects: []string{filepath.Join(p, "..", "gone"), p}, Repos: map[string]registry.Repo{p: {Name: "Pocket"}}}
	want := []proto.Project{{Path: p, Name: "Pocket", Worktrees: []proto.Worktree{{Name: "pocket", Path: p, Branch: "main", IsMain: true}}}}
	if got := Projects(f); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}
