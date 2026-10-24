// Package gitview reads and changes a tree's git state for phones: status,
// diffs, history and the desktop's write ops, each a fixed git argv ported
// from the desktop's crates/git.
package gitview

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/hostop"
)

const (
	readTimeout  = 30 * time.Second
	writeTimeout = 2 * time.Minute
)

// Failed is git refusing: its first stderr line and all of it.
type Failed struct{ Message, Detail string }

func (f *Failed) Error() string { return f.Message }

// ErrOutsideTree: a path leaves its tree.
var ErrOutsideTree = errors.New("The path is outside the tree")

// Service runs git for phones. The zero value runs with pocketd's own env.
type Service struct {
	// Env is the login env git runs with, so hooks, gpg and credential helpers find the user's tools.
	Env func() []string
	// Every is how often a watched tree is polled; zero means 2 s.
	Every time.Duration

	mu     sync.Mutex
	common map[string]string
	locks  map[string]*sync.Mutex
	bases  map[string]baseEntry
	last   map[string]cached
	watch  watchers
}

func New(env func() []string) *Service { return &Service{Env: env} }

func (s *Service) env() []string {
	env := os.Environ()
	if s.Env != nil {
		env = s.Env()
	}
	// Reads never take index.lock, so polling can't race the user's own git.
	return append(slices.Clip(env), "GIT_TERMINAL_PROMPT=0", "GIT_OPTIONAL_LOCKS=0")
}

// git runs git in tree; a non-zero exit is a *Failed unless ok lists the code.
func (s *Service) git(tree string, stdin []byte, timeout time.Duration, args []string, ok ...int) ([]byte, error) {
	// Phone paths are names: "*" or ":/" must not widen a discard to the whole tree.
	argv := append([]string{"git", "--literal-pathspecs", "-c", "core.quotepath=off", "-C", tree}, args...)
	code, out, errOut, err := hostop.Run(context.Background(), s.env(), "", argv, stdin, timeout)
	if err != nil {
		return nil, err
	}
	if code != 0 && !slices.Contains(ok, code) {
		return out, failed(errOut, code)
	}
	return out, nil
}

func failed(stderr []byte, code int) *Failed {
	detail := strings.TrimSpace(string(stderr))
	first, _, _ := strings.Cut(detail, "\n")
	for _, l := range strings.Split(detail, "\n") {
		// git prefixes the line that says what went wrong; hints and progress come before it.
		if l = strings.TrimSpace(l); strings.HasPrefix(l, "error:") || strings.HasPrefix(l, "fatal:") || strings.HasPrefix(l, "!") {
			first = l
			break
		}
	}
	if code == -1 {
		first = "git took too long"
	}
	if first == "" {
		first = "git failed"
	}
	return &Failed{Message: first, Detail: detail}
}

// text is git's output with one trailing newline cut.
func (s *Service) text(tree string, args ...string) (string, error) {
	out, err := s.git(tree, nil, readTimeout, args)
	return strings.TrimSuffix(string(out), "\n"), err
}

// commonDir is the repository tree belongs to, shared by its worktrees.
func (s *Service) commonDir(tree string) (string, error) {
	s.mu.Lock()
	dir, ok := s.common[tree]
	s.mu.Unlock()
	if ok {
		return dir, nil
	}
	dir, err := s.text(tree, "rev-parse", "--path-format=absolute", "--git-common-dir")
	if err != nil {
		return "", err
	}
	s.mu.Lock()
	if s.common == nil {
		s.common = map[string]string{}
	}
	s.common[tree] = dir
	s.mu.Unlock()
	return dir, nil
}

// lock serializes writes to one repository, so they don't race on index.lock.
func (s *Service) lock(tree string) (unlock func(), err error) {
	dir, err := s.commonDir(tree)
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	if s.locks == nil {
		s.locks = map[string]*sync.Mutex{}
	}
	m := s.locks[dir]
	if m == nil {
		m = &sync.Mutex{}
		s.locks[dir] = m
	}
	s.mu.Unlock()
	m.Lock()
	return m.Unlock, nil
}

// InTree checks that each relative path stays inside tree, through symlinks too.
func InTree(tree string, paths ...string) error {
	root, err := filepath.EvalSymlinks(tree)
	if err != nil {
		root = tree
	}
	for _, p := range paths {
		if !filepath.IsLocal(p) {
			return ErrOutsideTree
		}
		dir := filepath.Dir(filepath.Join(tree, p))
		real, err := filepath.EvalSymlinks(dir)
		if err != nil {
			// A deleted file's folder may be gone too; git still names it by the tree's own path.
			continue
		}
		if real != root && !strings.HasPrefix(real, root+string(filepath.Separator)) {
			return ErrOutsideTree
		}
	}
	return nil
}

// fields splits NUL-terminated output.
func fields(out []byte) []string {
	out = bytes.TrimSuffix(out, []byte{0})
	if len(out) == 0 {
		return nil
	}
	return strings.Split(string(out), "\x00")
}
