package gitview

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestStagesUnstagesAndDiscards(t *testing.T) {
	repo := scratch(t)
	s := service()
	write(t, repo, "a", "edited\n")
	write(t, repo, "new", "fresh\n")
	flags := func(path string) [2]bool {
		f, _ := file(t, s, repo, path)
		return [2]bool{f.Staged, f.Unstaged}
	}
	if flags("a") != [2]bool{false, true} || flags("new") != [2]bool{false, true} {
		t.Fatal(flags("a"), flags("new"))
	}
	s.Stage(repo, []string{"a", "new"}, true)
	if flags("a") != [2]bool{true, false} || flags("new") != [2]bool{true, false} {
		t.Fatal(flags("a"), flags("new"))
	}
	write(t, repo, "a", "edited twice\n")
	if flags("a") != [2]bool{true, true} {
		t.Fatal(flags("a"))
	}
	s.Discard(repo, []string{"a"})
	if b, _ := os.ReadFile(filepath.Join(repo, "a")); string(b) != "edited\n" {
		t.Fatalf("%q", b)
	}
	s.Stage(repo, []string{"a", "new"}, false)
	if flags("a") != [2]bool{false, true} {
		t.Fatal(flags("a"))
	}
	s.Discard(repo, []string{"a", "new"})
	if st, _ := s.Status(repo, 0, 0); len(st.Files) != 0 {
		t.Fatalf("%+v", st.Files)
	}
}

func TestUnstagingBeforeTheFirstCommitLeavesTheFileUntracked(t *testing.T) {
	dir := t.TempDir()
	sh(t, dir, "init", "-q")
	write(t, dir, "a", "a\n")
	s := service()
	s.Stage(dir, []string{"a"}, true)
	if err := s.Stage(dir, []string{"a"}, false); err != nil {
		t.Fatal(err)
	}
	if f, _ := file(t, s, dir, "a"); f.Status != "?" {
		t.Fatalf("%+v", f)
	}
}

func subjects(t *testing.T, repo string) []string {
	return strings.Split(sh(t, repo, "log", "--format=%s"), "\n")
}

func TestACommitTakesTheStagedFilesWithTheMessageFromStdin(t *testing.T) {
	repo := scratch(t)
	s := service()
	write(t, repo, "a", "edited\n")
	write(t, repo, "b", "not staged\n")
	s.Stage(repo, []string{"a"}, true)
	sha, err := s.CreateCommit(repo, "Edit a\n\nWith a body; $(not run) `either`", false, false)
	if err != nil || sha != sh(t, repo, "rev-parse", "HEAD") {
		t.Fatal(sha, err)
	}
	if !reflect.DeepEqual(subjects(t, repo), []string{"Edit a", "init"}) || !strings.Contains(sh(t, repo, "log", "-1", "--format=%b"), "$(not run)") {
		t.Fatal(subjects(t, repo))
	}
	if f, _ := file(t, s, repo, "b"); f.Status != "?" {
		t.Fatalf("%+v", f)
	}
}

func TestAmendingWithoutAMessageKeepsTheLastOneAndWithOneReplacesIt(t *testing.T) {
	repo := scratch(t)
	s := service()
	write(t, repo, "a", "edited\n")
	s.Stage(repo, []string{"a"}, true)
	if _, err := s.CreateCommit(repo, "", true, false); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(subjects(t, repo), []string{"init"}) {
		t.Fatal(subjects(t, repo))
	}
	s.CreateCommit(repo, "Start", true, false)
	if !reflect.DeepEqual(subjects(t, repo), []string{"Start"}) {
		t.Fatal(subjects(t, repo))
	}
}

func TestCommittingAllStagesEveryChangeOnlyWhenNoneIsStaged(t *testing.T) {
	repo := scratch(t)
	s := service()
	write(t, repo, "a", "edited\n")
	write(t, repo, "new", "fresh\n")
	if _, err := s.CreateCommit(repo, "All", false, false); err == nil {
		t.Fatal("committed with nothing staged")
	}
	s.CreateCommit(repo, "All", false, true)
	if st, _ := s.Status(repo, 0, 0); len(st.Files) != 0 {
		t.Fatalf("%+v", st.Files)
	}
	write(t, repo, "a", "again\n")
	write(t, repo, "b", "loose\n")
	s.Stage(repo, []string{"a"}, true)
	s.CreateCommit(repo, "Only a", false, true)
	if f, ok := file(t, s, repo, "b"); !ok || f.Status != "?" {
		t.Fatalf("%+v", f)
	}
	if _, err := s.CreateCommit(repo, "  ", false, true); err == nil {
		t.Fatal("committed without a message")
	}
}

func TestAFailedCommitSaysWhatTheHookSaid(t *testing.T) {
	repo := scratch(t)
	hook := filepath.Join(repo, ".git", "hooks", "pre-commit")
	os.WriteFile(hook, []byte("#!/bin/sh\necho 'hint: x' >&2\necho 'error: lint failed' >&2\nexit 1\n"), 0o755)
	write(t, repo, "a", "edited\n")
	_, err := service().CreateCommit(repo, "Edit", false, true)
	f, ok := err.(*Failed)
	if !ok || f.Message != "error: lint failed" || !strings.Contains(f.Detail, "hint: x") {
		t.Fatalf("%#v", err)
	}
}

func bare(t *testing.T, repo string) string {
	remote := filepath.Join(filepath.Dir(repo), "remote.git")
	sh(t, repo, "init", "-q", "--bare", "-b", "main", remote)
	sh(t, repo, "remote", "add", "origin", remote)
	return remote
}

func TestTheFirstPushSetsTheUpstream(t *testing.T) {
	repo := scratch(t)
	bare(t, repo)
	sh(t, repo, "switch", "-qc", "feature")
	if err := service().Push(repo, false); err != nil {
		t.Fatal(err)
	}
	if u := sh(t, repo, "rev-parse", "--abbrev-ref", "@{u}"); u != "origin/feature" {
		t.Fatal(u)
	}
}

func TestARewrittenBranchPushesWithLeaseButNeverOverCommitsNotYetFetched(t *testing.T) {
	repo := scratch(t)
	remote := bare(t, repo)
	s := service()
	if err := s.Push(repo, true); err != nil {
		t.Fatal(err)
	}
	sh(t, repo, "commit", "-q", "--amend", "-m", "amended")
	if err := s.Push(repo, false); err == nil {
		t.Fatal("pushed over the remote without a lease")
	} else if f := err.(*Failed); !strings.HasPrefix(f.Message, "!") && !strings.HasPrefix(f.Message, "error:") {
		t.Fatalf("%q", f.Message)
	}
	if err := s.Push(repo, true); err != nil {
		t.Fatal(err)
	}
	other := filepath.Join(filepath.Dir(repo), "other")
	sh(t, repo, "clone", "-q", remote, other)
	sh(t, other, "commit", "-q", "--allow-empty", "-m", "theirs")
	sh(t, other, "push", "-q")
	sh(t, repo, "commit", "-q", "--amend", "-m", "again")
	if err := s.Push(repo, true); err == nil {
		t.Fatal("the lease let a push over unseen commits")
	}
	sh(t, repo, "fetch", "-q")
	if err := s.Push(repo, true); err == nil {
		t.Fatal("fetched but not integrated")
	}
}

func TestPullOnlyFastForwards(t *testing.T) {
	repo := scratch(t)
	remote := bare(t, repo)
	s := service()
	s.Push(repo, false)
	other := filepath.Join(filepath.Dir(repo), "other")
	sh(t, repo, "clone", "-q", remote, other)
	sh(t, other, "commit", "-q", "--allow-empty", "-m", "theirs")
	sh(t, other, "push", "-q")
	if err := s.Pull(repo); err != nil || subjects(t, repo)[0] != "theirs" {
		t.Fatal(err, subjects(t, repo))
	}
	sh(t, other, "commit", "-q", "--allow-empty", "-m", "theirs again")
	sh(t, other, "push", "-q")
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "mine")
	if err := s.Pull(repo); err == nil {
		t.Fatal("pulled a diverged branch")
	}
}

func TestWritesToOneRepositoryRunOneAtATime(t *testing.T) {
	repo := scratch(t)
	s := service()
	var wg sync.WaitGroup
	errs := make(chan error, 20)
	for i := range 20 {
		name := string(rune('b' + i))
		write(t, repo, name, name)
		wg.Go(func() { errs <- s.Stage(repo, []string{name}, true) })
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}
	if st, _ := s.Status(repo, 0, 0); st.Totals.Staged != 20 {
		t.Fatalf("%+v", st.Totals)
	}
}

func TestAWatcherHearsWhenItsTreeMovesAndPollingStopsWithTheLastWatcher(t *testing.T) {
	repo := scratch(t)
	s := service()
	got := make(chan string, 8)
	stop := s.Watch([]string{repo}, func(tree, rev string) { got <- tree })
	stop2 := s.Watch([]string{repo}, func(string, string) {})
	if s.Watching() != 1 {
		t.Fatal(s.Watching())
	}
	time.Sleep(60 * time.Millisecond)
	write(t, repo, "a", "moved\n")
	select {
	case tree := <-got:
		if tree != repo {
			t.Fatal(tree)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no git.changed")
	}
	s.Stage(repo, []string{"a"}, true)
	select {
	case <-got:
	case <-time.After(5 * time.Second):
		t.Fatal("a write didn't tell the watcher")
	}
	stop()
	stop()
	if s.Watching() != 1 {
		t.Fatal(s.Watching())
	}
	stop2()
	if s.Watching() != 0 {
		t.Fatal(s.Watching())
	}
}
