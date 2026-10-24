package gitview

import (
	"fmt"
	"hash/fnv"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
)

const (
	// DefaultStatusPage is a git.status page without a limit: about 30 KB.
	DefaultStatusPage = 500
	maxCommits        = 20
	// emptyTree is what an unborn branch's changes are counted against.
	emptyTree = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
	// pageFor is how long a status read serves the pages after its first.
	pageFor = 30 * time.Second
	baseFor = time.Minute
)

type entry struct {
	path, old string
	x, y      byte
	kind      byte
}

func (e entry) status() string {
	switch e.kind {
	case '?':
		return "?"
	case 'u':
		return "U"
	case '2':
		if e.x == 'C' || e.y == 'C' {
			return "A"
		}
		return "R"
	}
	switch {
	case e.x == 'A':
		return "A"
	case e.x == 'D' || e.y == 'D':
		return "D"
	}
	return "M"
}

// porcelain is `git status --porcelain=v2 -z --branch` read.
type porcelain struct {
	oid, head, upstream string
	ahead, behind       int
	tracked             bool
	entries             []entry
}

func (p porcelain) unborn() bool   { return p.oid == "(initial)" }
func (p porcelain) detached() bool { return p.head == "(detached)" }

func parsePorcelain(out []byte) porcelain {
	var p porcelain
	recs := fields(out)
	for i := 0; i < len(recs); i++ {
		r := recs[i]
		if len(r) < 2 {
			continue
		}
		switch r[0] {
		case '#':
			key, val, _ := strings.Cut(r[2:], " ")
			switch key {
			case "branch.oid":
				p.oid = val
			case "branch.head":
				p.head = val
			case "branch.upstream":
				p.upstream = val
			case "branch.ab":
				fmt.Sscanf(val, "+%d -%d", &p.ahead, &p.behind)
				p.tracked = true
			}
		case '1':
			if f := strings.SplitN(r, " ", 9); len(f) == 9 {
				p.entries = append(p.entries, entry{path: f[8], x: f[1][0], y: f[1][1], kind: '1'})
			}
		case '2':
			if f := strings.SplitN(r, " ", 10); len(f) == 10 && i+1 < len(recs) {
				i++
				p.entries = append(p.entries, entry{path: f[9], old: recs[i], x: f[1][0], y: f[1][1], kind: '2'})
			}
		case 'u':
			if f := strings.SplitN(r, " ", 11); len(f) == 11 {
				p.entries = append(p.entries, entry{path: f[10], x: f[1][0], y: f[1][1], kind: 'u'})
			}
		case '?':
			p.entries = append(p.entries, entry{path: r[2:], x: '?', y: '?', kind: '?'})
		}
	}
	return p
}

type count struct {
	added, removed int
	binary         bool
}

// parseNumstat reads `--numstat -z`; a rename's counts are under its new path.
func parseNumstat(out []byte) map[string]count {
	got := map[string]count{}
	recs := fields(out)
	for i := 0; i < len(recs); i++ {
		parts := strings.SplitN(recs[i], "\t", 3)
		if len(parts) != 3 {
			continue
		}
		path := parts[2]
		if path == "" && i+2 < len(recs) {
			path = recs[i+2]
			i += 2
		}
		c := count{binary: parts[0] == "-"}
		c.added, _ = strconv.Atoi(parts[0])
		c.removed, _ = strconv.Atoi(parts[1])
		got[path] = c
	}
	return got
}

// porcelainArgs are the status the poller stamps and git.status reads; the two must agree.
var porcelainArgs = []string{"status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"}

// stamp hashes the porcelain output with each changed file's size and mtime,
// so an edit to a file that was already changed moves it too. Nothing is read.
func stamp(tree string, out []byte, p porcelain) string {
	h := fnv.New64a()
	h.Write(out)
	for _, e := range p.entries {
		if fi, err := os.Lstat(filepath.Join(tree, e.path)); err == nil {
			fmt.Fprintf(h, "%d.%d\x00", fi.Size(), fi.ModTime().UnixNano())
		}
	}
	return strconv.FormatUint(h.Sum64(), 36)
}

// Rev is the stamp of tree's changes now, as git.status's rev.
func (s *Service) Rev(tree string) (string, error) {
	out, err := s.git(tree, nil, readTimeout, porcelainArgs)
	if err != nil {
		return "", err
	}
	return stamp(tree, out, parsePorcelain(out)), nil
}

type cached struct {
	at     time.Time
	status proto.GitStatus
}

// Status is one page of tree's changes, files sorted by path. A page past the
// first comes from the read that served the first, while it is fresh, so
// pages agree.
func (s *Service) Status(tree string, cursor, limit int) (proto.GitStatus, error) {
	if limit <= 0 {
		limit = DefaultStatusPage
	}
	s.mu.Lock()
	c, ok := s.last[tree]
	s.mu.Unlock()
	if cursor == 0 || !ok || time.Since(c.at) > pageFor {
		full, err := s.readStatus(tree)
		if err != nil {
			return proto.GitStatus{}, err
		}
		c = cached{time.Now(), full}
		s.mu.Lock()
		if s.last == nil {
			s.last = map[string]cached{}
		}
		s.last[tree] = c
		s.mu.Unlock()
	}
	page := c.status
	files := page.Files
	lo, hi := min(cursor, len(files)), min(cursor+limit, len(files))
	page.Files = files[lo:hi]
	if hi < len(files) {
		page.NextCursor = &hi
	}
	return proto.NewGitStatus(page), nil
}

// readStatus is every changed file of tree, with its branch, base and the commits since it.
func (s *Service) readStatus(tree string) (proto.GitStatus, error) {
	var (
		out, numOut []byte
		err, numErr error
		wg          sync.WaitGroup
	)
	wg.Go(func() { out, err = s.git(tree, nil, readTimeout, porcelainArgs) })
	wg.Go(func() { numOut, numErr = s.git(tree, nil, readTimeout, []string{"diff", "HEAD", "--numstat", "-z", "-M"}) })
	wg.Wait()
	if err != nil {
		return proto.GitStatus{}, err
	}
	p := parsePorcelain(out)
	if p.unborn() {
		numOut, numErr = s.git(tree, nil, readTimeout, []string{"diff", emptyTree, "--numstat", "-z", "-M"})
	}
	if numErr != nil {
		return proto.GitStatus{}, numErr
	}
	counts := parseNumstat(numOut)
	st := proto.GitStatus{Tree: tree, Rev: stamp(tree, out, p), Upstream: p.upstream, Detached: p.detached()}
	if !p.detached() {
		st.Branch = p.head
	}
	slices.SortFunc(p.entries, func(a, b entry) int { return strings.Compare(a.path, b.path) })
	st.Files = make([]proto.GitFile, 0, len(p.entries))
	for _, e := range p.entries {
		f := proto.GitFile{Path: e.path, OldPath: e.old, Status: e.status(), Staged: e.x != '.' && e.kind != '?' && e.kind != 'u', Unstaged: e.y != '.' || e.kind == 'u'}
		if e.kind == '?' {
			f.Binary = binaryName(e.path)
		} else if c, ok := counts[e.path]; ok && c.binary {
			f.Binary = true
		} else {
			f.Added, f.Removed = new(c.added), new(c.removed)
			st.Totals.Added += c.added
			st.Totals.Removed += c.removed
		}
		if f.Staged {
			st.Totals.Staged++
		}
		st.Files = append(st.Files, f)
	}
	st.Totals.Files = len(st.Files)
	if p.upstream != "" && p.tracked {
		st.Unpushed = new(p.ahead)
	}
	if p.unborn() || st.Branch == "" {
		return st, nil
	}
	st.Base = s.baseFor(tree, st.Branch, p.upstream)
	if st.Base == "" {
		return st, nil
	}
	var log string
	var logErr error
	wg.Go(func() { log, logErr = s.text(tree, "log", "--format=%h%x00%s", "-n", strconv.Itoa(maxCommits), st.Base+"..HEAD", "--") })
	if st.Base == p.upstream && p.tracked {
		st.Ahead, st.Behind = p.ahead, p.behind
	} else if lr, err := s.text(tree, "rev-list", "--left-right", "--count", st.Base+"...HEAD", "--"); err == nil {
		fmt.Sscanf(lr, "%d %d", &st.Behind, &st.Ahead)
	}
	wg.Wait()
	if logErr == nil {
		for l := range strings.SplitSeq(log, "\n") {
			if sha, subject, ok := strings.Cut(l, "\x00"); ok {
				st.Commits = append(st.Commits, proto.CommitRef{Sha: sha, Subject: subject})
			}
		}
	}
	return st, nil
}

// baseFor is the branch's base as the desktop counts against it: its upstream
// on main or master, or when it was created from its own remote twin.
func (s *Service) baseFor(tree, branch, upstream string) string {
	if branch == "main" || branch == "master" {
		return upstream
	}
	b := s.baseOf(tree, branch)
	if strings.TrimPrefix(b, "origin/") == branch {
		return upstream
	}
	return b
}

type baseEntry struct {
	at     time.Time
	reflog time.Time
	base   string
}

// baseOf caches base_of per branch: it moves only with the branch's reflog, or a fetch that makes a candidate appear.
func (s *Service) baseOf(tree, branch string) string {
	var reflog time.Time
	if dir, err := s.commonDir(tree); err == nil {
		if fi, err := os.Stat(filepath.Join(dir, "logs", "refs", "heads", branch)); err == nil {
			reflog = fi.ModTime()
		}
	}
	key := tree + "\x00" + branch
	s.mu.Lock()
	e, ok := s.bases[key]
	s.mu.Unlock()
	if ok && e.reflog.Equal(reflog) && time.Since(e.at) < baseFor {
		return e.base
	}
	base := s.readBase(tree, branch)
	s.mu.Lock()
	if s.bases == nil {
		s.bases = map[string]baseEntry{}
	}
	s.bases[key] = baseEntry{time.Now(), reflog, base}
	s.mu.Unlock()
	return base
}

// readBase is base_of (crates/git/src/git.rs): what the branch was created from
// per its reflog, else origin's default branch, else main or master; origin's
// copy beats a local one, which may be stale.
func (s *Service) readBase(tree, branch string) string {
	exists := func(r string) bool {
		_, err := s.git(tree, nil, readTimeout, []string{"rev-parse", "--verify", "--quiet", r})
		return err == nil
	}
	if log, err := s.text(tree, "log", "-g", "--format=%gs", "refs/heads/"+branch, "--"); err == nil && log != "" {
		lines := strings.Split(log, "\n")
		if from, ok := createdFrom(lines[len(lines)-1]); ok {
			if exists("refs/remotes/origin/" + from) {
				return "origin/" + from
			}
			if exists("refs/heads/" + from) {
				return from
			}
		}
	}
	var candidates []string
	if d, err := s.text(tree, "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"); err == nil && d != "" {
		candidates = append(candidates, d)
	}
	for _, b := range append(candidates, "origin/main", "origin/master", "main", "master") {
		if exists(b) {
			return b
		}
	}
	return ""
}

// createdFrom is the branch a reflog's first entry says it was created from,
// without refs/… or origin/; false when made from HEAD.
func createdFrom(entry string) (string, bool) {
	from, ok := strings.CutPrefix(entry, "branch: Created from ")
	if !ok {
		return "", false
	}
	if f, ok := strings.CutPrefix(from, "refs/remotes/"); ok {
		from = f
	} else {
		from = strings.TrimPrefix(from, "refs/heads/")
	}
	from = strings.TrimPrefix(from, "origin/")
	return from, from != "HEAD"
}

// binaryName guesses from the name alone, since untracked files are never read.
func binaryName(path string) bool {
	switch strings.ToLower(strings.TrimPrefix(filepath.Ext(path), ".")) {
	case "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "heic", "tiff", "pdf", "zip", "gz", "tgz", "xz", "zst", "jar",
		"dylib", "so", "a", "o", "exe", "dll", "wasm", "ttf", "otf", "woff", "woff2", "mp3", "mp4", "mov", "wav", "sqlite", "db":
		return true
	}
	return false
}
