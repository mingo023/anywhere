package worktree

import (
	"errors"
	"fmt"
	"os/exec"
	"strings"
)

var errNotRenamable = errors.New("The branch moved, tracks a remote or is on one")

// Renamable: the Worktree at path is still on branch, and no remote has seen it.
func Renamable(path, branch string) bool {
	cur, err := exec.Command("git", "-C", path, "branch", "--show-current").Output()
	if err != nil || strings.TrimSpace(string(cur)) != branch {
		return false
	}
	if exec.Command("git", "-C", path, "rev-parse", "-q", "--verify", branch+"@{u}").Run() == nil {
		return false
	}
	out, err := exec.Command("git", "-C", path, "for-each-ref", "--format=%(refname)", "refs/remotes/*/"+branch).Output()
	return err == nil && strings.TrimSpace(string(out)) == ""
}

// RenameBranch moves the Worktree at path from branch from to to, or to-2,
// to-3… when a local or remote branch has it, and returns the name it took.
func RenameBranch(project, path, from, to string) (string, error) {
	if to == from {
		return from, nil
	}
	if !Renamable(path, from) {
		return "", errNotRenamable
	}
	taken, err := branches(project)
	if err != nil {
		return "", err
	}
	remotes, _ := exec.Command("git", "-C", project, "for-each-ref", "--format=%(refname:lstrip=3)", "refs/remotes").Output()
	taken = append(taken, strings.Fields(string(remotes))...)
	name := to
	for i := 2; ; i++ {
		err := Validate(name, taken)
		if err == nil {
			break
		}
		if !errors.Is(err, ErrExists) || i > 99 {
			return "", err
		}
		name = fmt.Sprintf("%s-%d", to, i)
	}
	if out, err := exec.Command("git", "-C", path, "branch", "-m", from, name).CombinedOutput(); err != nil {
		return "", errors.New(strings.TrimSpace(string(out)))
	}
	return name, nil
}
