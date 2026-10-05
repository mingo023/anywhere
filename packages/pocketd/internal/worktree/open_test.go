package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/registry"
)

// commitOn adds a commit to ref in the bare repo, starting ref at main when
// the bare repo lacks it.
func commitOn(t *testing.T, bare, ref string) string {
	t.Helper()
	parent := "main"
	if exec.Command("git", "-C", bare, "rev-parse", "-q", "--verify", ref).Run() == nil {
		parent = ref
	}
	c := git(t, bare, "commit-tree", "-p", parent, "-m", ref, git(t, bare, "rev-parse", "main^{tree}"))
	git(t, bare, "update-ref", ref, c)
	return c
}

// origin is the bare repo pushed made for p.
func origin(p string) string { return filepath.Join(filepath.Dir(p), "origin.git") }

// opened opens branch of p in a fresh worktrees folder and adds it.
func opened(t *testing.T, p, branch string) (*Open, Created) {
	t.Helper()
	o, err := PrepareBranch(registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")}), p, "", branch, os.Environ())
	if err != nil {
		t.Fatal(err)
	}
	c, err := o.Add()
	if err != nil {
		t.Fatal(err)
	}
	return o, c
}

func TestAFolderIsTheBranchWithDashesAndANumberWhenTaken(t *testing.T) {
	if got := Folder("fix/login", nil); got != "fix-login" {
		t.Error(got)
	}
	if got := Folder("fix/login", []string{"fix-login", "Fix-Login-2"}); got != "fix-login-3" {
		t.Error(got)
	}
}

func TestALocalBranchOpensAsItIs(t *testing.T) {
	p := repo(t)
	git(t, p, "branch", "fix/login")
	o, c := opened(t, p, "fix/login")
	if filepath.Base(c.Path) != "fix-login" || c.Branch != "fix/login" || o.Note != "" {
		t.Fatalf("%+v, note %q", c, o.Note)
	}
	if b := git(t, c.Path, "branch", "--show-current"); b != "fix/login" {
		t.Fatalf("on %q", b)
	}
}

func TestABranchOnlyOnOriginTracksIt(t *testing.T) {
	p, _ := pushed(t)
	remote := commitOn(t, origin(p), "refs/heads/feat")
	_, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != remote {
		t.Fatalf("HEAD %s, origin %s", head, remote)
	}
	if up := git(t, c.Path, "rev-parse", "--abbrev-ref", "@{upstream}"); up != "origin/feat" {
		t.Fatalf("upstream %q", up)
	}
}

func TestABranchBehindOriginIsFastForwarded(t *testing.T) {
	p, _ := pushed(t)
	git(t, p, "branch", "feat")
	remote := commitOn(t, origin(p), "refs/heads/feat")
	o, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != remote || o.Note != "Fast-forwarded feat" {
		t.Fatalf("HEAD %s, origin %s, note %q", head, remote, o.Note)
	}
}

func TestABranchThatDivergedFromOriginKeepsItsCommits(t *testing.T) {
	p, _ := pushed(t)
	commitOn(t, origin(p), "refs/heads/feat")
	git(t, p, "branch", "feat")
	local := git(t, p, "commit-tree", "-p", "feat", "-m", "mine", git(t, p, "rev-parse", "main^{tree}"))
	git(t, p, "update-ref", "refs/heads/feat", local)
	o, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != local || o.Note != "feat differs from origin/feat" {
		t.Fatalf("HEAD %s, local %s, note %q", head, local, o.Note)
	}
}

func TestABranchAlreadyOpenIsAdoptedMainIncluded(t *testing.T) {
	p := repo(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for branch, want := range map[string]string{"feat": tree, "main": p} {
		o, err := PrepareBranch(f, p, "", branch, os.Environ())
		if err != nil || o.Adopt != want || o.Path != want || o.Note != "Already open in "+filepath.Base(want) {
			t.Errorf("%s: %+v, %v", branch, o, err)
		}
	}
}

func TestADeletedWorktreeFolderNoLongerHoldsItsBranch(t *testing.T) {
	p := repo(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	os.RemoveAll(tree)
	if o, c := opened(t, p, "feat"); o.Adopt != "" || filepath.Base(c.Path) != "feat" {
		t.Fatalf("%+v", o)
	}
}

func TestOpeningABranchNobodyHasFails(t *testing.T) {
	p := repo(t)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for _, b := range []string{"nope", "-x", "a..b"} {
		if _, err := PrepareBranch(f, p, "", b, os.Environ()); err == nil || err.Error() != "No branch "+b || Code(err) != "spawn_failed" {
			t.Errorf("%s: %v", b, err)
		}
	}
}

func TestABranchOriginLacksIsNoBranch(t *testing.T) {
	p, _ := pushed(t)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	if _, err := PrepareBranch(f, p, "", "nope", os.Environ()); err == nil || err.Error() != "No branch nope" || Code(err) != "spawn_failed" {
		t.Fatal(err)
	}
}

func TestAnUnreachableOriginSaysTheFetchFailed(t *testing.T) {
	p, _ := pushed(t)
	git(t, p, "remote", "set-url", "origin", filepath.Join(t.TempDir(), "gone.git"))
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	_, err := PrepareBranch(f, p, "", "nope", os.Environ())
	if err == nil || !strings.HasPrefix(err.Error(), "Couldn't fetch nope from origin: ") || !strings.Contains(err.Error(), "gone.git") || Code(err) != "spawn_failed" {
		t.Fatal(err)
	}
}

func TestAFailedAddDeletesOnlyTheBranchItMade(t *testing.T) {
	p, _ := pushed(t)
	commitOn(t, origin(p), "refs/heads/feat")
	git(t, p, "branch", "mine")
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for _, b := range []string{"feat", "mine"} {
		o, err := PrepareBranch(f, p, "", b, os.Environ())
		if err != nil {
			t.Fatal(err)
		}
		os.MkdirAll(filepath.Dir(o.Path), 0o755)
		os.WriteFile(o.Path, nil, 0o644)
		if _, err := o.Add(); err == nil {
			t.Fatalf("%s: added over a file", b)
		}
	}
	if b := git(t, p, "branch", "--list", "feat", "mine"); b != "mine" {
		t.Fatalf("branches %q", b)
	}
}
