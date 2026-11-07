package gitview

import (
	"errors"
	"slices"
	"strings"
	"time"

	"pocketd/internal/proto"
)

// ErrUnknownRef: a compare's ref names no commit.
var ErrUnknownRef = errors.New("No branch or commit by that name")

type comparison struct {
	at      time.Time
	compare proto.GitCompare
}

// Compare is one page of what HEAD committed since it left against
// (`against...HEAD`, crates/git/src/compare.rs read), files sorted by path.
// A page past the first comes from the read that served the first, while it
// is fresh, so pages agree.
func (s *Service) Compare(tree, against string, cursor, limit int) (proto.GitCompare, error) {
	if limit <= 0 {
		limit = DefaultStatusPage
	}
	key := tree + "\x00" + against
	s.mu.Lock()
	c, ok := s.compared[key]
	s.mu.Unlock()
	if cursor == 0 || !ok || time.Since(c.at) > pageFor {
		full, err := s.readCompare(tree, against)
		if err != nil {
			return proto.GitCompare{}, err
		}
		c = comparison{time.Now(), full}
		s.mu.Lock()
		if s.compared == nil {
			s.compared = map[string]comparison{}
		}
		s.compared[key] = c
		s.mu.Unlock()
	}
	page := c.compare
	files := page.Files
	lo, hi := min(cursor, len(files)), min(cursor+limit, len(files))
	page.Files = files[lo:hi]
	if hi < len(files) {
		page.NextCursor = &hi
	}
	return proto.NewGitCompare(page), nil
}

func (s *Service) readCompare(tree, against string) (proto.GitCompare, error) {
	if strings.HasPrefix(against, "-") {
		return proto.GitCompare{}, ErrUnknownRef
	}
	if _, err := s.git(tree, nil, readTimeout, []string{"rev-parse", "--verify", "--quiet", against + "^{commit}"}); err != nil {
		if _, ok := errors.AsType[*Failed](err); ok {
			return proto.GitCompare{}, ErrUnknownRef
		}
		return proto.GitCompare{}, err
	}
	// Where HEAD left against, so a compare shows what a PR would; against itself when they share no history.
	fork, err := s.text(tree, "merge-base", against, "HEAD")
	if err != nil {
		fork = against
	}
	files, err := s.changedFiles(tree, fork, "HEAD")
	if err != nil {
		return proto.GitCompare{}, err
	}
	slices.SortFunc(files, func(a, b proto.CommitFile) int { return strings.Compare(a.Path, b.Path) })
	c := proto.GitCompare{Tree: tree, Against: against, Files: files}
	c.Totals.Files = len(files)
	for _, f := range files {
		if f.Added != nil {
			c.Totals.Added += *f.Added
			c.Totals.Removed += *f.Removed
		}
	}
	return c, nil
}
