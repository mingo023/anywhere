package gitview

import (
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/proto"
)

// DefaultLogPage is the desktop graph's PAGE.
const DefaultLogPage = 50

type ref struct{ name, sha, upstream string }

// tips are what the graph follows (git.rs tips): HEAD, the checked-out branch,
// its upstream and its base; revs are where the history starts, each once.
func (s *Service) tips(tree string, all bool) (tips proto.LogTips, revs []string, ok bool) {
	head, err := s.text(tree, "rev-parse", "--verify", "-q", "HEAD")
	if err != nil {
		return tips, nil, false
	}
	out, _ := s.text(tree, "for-each-ref", "--format=%(refname)%00%(objectname)%00%(upstream)", "refs/heads", "refs/remotes")
	var refs []ref
	for l := range strings.SplitSeq(out, "\n") {
		if p := strings.SplitN(l, "\x00", 3); len(p) == 3 {
			refs = append(refs, ref{p[0], p[1], p[2]})
		}
	}
	find := func(name string) (ref, bool) {
		i := slices.IndexFunc(refs, func(r ref) bool { return r.name == name })
		if i < 0 {
			return ref{}, false
		}
		return refs[i], true
	}
	tips.Head = head
	revs = []string{head}
	add := func(sha string) {
		if sha != "" && !slices.Contains(revs, sha) {
			revs = append(revs, sha)
		}
	}
	current, _ := s.text(tree, "symbolic-ref", "-q", "HEAD")
	name := strings.TrimPrefix(current, "refs/heads/")
	if b, ok := find(current); ok {
		tips.Branch = name
		if u, ok := find(b.upstream); ok {
			tips.Upstream = u.sha
			add(u.sha)
		}
	}
	if name != "" && name != "main" && name != "master" {
		if base := s.baseOf(tree, name); base != "" && strings.TrimPrefix(base, "origin/") != name {
			b, ok := find("refs/remotes/" + base)
			if !ok {
				b, ok = find("refs/heads/" + base)
			}
			if ok {
				tips.Base = b.sha
				add(b.sha)
			}
		}
	}
	if all {
		for _, r := range refs {
			add(r.sha)
		}
	}
	return tips, revs, true
}

// Log is a page of history from the tips, children before parents (git.rs log_graph).
func (s *Service) Log(tree string, all bool, skip, limit int) (proto.LogTips, []proto.LogCommit, bool, error) {
	if limit <= 0 {
		limit = DefaultLogPage
	}
	tips, revs, ok := s.tips(tree, all)
	if !ok {
		if _, err := s.commonDir(tree); err != nil {
			return tips, nil, false, err
		}
		return tips, nil, false, nil
	}
	args := []string{"log", "--format=%H%x00%P%x00%aN%x00%ct%x00%s", "--topo-order", "--skip=" + strconv.Itoa(skip), "-n" + strconv.Itoa(limit+1)}
	out, err := s.text(tree, append(append(args, revs...), "--")...)
	if err != nil {
		return tips, nil, false, err
	}
	var commits []proto.LogCommit
	for l := range strings.SplitSeq(out, "\n") {
		p := strings.SplitN(l, "\x00", 5)
		if len(p) != 5 {
			continue
		}
		at, _ := strconv.ParseInt(p[3], 10, 64)
		commits = append(commits, proto.LogCommit{Sha: p[0], Parents: strings.Fields(p[1]), Author: p[2], At: at, Subject: p[4]})
	}
	more := len(commits) > limit
	if more {
		commits = commits[:limit]
	}
	return tips, commits, more, nil
}

// parseNameStatus reads `diff-tree -r -z -M --name-status` (git.rs commit_files).
func parseNameStatus(out []byte) []proto.CommitFile {
	var files []proto.CommitFile
	recs := fields(out)
	for i := 0; i+1 < len(recs); i += 2 {
		status := recs[i]
		if status == "" {
			i--
			continue
		}
		f := proto.CommitFile{Path: recs[i+1], Status: status[:1]}
		if status[0] == 'R' || status[0] == 'C' {
			if i+2 >= len(recs) {
				break
			}
			f.OldPath, f.Path = recs[i+1], recs[i+2]
			i++
		}
		switch f.Status {
		case "C":
			f.Status = "A"
		case "T":
			f.Status = "M"
		}
		files = append(files, f)
	}
	return files
}

// Commit is one commit and the files it changed against its first parent.
func (s *Service) Commit(tree, rev string) (proto.GitCommit, error) {
	sha, err := s.commitSha(tree, rev)
	if err != nil {
		return proto.GitCommit{}, err
	}
	head, err := s.text(tree, "show", "-s", "--format=%H%x00%aN%x00%ct%x00%s%x00%b", sha)
	if err != nil {
		return proto.GitCommit{}, err
	}
	p := strings.SplitN(head, "\x00", 5)
	if len(p) != 5 {
		return proto.GitCommit{}, &Failed{Message: "Not a commit: " + rev}
	}
	at, _ := strconv.ParseInt(p[2], 10, 64)
	c := proto.GitCommit{Sha: p[0], Author: p[1], At: at, Subject: p[3], Body: strings.TrimSpace(p[4])}
	parent := s.parentOf(tree, sha)
	names, err := s.git(tree, nil, readTimeout, []string{"diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id", parent, sha})
	if err != nil {
		return c, err
	}
	nums, err := s.git(tree, nil, readTimeout, []string{"diff-tree", "-r", "-z", "-M", "--numstat", "--no-commit-id", parent, sha})
	if err != nil {
		return c, err
	}
	counts := parseNumstat(nums)
	c.Files = parseNameStatus(names)
	for i := range c.Files {
		if n, ok := counts[c.Files[i].Path]; ok && !n.binary {
			c.Files[i].Added, c.Files[i].Removed = new(n.added), new(n.removed)
		}
	}
	return proto.NewGitCommit(c), nil
}
