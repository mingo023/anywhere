//go:build unix

package github

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

// fake is a gh on PATH that answers from the files in dir (testdata/fakegh/gh).
type fake struct {
	t      *testing.T
	dir    string
	tokens atomic.Int64
}

func newFake(t *testing.T) *fake {
	f := &fake{t: t, dir: t.TempDir()}
	f.put("version", "gh version 2.81.0 (2025-10-01)\n")
	f.put("auth", "github.com\n  ✓ Logged in to github.com account mingo023 (keyring)\n  - Active account: true\n")
	return f
}

func (f *fake) put(name, content string) {
	f.t.Helper()
	if err := os.WriteFile(filepath.Join(f.dir, name), []byte(content), 0o644); err != nil {
		f.t.Fatal(err)
	}
}

func (f *fake) drop(name string) { os.Remove(filepath.Join(f.dir, name)) }

func (f *fake) calls() []string {
	raw, _ := os.ReadFile(filepath.Join(f.dir, "calls"))
	return strings.Split(strings.TrimSpace(string(raw)), "\n")
}

func (f *fake) called(prefix string) bool {
	for _, c := range f.calls() {
		if _, args, _ := strings.Cut(c, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

// env hands each run a new token, as daemon.GhEnv asks gh for one each time.
func (f *fake) env(withGh bool) Env {
	bin, _ := filepath.Abs(filepath.Join("testdata", "fakegh"))
	path := "/usr/bin:/bin"
	if withGh {
		path = bin + ":" + path
	}
	return func(dir string) []string {
		return []string{"PATH=" + path, "FAKEGH=" + f.dir, "HOME=" + f.dir, "GIT_CONFIG_GLOBAL=/dev/null", fmt.Sprintf("GH_TOKEN=tok%d", f.tokens.Add(1))}
	}
}

func git(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.name=T", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

// repo is a project on main with a worktree on branch fix, and a remote both push to.
func repo(t *testing.T, remote bool) (project, tree string, reg *registry.Registry) {
	// git names worktrees by their real path, /private/var rather than /var.
	root, _ := filepath.EvalSymlinks(t.TempDir())
	project, tree = filepath.Join(root, "app"), filepath.Join(root, "app-fix")
	os.Mkdir(project, 0o755)
	git(t, project, "init", "-q")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "first")
	if remote {
		git(t, root, "init", "-q", "--bare", "origin.git")
		git(t, project, "remote", "add", "origin", filepath.Join(root, "origin.git"))
	}
	git(t, project, "worktree", "add", "-q", "-b", "fix", tree)
	git(t, tree, "commit", "-q", "--allow-empty", "-m", "Fix it")
	path := filepath.Join(root, "desktop.json")
	os.WriteFile(path, []byte(fmt.Sprintf(`{"projects":[%q]}`, project)), 0o600)
	return project, tree, registry.New(path)
}

func ctx(t *testing.T) context.Context {
	c, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancel)
	return c
}

func TestATreesPRIsAskedOfGhThereWithItsThreads(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "cli.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	pr, problem := s.PR(ctx(t), tree)
	if problem != "" || pr == nil || pr.Number != 14580 || pr.Gate != "checksRunning" || len(pr.Threads) != 2 {
		t.Fatalf("%q %+v", problem, pr)
	}
	if !f.called("pr view --json number,title") || !f.called("api graphql -F owner={owner}") {
		t.Fatal(f.calls())
	}
}

func TestAClosedPRIsNotAskedForItsThreads(t *testing.T) {
	f := newFake(t)
	f.put("view.json", `{"isDraft":false,"number":7,"state":"MERGED","title":"t","url":"u","statusCheckRollup":[]}`)
	_, tree, reg := repo(t, false)
	pr, _ := New(f.env(true), reg).PR(ctx(t), tree)
	if pr == nil || pr.Gate != "done" || f.called("api graphql") {
		t.Fatalf("%+v %v", pr, f.calls())
	}
}

func TestThreadsThatFailToLoadKeepTheOnesShown(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "cli.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	s.PR(ctx(t), tree)
	f.put("threads.json", `{"errors":[{"message":"Something went wrong"}]}`)
	s.mu.Lock()
	s.cache.expire(tree, s.now())
	s.mu.Unlock()
	if pr, _ := s.PR(ctx(t), tree); len(pr.Threads) != 2 {
		t.Fatalf("%+v", pr.Threads)
	}
}

func TestAWatchedTreesPRIsServedFromTheCacheWhileFresh(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	s.PR(ctx(t), tree)
	before := len(f.calls())
	if pr, _ := s.PR(ctx(t), tree); pr.Number != 42 || len(f.calls()) != before {
		t.Fatalf("gh asked again: %v", f.calls())
	}
}

func TestATokenGitHubRefusesIsAskedForAgainOnce(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	f.put("unauthorized", "")
	_, tree, reg := repo(t, false)
	pr, problem := New(f.env(true), reg).PR(ctx(t), tree)
	calls := f.calls()
	if pr == nil || problem != "" || !strings.HasPrefix(calls[0], "GH_TOKEN=tok1 pr view") || !strings.HasPrefix(calls[1], "GH_TOKEN=tok2 pr view") {
		t.Fatalf("%+v %v", pr, calls)
	}
}

func TestWithoutGhOnThePathGhIsMissing(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, false)
	s := New(f.env(false), reg)
	if st := s.Status(ctx(t), ""); st.State != "missing" {
		t.Fatalf("%+v", st)
	}
	if pr, problem := s.PR(ctx(t), tree); pr != nil || problem != Missing {
		t.Fatalf("%+v %q", pr, problem)
	}
	var e *Error
	if err := s.Ready(ctx(t), tree, 7); !errors.As(err, &e) || e.Code != proto.CodeGhMissing {
		t.Fatalf("%v", err)
	}
}

func TestASignedOutGhSaysSo(t *testing.T) {
	f := newFake(t)
	f.put("auth", "You are not logged into any GitHub hosts. To log in, run: gh auth login\n")
	f.put("auth.fail", "")
	f.put("view.err", "To get started with GitHub CLI, please run:  gh auth login\n")
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	if st := s.Status(ctx(t), tree); st.State != "signedOut" || st.Version != "2.81.0" {
		t.Fatalf("%+v", st)
	}
	if _, problem := s.PR(ctx(t), tree); problem != SignedOut {
		t.Fatalf("%q", problem)
	}
	f.put("fail", "To get started with GitHub CLI, please run:  gh auth login\n")
	var e *Error
	if err := s.Merge(ctx(t), tree, 7, "squash"); !errors.As(err, &e) || e.Code != proto.CodeGhSignedOut {
		t.Fatalf("%v", err)
	}
}

func TestStatusNamesTheAccountGhIsSignedInAs(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, false)
	if st := New(f.env(true), reg).Status(ctx(t), tree); st.State != "ready" || st.Login != "mingo023" {
		t.Fatalf("%+v", st)
	}
}

func TestCreatePushesTheBranchThenOpensItsPR(t *testing.T) {
	f := newFake(t)
	f.put("created", "Warning: 1 uncommitted change\nhttps://github.com/acme/app/pull/57\n")
	project, tree, reg := repo(t, true)
	s := New(f.env(true), reg)
	n, url, err := s.Create(ctx(t), tree, "Fix it", "Closes #12\n", "origin/main", true)
	if err != nil || n != 57 || url != "https://github.com/acme/app/pull/57" {
		t.Fatal(n, url, err)
	}
	if got := git(t, project, "ls-remote", "origin", "refs/heads/fix"); got == "" {
		t.Fatal("fix wasn't pushed")
	}
	if body, _ := os.ReadFile(filepath.Join(f.dir, "body")); string(body) != "Closes #12\n" || !f.called("pr create --base main --title Fix it --body-file - --draft") {
		t.Fatalf("%q %v", body, f.calls())
	}
	items := s.PRs(project)
	if len(items) != 1 || items[0].Tree != tree || items[0].Number != 57 || items[0].Gate != "draft" {
		t.Fatalf("%+v", items)
	}
}

func TestAFailedPushOpensNoPR(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, false)
	var e *Error
	if _, _, err := New(f.env(true), reg).Create(ctx(t), tree, "Fix it", "", "", false); !errors.As(err, &e) || e.Code != proto.CodeGitFailed || e.Message == "" {
		t.Fatalf("%v", err)
	}
	if f.called("pr create") {
		t.Fatal(f.calls())
	}
}

func TestMergingNeverDeletesTheBranchAndShowsThePRMergedAtOnce(t *testing.T) {
	f := newFake(t)
	f.put("view.json", `{"isDraft":false,"number":7,"state":"OPEN","title":"t","url":"u","mergeable":"MERGEABLE","statusCheckRollup":[]}`)
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	s.PR(ctx(t), tree)
	if err := s.Merge(ctx(t), tree, 7, "rebase"); err != nil {
		t.Fatal(err)
	}
	if !f.called("pr merge 7 --rebase") || strings.Contains(strings.Join(f.calls(), "\n"), "--delete-branch") {
		t.Fatal(f.calls())
	}
	if pr, _ := s.cache.get(tree); pr.State != "MERGED" {
		t.Fatal(pr.State)
	}
}

func TestResolvingAThreadShowsItResolvedAtOnce(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	before, _ := s.PR(ctx(t), tree)
	if err := s.Resolve(ctx(t), tree, "PRRT_a", true); err != nil {
		t.Fatal(err)
	}
	if !f.called("api graphql -f id=PRRT_a -f query=mutation($id:ID!){resolveReviewThread") {
		t.Fatal(f.calls())
	}
	if pr, _ := s.cache.get(tree); !pr.Threads[0].Resolved || before.Threads[0].Resolved {
		t.Fatal("not resolved, or the answer already sent changed")
	}
}

func TestAFailingVerbCarriesGhsFirstLine(t *testing.T) {
	f := newFake(t)
	f.put("fail", "GraphQL: Resource not accessible by integration (addPullRequestReviewThreadReply)\nmore\n")
	_, tree, reg := repo(t, false)
	var e *Error
	err := New(f.env(true), reg).Reply(ctx(t), tree, "PRRT_a", "Done")
	if !errors.As(err, &e) || e.Code != proto.CodeGhFailed || e.Message != "GraphQL: Resource not accessible by integration (addPullRequestReviewThreadReply)" || !strings.HasSuffix(e.Detail, "more") {
		t.Fatalf("%#v", err)
	}
}

// clock is a now a test moves.
type clock struct {
	mu sync.Mutex
	at time.Time
}

func (c *clock) now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.at
}

func (c *clock) add(d time.Duration) {
	c.mu.Lock()
	c.at = c.at.Add(d)
	c.mu.Unlock()
}

func TestThePollerTellsAWatcherWhenItsTreesPRChanges(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	c := &clock{at: time.Now()}
	s.now = c.now
	changed := make(chan string, 8)
	s.Listen("k", func(tree string) { changed <- tree })
	t.Cleanup(func() { s.Leave("k") })
	s.Watch("k", []string{tree})
	wait := func() string {
		select {
		case got := <-changed:
			return got
		case <-time.After(10 * time.Second):
			t.Fatal("no change told")
		}
		return ""
	}
	if got := wait(); got != tree {
		t.Fatal(got)
	}
	f.put("view.json", fixture(t, "rerun.json"))
	c.add(WatchedEvery)
	s.poke()
	if got := wait(); got != tree {
		t.Fatal(got)
	}
	if pr, _ := s.PR(ctx(t), tree); pr.Number != 53 {
		t.Fatal(pr.Number)
	}
}

func TestThePollerAsksNothingOnceTheLastPhoneLeaves(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	_, _, reg := repo(t, false)
	s := New(f.env(true), reg)
	s.Listen("a", func(string) {})
	s.Listen("b", func(string) {})
	s.Leave("a")
	if s.cancel == nil {
		t.Fatal("stopped with a phone left")
	}
	s.Leave("b")
	if s.cancel != nil {
		t.Fatal("still polling")
	}
}

func TestAPhoneLeavingMidAskLeavesTheShownPRAlone(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	project, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	if pr, _ := s.PR(ctx(t), tree); pr == nil {
		t.Fatal("no PR")
	}
	s.mu.Lock()
	s.cache.expire(tree, s.now())
	s.mu.Unlock()
	gone, cancel := context.WithCancel(context.Background())
	cancel()
	s.PR(gone, tree)
	s.poll(gone)
	if items := s.PRs(project); len(items) != 1 {
		t.Fatalf("%+v", items)
	}
	if s.cache.running != "" {
		t.Fatal("the next poller would wait out the killed run's turn")
	}
}

func TestAPhoneSlowToTakeAChangeDoesNotHoldTheOthers(t *testing.T) {
	f := newFake(t)
	f.put("view.json", fixture(t, "open.json"))
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	stuck := make(chan struct{})
	t.Cleanup(func() { close(stuck) })
	changed := make(chan string, 8)
	s.Listen("slow", func(string) { <-stuck })
	s.Listen("k", func(tree string) { changed <- tree })
	t.Cleanup(func() { s.Leave("slow"); s.Leave("k") })
	s.Watch("slow", []string{tree})
	s.Watch("k", []string{tree})
	select {
	case <-changed:
	case <-time.After(10 * time.Second):
		t.Fatal("no change told")
	}
}
