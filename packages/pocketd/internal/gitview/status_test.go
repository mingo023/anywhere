package gitview

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func TestStatusReadsRenamesSpacesUnicodeAndBinaries(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "old.txt", "one\ntwo\nthree\nfour\nfive\n")
	write(t, repo, "sp ace é.txt", "x\n")
	write(t, repo, "logo.bin", "\x00\x01\x02")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "files")
	sh(t, repo, "mv", "old.txt", "new.txt")
	write(t, repo, "new.txt", "one\ntwo\nthree\nfour\nfive\nsix\n")
	write(t, repo, "sp ace é.txt", "x\ny\n")
	write(t, repo, "logo.bin", "\x00\x03")
	write(t, repo, "shot.png", "png")
	write(t, repo, "dir/tab\tname.md", "t\n")
	os.Remove(filepath.Join(repo, "a"))

	st, err := service().Status(repo, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	type row struct {
		Path, Old, Status string
		Staged, Unstaged  bool
		Added, Removed    int
		Counted, Binary   bool
	}
	var got []row
	for _, f := range st.Files {
		r := row{f.Path, f.OldPath, f.Status, f.Staged, f.Unstaged, 0, 0, f.Added != nil, f.Binary}
		if f.Added != nil {
			r.Added, r.Removed = *f.Added, *f.Removed
		}
		got = append(got, r)
	}
	want := []row{
		{"a", "", "D", false, true, 0, 1, true, false},
		{"dir/tab\tname.md", "", "?", false, true, 0, 0, false, false},
		{"logo.bin", "", "M", false, true, 0, 0, false, true},
		{"new.txt", "old.txt", "R", true, true, 1, 0, true, false},
		{"shot.png", "", "?", false, true, 0, 0, false, true},
		{"sp ace é.txt", "", "M", false, true, 1, 0, true, false},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got  %+v\nwant %+v", got, want)
	}
	if st.Totals != (proto.GitTotals{Files: 6, Added: 2, Removed: 1, Staged: 1}) || st.Branch != "main" || st.Rev == "" {
		t.Fatalf("%+v", st)
	}
}

func TestStatusPagesByPathWithTheSameTotalsAndRevOnEveryPage(t *testing.T) {
	repo := scratch(t)
	for i := range 7 {
		write(t, repo, fmt.Sprintf("f%d", 6-i), "x\n")
	}
	s := service()
	var paths []string
	var revs []string
	cursor := 0
	for page := 0; ; page++ {
		st, err := s.Status(repo, cursor, 3)
		if err != nil {
			t.Fatal(err)
		}
		if st.Totals.Files != 7 {
			t.Fatalf("page %d totals %+v", page, st.Totals)
		}
		for _, f := range st.Files {
			paths = append(paths, f.Path)
		}
		revs = append(revs, st.Rev)
		if st.NextCursor == nil {
			break
		}
		cursor = *st.NextCursor
	}
	if !reflect.DeepEqual(paths, []string{"f0", "f1", "f2", "f3", "f4", "f5", "f6"}) || revs[0] != revs[1] || revs[1] != revs[2] {
		t.Fatalf("%v %v", paths, revs)
	}
}

func TestTheRevMovesWhenAnAlreadyChangedFileChangesAgain(t *testing.T) {
	repo := scratch(t)
	s := service()
	write(t, repo, "a", "b\n")
	before, _ := s.Rev(repo)
	if again, _ := s.Rev(repo); again != before {
		t.Fatal("the rev moved with nothing changed")
	}
	write(t, repo, "a", "bc\n")
	if after, _ := s.Rev(repo); after == before {
		t.Fatal("the rev stood still")
	}
	st, _ := s.Status(repo, 0, 0)
	if now, _ := s.Rev(repo); st.Rev != now {
		t.Fatalf("status rev %s, poll rev %s", st.Rev, now)
	}
}

func TestAStatusOf1500ChangedFilesIsQuick(t *testing.T) {
	repo := scratch(t)
	for i := range 1500 {
		write(t, repo, fmt.Sprintf("src/mod%02d/file %04d.rs", i%40, i), "fn a() {}\n")
	}
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "many")
	for i := range 1500 {
		write(t, repo, fmt.Sprintf("src/mod%02d/file %04d.rs", i%40, i), "fn a() {}\nfn b() {}\n")
	}
	s := service()
	start := time.Now()
	st, err := s.Status(repo, 0, 0)
	took := time.Since(start)
	if err != nil {
		t.Fatal(err)
	}
	if st.Totals.Files != 1500 || st.Totals.Added != 1500 || len(st.Files) != DefaultStatusPage || *st.NextCursor != DefaultStatusPage {
		t.Fatalf("%+v", st.Totals)
	}
	start = time.Now()
	s.Rev(repo)
	t.Logf("status of 1500 changed files: %v; poll: %v", took, time.Since(start))
	if took > 2*time.Second {
		t.Fatalf("status took %v", took)
	}
}

func TestUnpushedIsNoneBeforeTheFirstPushThenCountsCommitsPastTheUpstream(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "cap-retries")
	write(t, repo, "a", "capped\n")
	sh(t, repo, "commit", "-qam", "Cap retries")
	s := service()
	st, _ := s.Status(repo, 0, 0)
	if st.Unpushed != nil || st.Base != "main" || st.Ahead != 1 || len(st.Commits) != 1 || st.Commits[0].Subject != "Cap retries" {
		t.Fatalf("%+v", st)
	}
	sh(t, repo, "branch", "--set-upstream-to", "main")
	st, _ = s.Status(repo, 0, 0)
	if st.Unpushed == nil || *st.Unpushed != 1 || st.Upstream != "main" {
		t.Fatalf("%+v", st)
	}
}

func TestABranchCountsAheadOfOriginMainNotAStaleLocalMain(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "landed")
	sh(t, repo, "update-ref", "refs/remotes/origin/main", "HEAD")
	sh(t, repo, "reset", "-q", "--hard", "HEAD~1")
	sh(t, repo, "switch", "-qc", "feature", "--no-track", "origin/main")
	st, _ := service().Status(repo, 0, 0)
	if st.Base != "origin/main" || st.Ahead != 0 || st.Behind != 0 || st.Unpushed != nil {
		t.Fatalf("%+v", st)
	}
}

// developAheadOfMaster is a repo on master with origin's develop one commit past it.
func developAheadOfMaster(t *testing.T) string {
	repo := scratch(t)
	sh(t, repo, "branch", "-M", "master")
	sh(t, repo, "update-ref", "refs/remotes/origin/master", "HEAD")
	sh(t, repo, "commit", "-q", "--allow-empty", "-m", "developed")
	sh(t, repo, "update-ref", "refs/remotes/origin/develop", "HEAD")
	return repo
}

func TestABranchCountsAgainstTheBranchItWasCreatedFrom(t *testing.T) {
	repo := developAheadOfMaster(t)
	sh(t, repo, "switch", "-qc", "feature", "--no-track", "origin/develop")
	st, _ := service().Status(repo, 0, 0)
	if st.Base != "origin/develop" || st.Ahead != 0 || st.Behind != 0 {
		t.Fatalf("%+v", st)
	}
}

func TestABranchWithNoRecordOfItsStartCountsAgainstOriginsDefaultBranch(t *testing.T) {
	repo := developAheadOfMaster(t)
	sh(t, repo, "symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/develop")
	sh(t, repo, "branch", "feature", "origin/develop")
	os.Remove(filepath.Join(repo, ".git", "logs", "refs", "heads", "feature"))
	sh(t, repo, "switch", "-q", "feature")
	st, _ := service().Status(repo, 0, 0)
	if st.Base != "origin/develop" || st.Ahead != 0 || st.Behind != 0 {
		t.Fatalf("%+v", st)
	}
}

func TestCreatedFromNamesTheBranchWithoutItsRefOrRemote(t *testing.T) {
	for entry, want := range map[string]string{
		"branch: Created from origin/develop":              "develop",
		"branch: Created from refs/remotes/origin/develop": "develop",
		"branch: Created from main":                        "main",
		"branch: Created from HEAD":                        "",
		"commit (initial): init":                           "",
	} {
		if got, _ := createdFrom(entry); got != want && !(want == "" && got == "HEAD") {
			t.Errorf("%q: %q", entry, got)
		}
		if _, ok := createdFrom(entry); ok != (want != "") {
			t.Errorf("%q: ok %v", entry, ok)
		}
	}
}

func TestARepositoryWithoutCommitsReadsAsOneOnItsBranch(t *testing.T) {
	dir := t.TempDir()
	sh(t, dir, "init", "-q", "-b", "trunk")
	write(t, dir, "a.txt", "a\n")
	st, err := service().Status(dir, 0, 0)
	if err != nil || st.Branch != "trunk" || len(st.Files) != 1 || st.Files[0].Status != "?" {
		t.Fatalf("%+v %v", st, err)
	}
	sh(t, dir, "add", ".")
	st, _ = service().Status(dir, 0, 0)
	if f := st.Files[0]; f.Status != "A" || !f.Staged || f.Added == nil || *f.Added != 1 {
		t.Fatalf("%+v", st.Files)
	}
}

func TestADetachedHeadHasNoBranch(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-q", "--detach")
	st, _ := service().Status(repo, 0, 0)
	if !st.Detached || st.Branch != "" || st.Base != "" {
		t.Fatalf("%+v", st)
	}
}

func TestAConflictReadsAsUAndUnstaged(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "other")
	write(t, repo, "a", "theirs\n")
	sh(t, repo, "commit", "-qam", "theirs")
	sh(t, repo, "checkout", "-q", "main")
	write(t, repo, "a", "ours\n")
	sh(t, repo, "commit", "-qam", "ours")
	cmd := []string{"merge", "-q", "other"}
	service().git(repo, nil, readTimeout, cmd, 1)
	f, ok := file(t, service(), repo, "a")
	if !ok || f.Status != "U" || f.Staged || !f.Unstaged {
		t.Fatalf("%+v", f)
	}
}

func TestNotARepositoryFailsWithGitsMessage(t *testing.T) {
	_, err := service().Status(t.TempDir(), 0, 0)
	if f, ok := err.(*Failed); !ok || f.Message == "" {
		t.Fatalf("%v", err)
	}
}
