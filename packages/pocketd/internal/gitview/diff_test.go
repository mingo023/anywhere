package gitview

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

const patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -45,4 +45,5 @@ fn a() {\n ctx\n-old\n+new1\n+new2\n ctx2\n\\ No newline at end of file\n"

func TestParsesHunksWithLineNumbers(t *testing.T) {
	rows, binary := parsePatch(patch)
	want := []proto.DiffRow{
		{K: "h", T: "@@ -45,4 +45,5 @@ fn a() {"},
		{K: "c", O: 45, N: 45, T: "ctx"},
		{K: "d", O: 46, T: "old"},
		{K: "a", N: 46, T: "new1"},
		{K: "a", N: 47, T: "new2"},
		{K: "c", O: 47, N: 48, T: "ctx2"},
	}
	if binary || !reflect.DeepEqual(rows, want) {
		t.Fatalf("%+v", rows)
	}
}

func TestDiffsWordByWord(t *testing.T) {
	for _, c := range []struct {
		old, new string
		del, add [][2]int
	}{
		{"let a = 1;", "let b = 1;", [][2]int{{4, 5}}, [][2]int{{4, 5}}},
		{"foo(bar)", "foo(bar, baz)", nil, [][2]int{{7, 12}}},
		{"é = 1", "é = 2", [][2]int{{5, 6}}, [][2]int{{5, 6}}},
		{"same", "same", nil, nil},
		{"", "all new", nil, [][2]int{{0, 7}}},
	} {
		del, add := Words(c.old, c.new)
		if !reflect.DeepEqual(del, c.del) || !reflect.DeepEqual(add, c.add) {
			t.Errorf("%q → %q: %v %v", c.old, c.new, del, add)
		}
	}
}

func TestWordsGoOnlyOnEqualRunsOfAtMostFiveLines(t *testing.T) {
	rows := func(dels, adds int) []proto.DiffRow {
		var out []proto.DiffRow
		for range dels {
			out = append(out, proto.DiffRow{K: "d", T: "let a = 1;"})
		}
		for range adds {
			out = append(out, proto.DiffRow{K: "a", T: "let b = 1;"})
		}
		return out
	}
	for _, c := range []struct {
		dels, adds int
		words      bool
	}{{1, 1, true}, {5, 5, true}, {6, 6, false}, {2, 1, false}, {1, 2, false}} {
		r := rows(c.dels, c.adds)
		addWords(r)
		if got := r[0].W != nil; got != c.words {
			t.Errorf("%d/%d: %v", c.dels, c.adds, got)
		}
	}
}

func lines(d proto.GitDiff) []string {
	var out []string
	for _, l := range d.Lines {
		out = append(out, l.K+" "+l.T)
	}
	return out
}

func TestAWorkingDiffComparesHeadWithTheFileAndMarksChangedWords(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "src/main.rs", "fn main() {\n    println!(\"hi\");\n}\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "main")
	write(t, repo, "src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n")
	d, err := service().Diff(repo, "src/main.rs", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if d.Language != "rust" || d.Added != 1 || d.Removed != 1 || len(d.Lines) != 5 {
		t.Fatalf("%+v", d)
	}
	del, add := d.Lines[2], d.Lines[3]
	if del.K != "d" || del.O != 2 || add.K != "a" || add.N != 2 {
		t.Fatalf("%+v", d.Lines)
	}
	if got := add.T[add.W[0][0]:add.W[0][1]]; got != "hello" || del.T[del.W[0][0]:del.W[0][1]] != "hi" {
		t.Fatalf("%v %v", del.W, add.W)
	}
}

func TestAnUntrackedFileDiffsAsAllAdded(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "new dir/ü.go", "package a\n\nfunc A() {}\n")
	d, err := service().Diff(repo, "new dir/ü.go", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if d.Language != "go" || d.Added != 3 || d.Removed != 0 || d.Lines[1].N != 1 || d.Lines[3].N != 3 {
		t.Fatalf("%+v", d)
	}
}

func TestAStagedDiffShowsOnlyWhatIsStaged(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "a", "staged\n")
	sh(t, repo, "add", "a")
	write(t, repo, "a", "staged and more\n")
	d, _ := service().Diff(repo, "a", proto.DiffAt{Kind: "staged"}, DiffOptions{})
	if !reflect.DeepEqual(lines(d)[1:], []string{"d a", "a staged"}) {
		t.Fatalf("%v", lines(d))
	}
	d, _ = service().Diff(repo, "a", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if !reflect.DeepEqual(lines(d)[1:], []string{"d a", "a staged and more"}) {
		t.Fatalf("%v", lines(d))
	}
}

func TestARenameDiffsAgainstItsOldNameInTheWorkingTreeAndInACommit(t *testing.T) {
	repo := scratch(t)
	body := "one\ntwo\nthree\nfour\nfive\nsix\n"
	write(t, repo, "old.txt", body)
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "old")
	sh(t, repo, "mv", "old.txt", "new.txt")
	write(t, repo, "new.txt", body+"seven\n")
	s := service()
	d, err := s.Diff(repo, "new.txt", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if err != nil || d.OldPath != "old.txt" || d.Added != 1 || d.Removed != 0 {
		t.Fatalf("%+v %v", d, err)
	}
	sh(t, repo, "commit", "-qam", "renamed")
	d, err = s.Diff(repo, "new.txt", proto.DiffAt{Kind: "commit", Rev: "HEAD"}, DiffOptions{})
	if err != nil || d.OldPath != "old.txt" || d.Added != 1 || d.Removed != 0 {
		t.Fatalf("%+v %v", d, err)
	}
}

func TestABaseDiffShowsTheFileFromWhereTheBranchLeftItsBaseToHead(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "cap-retries")
	write(t, repo, "a", "capped\n")
	sh(t, repo, "commit", "-qam", "Cap retries")
	write(t, repo, "a", "uncommitted\n")
	d, err := service().Diff(repo, "a", proto.DiffAt{Kind: "base"}, DiffOptions{})
	if err != nil || !reflect.DeepEqual(lines(d)[1:], []string{"d a", "a capped"}) {
		t.Fatalf("%v %v", lines(d), err)
	}
}

func TestBinariesImagesAndHugeFilesSendNoLines(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "shot.png", "\x89PNG\x00\x00")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "png")
	write(t, repo, "shot.png", "\x89PNG\x00\x01")
	s := service()
	d, _ := s.Diff(repo, "shot.png", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if !d.Binary || !d.Image || len(d.Lines) != 0 {
		t.Fatalf("%+v", d)
	}
	big := filepath.Join(repo, "huge.log")
	f, _ := os.Create(big)
	f.Truncate(MaxRead + 1)
	f.Close()
	d, _ = s.Diff(repo, "huge.log", proto.DiffAt{Kind: "working"}, DiffOptions{})
	if !d.TooLarge || len(d.Lines) != 0 {
		t.Fatalf("%+v", d)
	}
}

func TestLongLinesAreCutAndLongDiffsTruncated(t *testing.T) {
	repo := scratch(t)
	long := strings.Repeat("é", MaxLineChars+10)
	write(t, repo, "a", long+"\n"+strings.Repeat("x\n", 50))
	d, _ := service().Diff(repo, "a", proto.DiffAt{Kind: "working"}, DiffOptions{MaxLines: 10})
	add := d.Lines[2]
	if !add.Cut || len([]rune(add.T)) != MaxLineChars || d.Truncated == nil || *d.Truncated != (proto.Truncated{Shown: 10, Total: 53}) || len(d.Lines) != 10 || d.Added != 51 {
		t.Fatalf("%+v %+v", d.Truncated, add.Cut)
	}
}

func TestContextAndWhitespaceShapeTheHunks(t *testing.T) {
	repo := scratch(t)
	write(t, repo, "f.rs", "fn a() {\n    x();\n}\n1\n2\n3\n4\n5\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "f")
	write(t, repo, "f.rs", "fn a() {\n        x();\n}\n1\n2\n3\n4\n5\ny();\n")
	s := service()
	zero := 0
	d, _ := s.Diff(repo, "f.rs", proto.DiffAt{Kind: "working"}, DiffOptions{Context: &zero})
	if strings.Count(strings.Join(lines(d), "|"), "c ") != 0 {
		t.Fatalf("%v", lines(d))
	}
	d, _ = s.Diff(repo, "f.rs", proto.DiffAt{Kind: "working"}, DiffOptions{IgnoreWhitespace: true})
	for _, l := range d.Lines {
		if l.K == "d" || l.K == "a" && l.T != "y();" {
			t.Fatalf("%v", lines(d))
		}
	}
}

func TestADiffOfACommitThatDoesNotExistFails(t *testing.T) {
	repo := scratch(t)
	for _, rev := range []string{"nope", "--output=/tmp/x"} {
		if _, err := service().Diff(repo, "a", proto.DiffAt{Kind: "commit", Rev: rev}, DiffOptions{}); err == nil {
			t.Errorf("%s diffed", rev)
		}
	}
}

func TestLanguagesFollowTheDesktop(t *testing.T) {
	for path, want := range map[string]string{
		"a.rs": "rust", "b.mts": "typescript", "c.tsx": "tsx", "d.cjs": "javascript", "e.py": "python", "f.kts": "kotlin",
		"g.yml": "yaml", "h.zsh": "bash", "i.scss": "css", "Makefile": "text", "j.md": "markdown",
	} {
		if got := LanguageFor(path); got != want {
			t.Errorf("%s: %s", path, got)
		}
	}
}

func TestAPathMayNotLeaveItsTree(t *testing.T) {
	repo := scratch(t)
	outside := t.TempDir()
	os.Symlink(outside, filepath.Join(repo, "out"))
	if InTree(repo, "a", "src/new/x.go") != nil {
		t.Fatal("refused an inside path")
	}
	for _, p := range []string{"../x", "/etc/passwd", "out/secret", "a/../../x"} {
		if InTree(repo, p) != ErrOutsideTree {
			t.Errorf("%s allowed", p)
		}
	}
}
