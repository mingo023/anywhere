package workspace

import (
	"bytes"
	"errors"
	"os/exec"
	"strconv"
	"strings"

	"pocketd/internal/proto"
)

// MaxBranches is the most branch.list returns on each side.
const MaxBranches = 200

// Branches is tree's local and remote branches, newest commit first, from one
// for-each-ref; current is the branch checked out in tree, "" when detached.
// Remote names keep their remote ("origin/main"); remotes' HEAD links are left out.
func Branches(tree string) (current string, local, remote []proto.Branch, err error) {
	out, err := exec.Command("git", "-C", tree, "for-each-ref", "--sort=-committerdate",
		"--format=%(HEAD)%00%(refname)%00%(committerdate:unix)%00%(symref)", "refs/heads", "refs/remotes").Output()
	if err != nil {
		if exit, ok := errors.AsType[*exec.ExitError](err); ok {
			line, _, _ := strings.Cut(strings.TrimSpace(string(exit.Stderr)), "\n")
			return "", nil, nil, errors.New(line)
		}
		return "", nil, nil, err
	}
	current, local, remote = parseRefs(out)
	return current, local, remote, nil
}

func parseRefs(out []byte) (current string, local, remote []proto.Branch) {
	local, remote = []proto.Branch{}, []proto.Branch{}
	for line := range bytes.Lines(out) {
		f := strings.Split(strings.TrimRight(string(line), "\n"), "\x00")
		if len(f) != 4 || f[3] != "" {
			continue
		}
		at, _ := strconv.ParseInt(f[2], 10, 64)
		if name, ok := strings.CutPrefix(f[1], "refs/heads/"); ok {
			if f[0] == "*" {
				current = name
			}
			if len(local) < MaxBranches {
				local = append(local, proto.Branch{Name: name, At: at})
			}
		} else if name, ok := strings.CutPrefix(f[1], "refs/remotes/"); ok && len(remote) < MaxBranches {
			remote = append(remote, proto.Branch{Name: name, At: at})
		}
	}
	return current, local, remote
}
