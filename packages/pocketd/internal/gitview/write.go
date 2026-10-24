package gitview

import (
	"slices"
	"strings"
)

func withPaths(args []string, paths []string) []string {
	return append(append(args, "--"), paths...)
}

// write runs one change under the repository's lock, then tells watchers.
func (s *Service) write(tree string, do func() error) error {
	unlock, err := s.lock(tree)
	if err != nil {
		return err
	}
	err = do()
	unlock()
	s.forget(tree)
	go s.Kick(tree)
	return err
}

// forget drops tree's cached status, which a write made stale.
func (s *Service) forget(tree string) {
	s.mu.Lock()
	delete(s.last, tree)
	s.mu.Unlock()
}

// Stage leaves each path wholly staged or wholly unstaged (git.rs set_staged).
func (s *Service) Stage(tree string, paths []string, staged bool) error {
	return s.write(tree, func() error {
		args := []string{"add", "-A"}
		if !staged {
			args = []string{"reset", "-q"}
			if !s.hasHead(tree) {
				// Before the first commit there is nothing to reset to.
				args = []string{"rm", "-r", "-q", "--cached", "--ignore-unmatch"}
			}
		}
		_, err := s.git(tree, nil, readTimeout, withPaths(args, paths))
		return err
	})
}

// Discard throws away the unstaged edits to paths, deleting the untracked ones (git.rs discard).
func (s *Service) Discard(tree string, paths []string) error {
	return s.write(tree, func() error {
		out, err := s.git(tree, nil, readTimeout, withPaths([]string{"ls-files", "-z", "--others", "--exclude-standard"}, paths))
		if err != nil {
			return err
		}
		untracked := fields(out)
		var tracked []string
		for _, p := range paths {
			if !slices.Contains(untracked, p) {
				tracked = append(tracked, p)
			}
		}
		if len(untracked) > 0 {
			if _, err := s.git(tree, nil, readTimeout, withPaths([]string{"clean", "-fq"}, untracked)); err != nil {
				return err
			}
		}
		if len(tracked) > 0 {
			_, err = s.git(tree, nil, readTimeout, withPaths([]string{"checkout", "-q"}, tracked))
		}
		return err
	})
}

// CommitArgs is commit_argv: the message comes on stdin, and amending with none keeps the last.
func CommitArgs(amend bool, message string) []string {
	args := []string{"commit", "-q"}
	if amend {
		args = append(args, "--amend")
	}
	if message == "" {
		return append(args, "--no-edit")
	}
	return append(args, "-F", "-")
}

// CreateCommit commits what is staged; with all and nothing staged it stages
// every change first, as the desktop's commit_all. It returns the new HEAD.
func (s *Service) CreateCommit(tree, message string, amend, all bool) (string, error) {
	if !amend && strings.TrimSpace(message) == "" {
		return "", &Failed{Message: "Write a commit message first"}
	}
	var sha string
	err := s.write(tree, func() error {
		if all && !amend && s.nothingStaged(tree) {
			if _, err := s.git(tree, nil, readTimeout, []string{"add", "-A"}); err != nil {
				return err
			}
		}
		if _, err := s.git(tree, []byte(message), writeTimeout, CommitArgs(amend, message)); err != nil {
			return err
		}
		var err error
		sha, err = s.text(tree, "rev-parse", "HEAD")
		return err
	})
	return sha, err
}

func (s *Service) nothingStaged(tree string) bool {
	if !s.hasHead(tree) {
		out, _ := s.git(tree, nil, readTimeout, []string{"ls-files", "-z", "--cached"})
		return len(out) == 0
	}
	_, err := s.git(tree, nil, readTimeout, []string{"diff", "--cached", "--quiet"})
	return err == nil
}

// PushArgs is push_argv: the first push sets the upstream; remote picks where
// to; with lease a rewritten branch replaces the remote one unless it holds
// commits not yet fetched.
func PushArgs(remote string, lease bool) []string {
	args := []string{"-c", "push.autoSetupRemote=true"}
	if remote != "" {
		args = append(args, "-c", "remote.pushDefault="+remote)
	}
	args = append(args, "push")
	if lease {
		args = append(args, "--force-with-lease", "--force-if-includes")
	}
	return args
}

func (s *Service) Push(tree string, lease bool) error {
	return s.write(tree, func() error {
		_, err := s.git(tree, nil, writeTimeout, PushArgs("", lease))
		return err
	})
}

// Pull fast-forwards to the upstream and refuses anything else.
func (s *Service) Pull(tree string) error {
	return s.write(tree, func() error {
		_, err := s.git(tree, nil, writeTimeout, []string{"pull", "--ff-only"})
		return err
	})
}
