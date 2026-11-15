package factory

import (
	"fmt"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestTheFixNoteNamesTheFailedCommandWithItsExitCodeAndOutput(t *testing.T) {
	checks := []proto.FactoryCheck{{Command: "cargo fmt --check", Exit: 0, Output: "fine\n"}, {Command: "cargo test", Exit: 101, Output: "test a ... ok\ntest b ... FAILED\n"}}
	want := `<anywhere_factory_fix attempt="1" of="2">
The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.
$ cargo test   (exit 101)
test a ... ok
test b ... FAILED
</anywhere_factory_fix>`
	if got := FixNote(1, 2, "", checks, nil); got != want {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}

func TestTheFixNoteKeepsTheLast200LinesOfAnOutput(t *testing.T) {
	var out strings.Builder
	for i := range 500 {
		fmt.Fprintf(&out, "line %d\n", i)
	}
	got := FixNote(1, 2, "", []proto.FactoryCheck{{Command: "make", Exit: 2, Output: out.String()}}, nil)
	if strings.Contains(got, "line 299\n") || !strings.Contains(got, "(exit 2)\nline 300\n") || !strings.Contains(got, "line 499\n</anywhere_factory_fix>") {
		t.Fatalf("got\n%s", got)
	}
}

func TestTheFixNoteListsEachFailedGitHubCheckWithItsLink(t *testing.T) {
	pr := &proto.PR{Base: "main", Runs: []proto.CheckRun{{Name: "lint", Outcome: "passed", URL: "https://x/1"}, {Name: "e2e", Outcome: "failed", URL: "https://x/2"}, {Name: "build", Outcome: "failed", URL: "https://x/3"}}}
	got := FixNote(2, 2, "", nil, pr)
	if !strings.Contains(got, "\nGitHub check failed: e2e https://x/2\nGitHub check failed: build https://x/3\n</anywhere_factory_fix>") || strings.Contains(got, "lint") {
		t.Fatalf("got\n%s", got)
	}
}

func TestTheFixNoteForABaseConflictSaysWhatToMerge(t *testing.T) {
	got := FixNote(1, 2, "", nil, &proto.PR{Base: "main", Mergeable: "CONFLICTING"})
	if !strings.Contains(got, "\nThe pull request conflicts with main. Merge origin/main into this branch and resolve the conflicts.\n") {
		t.Fatalf("got\n%s", got)
	}
}

func TestAHintNoteIsMarkedAsYoursAndStartsWithYourText(t *testing.T) {
	got := FixNote(3, 2, "  Bump the snapshot instead.\n", []proto.FactoryCheck{{Command: "make", Exit: 2, Output: "no newline at the end"}}, nil)
	want := `<anywhere_factory_fix attempt="3" of="2" by="you">
Bump the snapshot instead.
The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.
$ make   (exit 2)
no newline at the end
</anywhere_factory_fix>`
	if got != want {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}
