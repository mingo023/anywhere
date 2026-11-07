package gitview

import (
	"errors"
	"fmt"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

// forked is main with a, then feature off it changing a and adding b while main gains c; on feature.
func forked(t *testing.T) string {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "feature")
	write(t, repo, "a", "changed\n")
	write(t, repo, "b", "b\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "feature work")
	sh(t, repo, "checkout", "-q", "main")
	write(t, repo, "c", "c\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "main work")
	sh(t, repo, "checkout", "-q", "feature")
	return repo
}

func TestACompareShowsWhatTheBranchCommittedSinceItLeftTheRefAndNothingUncommitted(t *testing.T) {
	repo := forked(t)
	write(t, repo, "b", "b\nmore\n")
	write(t, repo, "new", "n\n")
	c, err := service().Compare(repo, "main", 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	want := []proto.CommitFile{{Path: "a", Status: "M", Added: new(1), Removed: new(1)}, {Path: "b", Status: "A", Added: new(1), Removed: new(0)}}
	if !reflect.DeepEqual(c.Files, want) || c.Totals != (proto.CompareTotals{Files: 2, Added: 2, Removed: 1}) || c.Against != "main" {
		t.Fatalf("%+v", c)
	}
}

func TestACompareNamesARenamedFilesOldPath(t *testing.T) {
	repo := forked(t)
	sh(t, repo, "mv", "a", "z")
	sh(t, repo, "commit", "-qm", "rename")
	c, _ := service().Compare(repo, "feature~1", 0, 0)
	if len(c.Files) != 1 || c.Files[0].Path != "z" || c.Files[0].OldPath != "a" || c.Files[0].Status != "R" {
		t.Fatalf("%+v", c.Files)
	}
}

func TestACompareAgainstARefThatNamesNoCommitIsUnknown(t *testing.T) {
	repo := forked(t)
	for _, ref := range []string{"deleted-branch", "--all"} {
		if _, err := service().Compare(repo, ref, 0, 0); !errors.Is(err, ErrUnknownRef) {
			t.Errorf("%s: %v", ref, err)
		}
	}
}

func TestACompareTakesItsLaterPagesFromTheFirstRead(t *testing.T) {
	repo := scratch(t)
	sh(t, repo, "checkout", "-qb", "feature")
	for i := range 5 {
		write(t, repo, fmt.Sprintf("f%d", i), "x\n")
	}
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "five")
	s := service()
	first, _ := s.Compare(repo, "main", 0, 3)
	write(t, repo, "f9", "x\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "six")
	second, _ := s.Compare(repo, "main", *first.NextCursor, 3)
	if len(first.Files) != 3 || len(second.Files) != 2 || second.NextCursor != nil || second.Totals.Files != 5 || second.Files[1].Path != "f4" {
		t.Fatalf("%+v\n%+v", first, second)
	}
}
