package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"pocketd/internal/registry"
)

func TestParseViewReadsWhatGhPrints(t *testing.T) {
	out := `{"headRefName":"andyfeller/flag-level-disableauth","headRefOid":"cc36d32a212a2b8b6611fb73549fe6d04fb6ec38","isCrossRepository":false,"number":9000,"state":"MERGED","url":"https://github.com/cli/cli/pull/9000"}`
	pr, err := parseView([]byte(out))
	want := PR{Number: 9000, URL: "https://github.com/cli/cli/pull/9000", Head: "andyfeller/flag-level-disableauth", OID: "cc36d32a212a2b8b6611fb73549fe6d04fb6ec38", State: "MERGED"}
	if err != nil || pr != want {
		t.Fatalf("%+v, %v", pr, err)
	}
	if _, err := parseView([]byte(`{}`)); Code(err) != "spawn_failed" {
		t.Fatal(err)
	}
}

func TestGhFailuresSayWhatToDo(t *testing.T) {
	for stderr, want := range map[string]string{
		"To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.\n": "Run gh auth login to open pull requests",
		"GraphQL: Could not resolve to a PullRequest with the number of 99999. (repository.pullRequest)\n":                                                                "No pull request #99999",
		`no pull requests found for branch "#99999"` + "\n":                                                                                                               "No pull request #99999",
		"HTTP 502: Bad Gateway\nTry again\n": "HTTP 502: Bad Gateway",
		"":                                   "gh couldn't open pull request #99999",
	} {
		if err := ghFailure("#99999", stderr); err.Error() != want || Code(err) != "spawn_failed" {
			t.Errorf("%q: %v", stderr, err)
		}
	}
}

// pulled is a repo whose origin is a bare acme/pocket.git, where PR 7 points
// at a new commit on feat.
func pulled(t *testing.T) (p, bare string, pr PR) {
	t.Helper()
	p = repo(t)
	bare = filepath.Join(filepath.Dir(p), "acme", "pocket.git")
	git(t, p, "clone", "-q", "--bare", p, bare)
	git(t, p, "remote", "add", "origin", bare)
	oid := commitOn(t, bare, "refs/heads/feat")
	git(t, bare, "update-ref", "refs/pull/7/head", oid)
	return p, bare, PR{Number: 7, URL: "https://github.com/acme/pocket/pull/7", Head: "feat", OID: oid, State: "OPEN"}
}

func preparedPR(t *testing.T, p string, pr PR) (*Open, error) {
	t.Helper()
	return PreparePR(registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")}), p, "", pr, os.Environ())
}

func openedPR(t *testing.T, p string, pr PR) (*Open, Created) {
	t.Helper()
	o, err := preparedPR(t, p, pr)
	if err != nil {
		t.Fatal(err)
	}
	c, err := o.Add()
	if err != nil {
		t.Fatal(err)
	}
	return o, c
}

func TestASameRepoPRTracksItsHeadBranch(t *testing.T) {
	p, _, pr := pulled(t)
	o, c := openedPR(t, p, pr)
	if filepath.Base(c.Path) != "feat" || git(t, c.Path, "rev-parse", "HEAD") != pr.OID || o.Note != "" {
		t.Fatalf("%+v, note %q", c, o.Note)
	}
	if up := git(t, c.Path, "rev-parse", "--abbrev-ref", "@{upstream}"); up != "origin/feat" {
		t.Fatalf("upstream %q", up)
	}
}

func TestAForkPRLandsOnPrNTrackingThePullRef(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "-d", "refs/heads/feat")
	pr.Fork = true
	_, c := openedPR(t, p, pr)
	if filepath.Base(c.Path) != "pr-7" || git(t, c.Path, "branch", "--show-current") != "pr/7" || git(t, c.Path, "rev-parse", "HEAD") != pr.OID {
		t.Fatalf("%+v", c)
	}
	if m := git(t, p, "config", "branch.pr/7.merge"); m != "refs/pull/7/head" {
		t.Fatalf("merge %q", m)
	}
	if exec.Command("git", "-C", p, "rev-parse", "-q", "--verify", "refs/pocket/pr/7").Run() == nil {
		t.Fatal("the fetched pull ref was left behind")
	}
}

func TestAPRWhoseHeadBranchIsGoneOpensFromThePullRef(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "-d", "refs/heads/feat")
	_, c := openedPR(t, p, pr)
	if git(t, c.Path, "rev-parse", "HEAD") != pr.OID || git(t, p, "config", "branch.feat.merge") != "refs/pull/7/head" {
		t.Fatalf("%+v", c)
	}
}

func TestAPRThatMovedWhileOpeningIsRefused(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "refs/pull/7/head", commitOn(t, bare, "refs/heads/feat"))
	if _, err := preparedPR(t, p, pr); err == nil || err.Error() != "PR #7 changed while opening, try again" {
		t.Fatal(err)
	}
}

func TestALocalBranchWithCommitsOffThePRIsRefused(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "commit", "-q", "--allow-empty", "-m", "mine")
	git(t, p, "branch", "feat")
	if _, err := preparedPR(t, p, pr); err == nil || err.Error() != "Branch feat has commits not in PR #7" {
		t.Fatal(err)
	}
}

func TestAMergedPROpensWithANote(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "branch", "feat")
	pr.State = "MERGED"
	if o, _ := openedPR(t, p, pr); o.Note != "Fast-forwarded feat; PR #7 is merged" {
		t.Fatalf("note %q", o.Note)
	}
}

func TestAPRAlreadyOpenOffItsHeadSaysSo(t *testing.T) {
	p, _, pr := pulled(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	o, err := preparedPR(t, p, pr)
	if err != nil || o.Adopt != tree || o.Note != "Already open in feat; feat isn't at PR #7's head" {
		t.Fatalf("%+v, %v", o, err)
	}
}

func TestAFailedPROpenDeletesOnlyTheBranchItMade(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "branch", "feat")
	fork := pr
	fork.Fork = true
	for _, pr := range []PR{pr, fork} {
		o, err := preparedPR(t, p, pr)
		if err != nil {
			t.Fatal(err)
		}
		os.MkdirAll(filepath.Dir(o.Path), 0o755)
		os.WriteFile(o.Path, nil, 0o644)
		if _, err := o.Add(); err == nil {
			t.Fatalf("%s: added over a file", o.Branch)
		}
	}
	if b := git(t, p, "branch", "--list", "feat", "pr/7"); b != "feat" {
		t.Fatalf("branches %q", b)
	}
}

func TestThePRsRemoteIsTheOneWithItsRepo(t *testing.T) {
	p := repo(t)
	git(t, p, "remote", "add", "up", "git@github.com:Acme/Pocket.git/")
	o := &Open{Project: p, env: os.Environ()}
	if r, err := o.remoteFor("https://github.com/acme/pocket/pull/7"); r != "up" || err != nil {
		t.Fatalf("%q, %v", r, err)
	}
	if _, err := o.remoteFor("https://github.com/other/thing/pull/7"); err == nil || err.Error() != "No remote points at other/thing" {
		t.Fatal(err)
	}
}
