package worktree

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/registry"
)

func registered(p string, r registry.Repo) registry.File {
	return registry.File{Projects: []string{p}, Repos: map[string]registry.Repo{p: r}}
}

func TestANameMustBeFreeAndValidForGit(t *testing.T) {
	taken := []string{"main", "Foo", "fix/login"}
	if err := Validate("fix-login_2.0", taken); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"main", "foo", "fix"} {
		if err := Validate(name, taken); Code(err) != "worktree_exists" {
			t.Errorf("%q: %v", name, err)
		}
	}
	for _, name := range []string{"", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên", "HEAD"} {
		if err := Validate(name, taken); Code(err) != "invalid_name" {
			t.Errorf("%q: %v", name, err)
		}
	}
}

func TestTakenNamesAreBranchesAndFolders(t *testing.T) {
	p := repo(t)
	git(t, p, "branch", "feat/x")
	dir := t.TempDir()
	os.Mkdir(filepath.Join(dir, "old"), 0o755)
	taken, err := Taken(p, dir)
	if err != nil || !slices.Contains(taken, "main") || !slices.Contains(taken, "feat/x") || !slices.Contains(taken, "old") {
		t.Fatalf("taken = %v, %v", taken, err)
	}
}

func TestWorktreesGoWhereTheRepoSaysElseUnderHome(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	for _, c := range []struct {
		repo registry.Repo
		want string
	}{
		{registry.Repo{Worktrees: "/wt"}, "/wt"},
		{registry.Repo{Name: "Pocket"}, filepath.Join(home, ".worktrees", "Pocket")},
		{registry.Repo{}, filepath.Join(home, ".worktrees", "pocket")},
	} {
		if got := Dir(registered("/w/pocket", c.repo), "/w/pocket"); got != c.want {
			t.Errorf("%+v: got %s", c.repo, got)
		}
	}
}

func TestTheDeepestRegisteredFolderContainsADir(t *testing.T) {
	f := registry.File{Projects: []string{"/w", "/w/pocket"}, Repos: map[string]registry.Repo{"/w/pocket": {Worktrees: "/wt/pocket"}}}
	for dir, want := range map[string]string{"/w/pocket/src": "/w/pocket", "/w/other": "/w", "/wt/pocket/calm-otter": "/w/pocket"} {
		if got, ok := Containing(f, dir); !ok || got != want {
			t.Errorf("%s: got %q", dir, got)
		}
	}
	if _, ok := Containing(f, "/elsewhere"); ok {
		t.Error("/elsewhere is in a project")
	}
}

func TestCreateBranchesFromTheRepoBaseIntoItsWorktreesFolder(t *testing.T) {
	p := repo(t)
	git(t, p, "switch", "-q", "-c", "dev")
	git(t, p, "commit", "-q", "--allow-empty", "-m", "dev")
	git(t, p, "switch", "-q", "main")
	wt := filepath.Join(t.TempDir(), "wt")
	c, err := Create(registered(p, registry.Repo{Base: "dev", Worktrees: wt}), p, "fix-login", "")
	if err != nil || c.Path != filepath.Join(wt, "fix-login") || c.Branch != "fix-login" || c.Base != "dev" {
		t.Fatalf("%+v, %v", c, err)
	}
	if head, dev := git(t, c.Path, "rev-parse", "HEAD"), git(t, p, "rev-parse", "dev"); head != dev {
		t.Fatalf("HEAD %s, dev %s", head, dev)
	}
}

func TestCreateTakesAnExplicitBaseAndDefaultsUnderHome(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	p := repo(t)
	c, err := Create(registered(p, registry.Repo{}), p, "calm-otter", "main")
	if want := filepath.Join(os.Getenv("HOME"), ".worktrees", "pocket", "calm-otter"); err != nil || c.Path != want || c.Base != "main" {
		t.Fatalf("%+v, %v", c, err)
	}
}

func TestARepoWithoutBranchesBranchesFromHEAD(t *testing.T) {
	p := repo(t)
	git(t, p, "checkout", "-q", "--detach")
	git(t, p, "branch", "-q", "-D", "main")
	c, err := Create(registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")}), p, "calm-otter", "")
	if err != nil || c.Base != "HEAD" {
		t.Fatalf("%+v, %v", c, err)
	}
	if head, at := git(t, c.Path, "rev-parse", "HEAD"), git(t, p, "rev-parse", "HEAD"); head != at {
		t.Fatalf("HEAD %s, repo at %s", head, at)
	}
}

func TestCreateRefusesATakenNameBeforeGitRuns(t *testing.T) {
	p := repo(t)
	wt := filepath.Join(t.TempDir(), "wt")
	if _, err := Create(registered(p, registry.Repo{Worktrees: wt}), p, "MAIN", ""); Code(err) != "worktree_exists" {
		t.Fatal(err)
	}
	if _, err := os.Stat(wt); !os.IsNotExist(err) {
		t.Fatalf("folder made: %v", err)
	}
}

func TestCreateInAFolderThatIsNotARepoFailsWithGitsError(t *testing.T) {
	p := t.TempDir()
	if _, err := Create(registered(p, registry.Repo{}), p, "x", ""); err == nil || !strings.Contains(err.Error(), "not a git repository") {
		t.Fatal(err)
	}
}

func TestCreateRefusesAProjectThatIsNotRegistered(t *testing.T) {
	p := repo(t)
	if _, err := Create(registry.File{}, p, "x", ""); Code(err) != "unknown_project" {
		t.Fatal(err)
	}
}

func TestRemoveKeepsTheBranchAndRefusesTheMainWorktree(t *testing.T) {
	p := repo(t)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	c, err := Create(f, p, "calm-otter", "")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(c.Path, "dirty.txt"), []byte("x"), 0o644)
	if err := Remove(f, p, c.Path); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(c.Path); !os.IsNotExist(err) {
		t.Fatalf("folder kept: %v", err)
	}
	if b := git(t, p, "branch", "--list", "calm-otter"); b != "calm-otter" {
		t.Fatalf("branch = %q", b)
	}
	if err := Remove(f, p, p); Code(err) != "main_worktree" {
		t.Fatal(err)
	}
	if err := Remove(f, p, "/nowhere"); Code(err) != "unknown_worktree" {
		t.Fatal(err)
	}
}

func TestABaseThatLooksLikeAFlagIsNotAnOption(t *testing.T) {
	p := repo(t)
	wt := filepath.Join(t.TempDir(), "wt")
	if _, err := Add(registered(p, registry.Repo{Worktrees: wt}), p, "calm-otter", "--lock"); err == nil {
		t.Fatal("git took the base as an option")
	}
	if _, err := os.Stat(filepath.Join(wt, "calm-otter")); !os.IsNotExist(err) {
		t.Fatalf("worktree made: %v", err)
	}
}
