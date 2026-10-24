package fsview

import (
	"cmp"
	"slices"
	"strings"
)

// Index is a tree's files, as `git ls-files --cached --others
// --exclude-standard` lists them, ready to search.
type Index struct {
	paths []string
	lower []string
}

// NewIndex takes ls-files output, NUL or newline separated.
func NewIndex(out string) Index {
	sep := "\n"
	if strings.Contains(out, "\x00") {
		sep = "\x00"
	}
	var ix Index
	for p := range strings.SplitSeq(out, sep) {
		if p != "" {
			ix.paths = append(ix.paths, p)
			ix.lower = append(ix.lower, strings.ToLower(p))
		}
	}
	return ix
}

func (ix Index) Len() int { return len(ix.paths) }

// Search is the best limit paths for query, each a fuzzy subsequence match:
// best score first, then changed files, then shorter paths, then git's
// order. An empty query lists the changed files, then the rest, as the
// desktop's @ mentions do.
func (ix Index) Search(query string, limit int, changed func(string) bool) []string {
	if limit <= 0 {
		return []string{}
	}
	type hit struct{ score, unchanged, length, at int }
	before := func(a, b hit) int {
		return cmp.Or(cmp.Compare(b.score, a.score), cmp.Compare(a.unchanged, b.unchanged), cmp.Compare(a.length, b.length), cmp.Compare(a.at, b.at))
	}
	q := strings.ToLower(strings.Join(strings.Fields(query), ""))
	// The best limit so far, in order; most hits lose to the last at once.
	best := make([]hit, 0, limit+1)
	for i, l := range ix.lower {
		s, ok := Score(q, ix.paths[i], l)
		if !ok {
			continue
		}
		h := hit{score: s, unchanged: 1, at: i}
		if changed != nil && changed(ix.paths[i]) {
			h.unchanged = 0
		}
		if q != "" {
			h.length = len(l)
		}
		if len(best) == limit && before(h, best[limit-1]) >= 0 {
			continue
		}
		at, _ := slices.BinarySearchFunc(best, h, before)
		best = slices.Insert(best, at, h)
		if len(best) > limit {
			best = best[:limit]
		}
	}
	out := make([]string, len(best))
	for i, h := range best {
		out[i] = ix.paths[h.at]
	}
	return out
}

const (
	scoreMatch       = 16
	scoreBoundary    = 10
	scoreSegment     = 6
	scoreCamel       = 8
	scoreConsecutive = 10
	scoreBasename    = 12
	scoreExactName   = 40
	penaltyGapStart  = 3
	penaltyGap       = 1
)

// Score rates query (lowercased, no spaces) as a subsequence of path, whose
// lowercase is lower. It takes the rightmost match, which lands in the base
// name when it can, then tightens it from the left. Matches at word starts,
// runs, and base-name matches score higher; gaps cost.
func Score(query, path, lower string) (int, bool) {
	if query == "" {
		return 0, true
	}
	if len(query) > len(lower) {
		return 0, false
	}
	// Rightmost start: match the query backwards from the end.
	qi, start := len(query)-1, -1
	for i := len(lower) - 1; i >= 0; i-- {
		if lower[i] == query[qi] {
			if qi == 0 {
				start = i
				break
			}
			qi--
		}
	}
	if start < 0 {
		return 0, false
	}
	camel := len(path) == len(lower)
	name := strings.LastIndexByte(lower, '/') + 1
	score, prev := 0, -2
	qi = 0
	for i := start; i < len(lower) && qi < len(query); i++ {
		if lower[i] != query[qi] {
			continue
		}
		score += scoreMatch
		switch {
		case i == 0 || lower[i-1] == '/':
			score += scoreBoundary + scoreSegment
		case strings.IndexByte("_-. ", lower[i-1]) >= 0:
			score += scoreBoundary
		case camel && isUpper(path[i]) && !isUpper(path[i-1]):
			score += scoreCamel
		}
		switch {
		case i == prev+1:
			score += scoreConsecutive
		case prev >= 0:
			score -= penaltyGapStart + penaltyGap*(i-prev-1)
		}
		if i >= name {
			score += scoreBasename
		}
		prev = i
		qi++
	}
	if lower[name:] == query || strings.TrimSuffix(lower[name:], extOf(lower[name:])) == query {
		score += scoreExactName
	}
	return score, true
}

func isUpper(b byte) bool { return 'A' <= b && b <= 'Z' }

func extOf(name string) string {
	if i := strings.LastIndexByte(name, '.'); i > 0 {
		return name[i:]
	}
	return ""
}
