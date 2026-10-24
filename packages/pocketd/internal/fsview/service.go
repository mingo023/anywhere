package fsview

import (
	"cmp"
	"context"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/hostop"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

const (
	// lettersFresh is how long a tree's git letters serve listings; the
	// desktop refreshes git every 2 s too.
	lettersFresh = 2 * time.Second
	// filesFresh is how long a tree's file index serves searches before a
	// search starts re-listing it behind the answer.
	filesFresh = 5 * time.Second
	gitTimeout = 30 * time.Second
)

// Service serves fs.v1 over the registered Projects' trees.
type Service struct {
	Registry *registry.Registry
	// Env is the login environment git runs with.
	Env func() []string
	// Attachments is where uploads land: <home>/attachments.
	Attachments string

	mu    sync.Mutex
	trees map[string]*caches
}

// caches are one tree's git reads.
type caches struct {
	letters cached[Letters]
	files   cached[Index]
}

// cached is a value and when it was read, read by one caller at a time.
type cached[T any] struct {
	mu    sync.Mutex
	value T
	at    time.Time
	// loading is closed when the read under way lands.
	loading chan struct{}
}

// get is the value while it is fresh, else a new read that callers share.
// Stale with behind, the old value answers while the read runs.
func (c *cached[T]) get(fresh time.Duration, behind bool, read func() T) T {
	c.mu.Lock()
	for c.loading != nil && (c.at.IsZero() || !behind) {
		wait := c.loading
		c.mu.Unlock()
		<-wait
		c.mu.Lock()
	}
	if !c.at.IsZero() && (time.Since(c.at) < fresh || c.loading != nil) {
		defer c.mu.Unlock()
		return c.value
	}
	done := make(chan struct{})
	c.loading = done
	if behind && !c.at.IsZero() {
		v := c.value
		c.mu.Unlock()
		go c.fill(read, done)
		return v
	}
	c.mu.Unlock()
	return c.fill(read, done)
}

func (c *cached[T]) fill(read func() T, done chan struct{}) T {
	v := read()
	c.mu.Lock()
	c.value, c.at, c.loading = v, time.Now(), nil
	c.mu.Unlock()
	close(done)
	return v
}

func (s *Service) caches(root string) *caches {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.trees == nil {
		s.trees = map[string]*caches{}
	}
	if s.trees[root] == nil {
		s.trees[root] = &caches{}
	}
	return s.trees[root]
}

func (s *Service) env() []string {
	if s.Env == nil {
		return nil
	}
	return s.Env()
}

// Resolve is tree's real root and rel's real path inside it.
func (s *Service) Resolve(tree, rel string) (root, full string, err error) {
	if root, err = Tree(s.Registry.Load(), tree); err != nil {
		return "", "", err
	}
	full, err = Join(root, rel)
	return root, full, err
}

// List is dir's entries, folders first then by name, with ignored flags and
// git letters; .git is left out, as the Explorer leaves it.
func (s *Service) List(ctx context.Context, tree, dir string) ([]proto.FsEntry, error) {
	root, full, err := s.Resolve(tree, dir)
	if err != nil {
		return nil, err
	}
	raw, err := hostop.List(ctx, s.env(), full)
	if err != nil {
		return nil, err
	}
	letters := s.lettersOf(ctx, root)
	rel, _ := filepath.Rel(root, full)
	entries := make([]proto.FsEntry, 0, len(raw))
	for _, e := range raw {
		if e.Name == ".git" {
			continue
		}
		p := filepath.ToSlash(filepath.Join(rel, e.Name))
		git := letters.File(p)
		if e.Dir {
			git = letters.Folder(p)
		}
		entries = append(entries, proto.FsEntry{Name: e.Name, Dir: e.Dir, Link: e.Link, Size: e.Size, Mtime: e.Mtime, Ignored: e.Ignored, Git: git})
	}
	Sort(entries)
	return entries, nil
}

// Sort puts folders first, then orders by name, case first, as the
// Explorer's default (prefs::Files::sort).
func Sort(entries []proto.FsEntry) {
	slices.SortFunc(entries, func(a, b proto.FsEntry) int {
		if a.Dir != b.Dir {
			if a.Dir {
				return -1
			}
			return 1
		}
		return cmp.Compare(a.Name, b.Name)
	})
}

// Read is the preview of the file at rel.
func (s *Service) Read(tree, rel string, max int) (proto.FsRead, error) {
	if rel == "" {
		return proto.FsRead{}, errOutsideTree
	}
	_, full, err := s.Resolve(tree, rel)
	if err != nil {
		return proto.FsRead{}, err
	}
	r := Read(full, rel, max)
	r.Tree = tree
	return r, nil
}

// Search is go-to-file over the tree's files, changed files first on ties.
func (s *Service) Search(ctx context.Context, tree, query string, limit int) ([]string, error) {
	root, _, err := s.Resolve(tree, "")
	if err != nil {
		return nil, err
	}
	ix := s.indexOf(ctx, root)
	letters := s.lettersOf(ctx, root)
	return ix.Search(query, limit, func(p string) bool { return letters.File(p) != "" }), nil
}

// lettersOf and indexOf read without the request's cancel, since other
// requests share the read.
func (s *Service) lettersOf(ctx context.Context, root string) Letters {
	return s.caches(root).letters.get(lettersFresh, false, func() Letters { return s.readLetters(context.WithoutCancel(ctx), root) })
}

func (s *Service) indexOf(ctx context.Context, root string) Index {
	return s.caches(root).files.get(filesFresh, true, func() Index { return s.readIndex(context.WithoutCancel(ctx), root) })
}

func (s *Service) git(ctx context.Context, root string, args ...string) (string, bool) {
	code, out, _, err := hostop.Run(ctx, s.env(), root, append([]string{"git"}, args...), nil, gitTimeout)
	return string(out), err == nil && code == 0
}

func (s *Service) readLetters(ctx context.Context, root string) Letters {
	prefix, ok := s.git(ctx, root, "rev-parse", "--show-prefix")
	if !ok {
		return Letters{}
	}
	out, _ := s.git(ctx, root, "status", "--porcelain=v2", "-z")
	return ParseStatus([]byte(out), strings.TrimSpace(prefix))
}

func (s *Service) readIndex(ctx context.Context, root string) Index {
	out, _ := s.git(ctx, root, "ls-files", "--cached", "--others", "--exclude-standard", "-z")
	return NewIndex(out)
}
