package gitview

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"unicode/utf8"

	"pocketd/internal/proto"
)

const (
	// DefaultMaxLines is how many rows a git.diff sends without maxLines.
	DefaultMaxLines = 4000
	// MaxLineChars is Zed's MAX_LINE_LEN: longer lines are cut.
	MaxLineChars = 1024
	// MaxRead is the desktop's MAX_READ: a side above it is too large to diff.
	MaxRead = 16 << 20
	// MaxWordLines is the desktop's MAX_WORD_LINES: longer runs get no word ranges.
	MaxWordLines = 5
)

// DiffOptions are git.diff's knobs; a nil Context is git's 3.
type DiffOptions struct {
	Context          *int
	IgnoreWhitespace bool
	MaxLines         int
}

// Diff is path as at compares it, parsed from git's own patch.
func (s *Service) Diff(tree, path string, at proto.DiffAt, o DiffOptions) (proto.GitDiff, error) {
	d := proto.GitDiff{Tree: tree, Path: path, Language: LanguageFor(path), Image: imageName(path)}
	args := []string{"diff", "--no-color", "--no-ext-diff", "--histogram", "-M"}
	if o.Context != nil {
		args = append(args, "-U"+strconv.Itoa(*o.Context))
	}
	if o.IgnoreWhitespace {
		args = append(args, "--ignore-all-space")
	}
	ok := []int{}
	working := false
	switch at.Kind {
	case "working", "staged":
		e, err := s.entryOf(tree, path)
		if err != nil {
			return d, err
		}
		d.OldPath = e.old
		switch {
		case e.kind == '?' && at.Kind == "working":
			args = append(args, "--no-index", "--", os.DevNull, path)
			// --no-index exits 1 when the files differ, which they always do.
			ok = []int{1}
		case at.Kind == "staged":
			args = append(append(args, "--cached", "--"), pair(e.old, path)...)
		default:
			head := "HEAD"
			if !s.hasHead(tree) {
				head = emptyTree
			}
			args = append(append(args, head, "--"), pair(e.old, path)...)
		}
		working = at.Kind == "working"
	case "commit":
		sha, err := s.commitSha(tree, at.Rev)
		if err != nil {
			return d, err
		}
		parent := s.parentOf(tree, sha)
		d.OldPath = s.renamedFrom(tree, parent, sha, path)
		args = append(append(args, parent, sha, "--"), pair(d.OldPath, path)...)
	case "base":
		base := at.Rev
		if base == "" {
			branch, _ := s.text(tree, "symbolic-ref", "-q", "--short", "HEAD")
			upstream, _ := s.text(tree, "rev-parse", "--abbrev-ref", "@{u}")
			if base = s.baseFor(tree, branch, upstream); base == "" {
				return d, &Failed{Message: "This branch has no base to compare with"}
			}
		}
		if strings.HasPrefix(base, "-") {
			return d, &Failed{Message: "Not a revision: " + base}
		}
		fork, err := s.text(tree, "merge-base", base, "HEAD")
		if err != nil {
			return d, err
		}
		d.OldPath = s.renamedFrom(tree, fork, "HEAD", path)
		args = append(append(args, fork, "HEAD", "--"), pair(d.OldPath, path)...)
	}
	if working {
		if fi, err := os.Stat(filepath.Join(tree, path)); err == nil && fi.Size() > MaxRead {
			d.TooLarge = true
			return proto.NewGitDiff(d), nil
		}
	}
	out, err := s.git(tree, nil, readTimeout, args, ok...)
	if err != nil {
		return d, err
	}
	rows, binary := parsePatch(string(out))
	if binary {
		d.Binary = true
		return proto.NewGitDiff(d), nil
	}
	max := o.MaxLines
	if max <= 0 {
		max = DefaultMaxLines
	}
	for _, r := range rows {
		switch r.K {
		case "a":
			d.Added++
		case "d":
			d.Removed++
		}
	}
	if len(rows) > max {
		d.Truncated = &proto.Truncated{Shown: max, Total: len(rows)}
		rows = rows[:max]
	}
	for i := range rows {
		rows[i].T, rows[i].Cut = cut(rows[i].T)
	}
	addWords(rows)
	d.Lines = rows
	return proto.NewGitDiff(d), nil
}

// pair names a renamed file by both paths, so -M sees the rename.
func pair(old, path string) []string {
	if old != "" {
		return []string{old, path}
	}
	return []string{path}
}

// entryOf is path's line in the status, or a tracked file with no changes.
func (s *Service) entryOf(tree, path string) (entry, error) {
	out, err := s.git(tree, nil, readTimeout, append(append([]string{}, porcelainArgs...), "--", path))
	if err != nil {
		return entry{}, err
	}
	for _, e := range parsePorcelain(out).entries {
		if e.path != path {
			continue
		}
		// A pathspec of the new name alone hides a staged rename's old name, so an add asks the index.
		if e.kind == '1' && e.x == 'A' && s.hasHead(tree) {
			names, err := s.git(tree, nil, readTimeout, []string{"diff", "--cached", "-z", "-M", "--name-status", "HEAD"})
			if err == nil {
				for _, f := range parseNameStatus(names) {
					if f.Path == path && f.OldPath != "" {
						e.old, e.kind = f.OldPath, '2'
					}
				}
			}
		}
		return e, nil
	}
	return entry{path: path, kind: '1'}, nil
}

func (s *Service) hasHead(tree string) bool {
	_, err := s.git(tree, nil, readTimeout, []string{"rev-parse", "--verify", "--quiet", "HEAD"})
	return err == nil
}

// commitSha resolves rev to a commit, refusing anything git would read as an option.
func (s *Service) commitSha(tree, rev string) (string, error) {
	if rev == "" || strings.HasPrefix(rev, "-") {
		return "", &Failed{Message: "Not a commit: " + rev}
	}
	sha, err := s.text(tree, "rev-parse", "--verify", "--quiet", rev+"^{commit}")
	if f, ok := errors.AsType[*Failed](err); ok {
		f.Message = "Not a commit: " + rev
	}
	return sha, err
}

// parentOf is sha's first parent, or the empty tree for a root commit.
func (s *Service) parentOf(tree, sha string) string {
	if p, err := s.text(tree, "rev-parse", "--verify", "--quiet", sha+"^"); err == nil {
		return p
	}
	return emptyTree
}

// renamedFrom is path's old name when from→to renamed it, as commit_files reads it.
func (s *Service) renamedFrom(tree, from, to, path string) string {
	out, err := s.git(tree, nil, readTimeout, []string{"diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id", from, to})
	if err != nil {
		return ""
	}
	for _, f := range parseNameStatus(out) {
		if f.Path == path {
			return f.OldPath
		}
	}
	return ""
}

// parsePatch reads git's unified diff into rows numbered on each side.
func parsePatch(out string) (rows []proto.DiffRow, binary bool) {
	out = strings.TrimSuffix(out, "\n")
	if out == "" {
		return nil, false
	}
	body := false
	o, n := 0, 0
	for l := range strings.SplitSeq(out, "\n") {
		if strings.HasPrefix(l, "@@") {
			body = true
			o, n = hunkStart(l, '-'), hunkStart(l, '+')
			rows = append(rows, proto.DiffRow{K: "h", T: l})
			continue
		}
		if strings.HasPrefix(l, "diff ") {
			body = false
			continue
		}
		if !body {
			if strings.HasPrefix(l, "Binary files ") || l == "GIT binary patch" {
				binary = true
			}
			continue
		}
		var k byte
		if l != "" {
			k = l[0]
		}
		switch k {
		case '+':
			rows = append(rows, proto.DiffRow{K: "a", N: n, T: l[1:]})
			n++
		case '-':
			rows = append(rows, proto.DiffRow{K: "d", O: o, T: l[1:]})
			o++
		case ' ', 0:
			rows = append(rows, proto.DiffRow{K: "c", O: o, N: n, T: strings.TrimPrefix(l, " ")})
			o++
			n++
		}
	}
	return rows, binary
}

// hunkStart is hunk_start: the first line a header's side starts at.
func hunkStart(header string, sign byte) int {
	for _, w := range strings.Fields(header) {
		if len(w) > 1 && w[0] == sign {
			num, _, _ := strings.Cut(w[1:], ",")
			if v, err := strconv.Atoi(num); err == nil {
				return v
			}
		}
	}
	return 1
}

func cut(t string) (string, bool) {
	if utf8.RuneCountInString(t) <= MaxLineChars {
		return t, false
	}
	i, chars := 0, 0
	for i = range t {
		if chars == MaxLineChars {
			break
		}
		chars++
	}
	return t[:i], true
}

// addWords marks the changed words of each run of up to five deletions
// replaced line for line, as the desktop's highlights does.
func addWords(rows []proto.DiffRow) {
	for i := 0; i < len(rows); {
		dels := 0
		for i+dels < len(rows) && rows[i+dels].K == "d" {
			dels++
		}
		if dels == 0 {
			i++
			continue
		}
		adds := 0
		for i+dels+adds < len(rows) && rows[i+dels+adds].K == "a" {
			adds++
		}
		if dels == adds && dels <= MaxWordLines {
			for k := range dels {
				d, a := &rows[i+k], &rows[i+dels+k]
				d.W, a.W = Words(d.T, a.T)
			}
		}
		i += dels + adds
	}
}

// LanguageFor is the desktop's language_for (pocket/src/syntax.rs).
func LanguageFor(path string) string {
	switch strings.TrimPrefix(filepath.Ext(path), ".") {
	case "rs":
		return "rust"
	case "ts", "mts", "cts":
		return "typescript"
	case "tsx":
		return "tsx"
	case "js", "jsx", "mjs", "cjs":
		return "javascript"
	case "py":
		return "python"
	case "go":
		return "go"
	case "swift":
		return "swift"
	case "kt", "kts":
		return "kotlin"
	case "java":
		return "java"
	case "json":
		return "json"
	case "md":
		return "markdown"
	case "toml":
		return "toml"
	case "yml", "yaml":
		return "yaml"
	case "sh", "zsh", "bash":
		return "bash"
	case "css", "scss":
		return "css"
	case "html":
		return "html"
	}
	return "text"
}

// imageName is the desktop preview's image_format.
func imageName(path string) bool {
	switch strings.ToLower(strings.TrimPrefix(filepath.Ext(path), ".")) {
	case "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg":
		return true
	}
	return false
}
