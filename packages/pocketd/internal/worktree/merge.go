package worktree

import (
	"errors"
	"os/exec"
	"strings"
)

// gitIn runs git in dir and returns its output; its error is git's own message.
func gitIn(dir string, args ...string) (string, error) {
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).Output()
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return string(out), errors.New(strings.TrimSpace(string(exit.Stderr)))
	}
	return string(out), err
}

// Head is the commit dir's checkout is on.
func Head(dir string) (string, error) {
	out, err := gitIn(dir, "rev-parse", "HEAD")
	return strings.TrimSpace(out), err
}

// Commit commits everything in dir's checkout, untracked files too, and says
// whether there was anything to commit. Hooks don't run: pocketd commits for
// an agent that was told not to, and a failing hook would lose its work.
func Commit(dir, message string) (bool, error) {
	status, err := gitIn(dir, "status", "--porcelain")
	if err != nil || strings.TrimSpace(status) == "" {
		return false, err
	}
	if _, err := gitIn(dir, "add", "-A"); err != nil {
		return false, err
	}
	if _, err := gitIn(dir, "commit", "-q", "--no-verify", "-m", message); err != nil {
		return false, err
	}
	return true, nil
}

// Stat is `git diff --stat` of what dir's branch changed since it left base,
// counted from their merge base so base moving on since adds nothing.
func Stat(dir, base string) (string, error) {
	out, err := gitIn(dir, "diff", "--stat", base+"...HEAD")
	return strings.TrimRight(out, "\n"), err
}

// Conflict is a merge of Branch that git stopped on, and the files it conflicted in.
type Conflict struct {
	Branch string
	Files  []string
}

func (c *Conflict) Error() string {
	return "Merging " + c.Branch + " conflicted in: " + strings.Join(c.Files, ", ")
}

// MergeBranches makes branch name in project from branches merged in order,
// without touching a checkout. A conflict is a *Conflict, and makes no branch.
func MergeBranches(project, name string, branches []string) error {
	out, err := gitIn(project, "rev-parse", "--verify", branches[0]+"^{commit}")
	if err != nil {
		return err
	}
	head := strings.TrimSpace(out)
	for _, b := range branches[1:] {
		out, err := gitIn(project, "merge-tree", "--write-tree", "--name-only", "--no-messages", head, b)
		lines := strings.Split(strings.TrimSpace(out), "\n")
		if err != nil && len(lines) > 1 {
			return &Conflict{Branch: b, Files: lines[1:]}
		}
		if err != nil {
			return err
		}
		if out, err = gitIn(project, "commit-tree", lines[0], "-p", head, "-p", b, "-m", "Merge "+b); err != nil {
			return err
		}
		head = strings.TrimSpace(out)
	}
	_, err = gitIn(project, "branch", name, head)
	return err
}

// ErrDirty refuses a merge into a checkout with uncommitted changes.
var ErrDirty = coded("dirty_checkout", "This checkout has uncommitted changes; commit or stash them first")

// Dirty is whether dir's checkout has uncommitted changes to tracked files.
func Dirty(dir string) (bool, error) {
	out, err := gitIn(dir, "status", "--porcelain", "--untracked-files=no")
	return strings.TrimSpace(out) != "", err
}

// Merge merges branches into dir's checkout in order, each with its own merge
// commit, and returns those it merged. A conflict aborts that merge and stops
// there: the error is a *Conflict.
func Merge(dir string, branches []string) ([]string, error) {
	dirty, err := Dirty(dir)
	if err != nil {
		return nil, err
	}
	if dirty {
		return nil, ErrDirty
	}
	var merged []string
	for _, b := range branches {
		if _, err := gitIn(dir, "merge", "-q", "--no-ff", "--no-edit", "-m", "Merge "+b, b); err != nil {
			files, _ := gitIn(dir, "diff", "--name-only", "--diff-filter=U")
			gitIn(dir, "merge", "--abort")
			if files = strings.TrimSpace(files); files != "" {
				return merged, &Conflict{Branch: b, Files: strings.Split(files, "\n")}
			}
			return merged, err
		}
		merged = append(merged, b)
	}
	return merged, nil
}

// MergeInProgress is whether dir's checkout is in the middle of a merge.
func MergeInProgress(dir string) (bool, error) {
	err := exec.Command("git", "-C", dir, "rev-parse", "-q", "--verify", "MERGE_HEAD").Run()
	if _, ok := errors.AsType[*exec.ExitError](err); ok {
		return false, nil
	}
	return err == nil, err
}

// AbortMerge aborts the merge in progress in dir's checkout.
func AbortMerge(dir string) error {
	_, err := gitIn(dir, "merge", "--abort")
	return err
}
