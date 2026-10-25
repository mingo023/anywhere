package proto

import (
	"encoding/json"
	"slices"
)

// CapGit gates the git.* verbs and git.changed/gh.changed pushes.
const CapGit = "git.v1"

// CodeGitFailed carries git's first stderr line as the message and all of it as the detail.
const CodeGitFailed = "git_failed"

const (
	GitStatusLimit = 2000
	GitLogLimit    = 200
	// GitWatchLimit bounds the trees one conn has polled; a phone watches what is on screen.
	GitWatchLimit = 32
)

// DiffAt is what a git.diff compares against: working, staged, commit (Rev
// required) or base (Rev optional).
type DiffAt struct {
	Kind string `json:"kind"`
	Rev  string `json:"rev,omitempty"`
}

func decodeDiffAt(raw json.RawMessage, dst **DiffAt) bool {
	var at DiffAt
	if !strictInto(raw, &at, []string{"kind"}, "rev") {
		return false
	}
	switch at.Kind {
	case "working", "staged":
		if at.Rev != "" {
			return false
		}
	case "commit":
		if at.Rev == "" {
			return false
		}
	case "base":
	default:
		return false
	}
	*dst = &at
	return true
}

var gitVerbs = map[string]verbDecoder{
	"git.status": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.optionalInt("cursor", &m.Cursor, 0, maxCount) && f.limit(m, GitStatusLimit)
	},
	"git.watch": func(f fields, m *ClientMessage) bool {
		return f.stringList("trees", &m.Trees) && !slices.Contains(m.Trees, "") && len(m.Trees) <= GitWatchLimit
	},
	"git.diff": func(f fields, m *ClientMessage) bool {
		if !f.text("tree", &m.Tree) || !f.text("path", &m.Path) || !decodeDiffAt(f["at"], &m.At) ||
			!f.optionalBool("ignoreWhitespace", &m.IgnoreWhitespace) || !f.optionalInt("maxLines", &m.MaxLines, 1, maxCount) {
			return false
		}
		if f.has("context") {
			m.Context = new(int)
			return f.integer("context", m.Context, 0, maxCount)
		}
		return true
	},
	"git.log": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.optionalBool("all", &m.All) && f.optionalInt("skip", &m.Skip, 0, maxCount) && f.limit(m, GitLogLimit)
	},
	"git.commit": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.text("sha", &m.Sha)
	},
	"git.stage": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.paths("paths", &m.Paths) && f.get("staged", &m.Staged)
	},
	"git.discard": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.paths("paths", &m.Paths)
	},
	"git.commit.create": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.get("message", &m.Message) && f.optionalBool("amend", &m.Amend) && f.optionalBool("all", &m.All)
	},
	"git.push": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.optionalBool("lease", &m.Lease)
	},
	"git.pull": func(f fields, m *ClientMessage) bool { return f.text("tree", &m.Tree) },
}

type GitTotals struct {
	Files   int `json:"files"`
	Added   int `json:"added"`
	Removed int `json:"removed"`
	Staged  int `json:"staged"`
}

// GitFile is a changed file. Status is M, A, D, R, U (conflict) or ? (untracked);
// Added and Removed are nil for an untracked file.
type GitFile struct {
	Path     string `json:"path"`
	OldPath  string `json:"oldPath,omitempty"`
	Status   string `json:"status"`
	Staged   bool   `json:"staged"`
	Unstaged bool   `json:"unstaged"`
	Added    *int   `json:"added,omitempty"`
	Removed  *int   `json:"removed,omitempty"`
	Binary   bool   `json:"binary,omitempty"`
}

type CommitRef struct {
	Sha     string `json:"sha"`
	Subject string `json:"subject"`
}

// GitStatus is one page of a tree's changes, files sorted by path. Rev is an
// opaque stamp of the porcelain output; Unpushed is nil before the first push.
type GitStatus struct {
	Type       string      `json:"type"`
	ID         string      `json:"id"`
	Tree       string      `json:"tree"`
	Rev        string      `json:"rev"`
	Branch     string      `json:"branch,omitempty"`
	Detached   bool        `json:"detached,omitempty"`
	Upstream   string      `json:"upstream,omitempty"`
	Base       string      `json:"base,omitempty"`
	Ahead      int         `json:"ahead"`
	Behind     int         `json:"behind"`
	Unpushed   *int        `json:"unpushed,omitempty"`
	Totals     GitTotals   `json:"totals"`
	Files      []GitFile   `json:"files"`
	Commits    []CommitRef `json:"commits"`
	NextCursor *int        `json:"nextCursor,omitempty"`
}

// NewGitStatus fills Type and empty lists; the caller sets the rest.
func NewGitStatus(s GitStatus) GitStatus {
	s.Type = "git.status"
	if s.Files == nil {
		s.Files = []GitFile{}
	}
	if s.Commits == nil {
		s.Commits = []CommitRef{}
	}
	return s
}

type GitChanged struct {
	Type string `json:"type"`
	Tree string `json:"tree"`
	Rev  string `json:"rev"`
}

func NewGitChanged(tree, rev string) GitChanged { return GitChanged{"git.changed", tree, rev} }

// GhChanged is pushed to git.watch conns when a watched tree's PR result changes.
type GhChanged struct {
	Type string `json:"type"`
	Tree string `json:"tree"`
}

func NewGhChanged(tree string) GhChanged { return GhChanged{"gh.changed", tree} }

// DiffRow is a git.diff line. K is h (hunk header), c, a or d; O and N the
// old and new line numbers; W the changed byte ranges in T.
type DiffRow struct {
	K   string   `json:"k"`
	O   int      `json:"o,omitempty"`
	N   int      `json:"n,omitempty"`
	T   string   `json:"t"`
	W   [][2]int `json:"w,omitempty"`
	Cut bool     `json:"cut,omitempty"`
}

type Truncated struct {
	Shown int `json:"shown"`
	Total int `json:"total"`
}

type GitDiff struct {
	Type      string     `json:"type"`
	ID        string     `json:"id"`
	Tree      string     `json:"tree"`
	Path      string     `json:"path"`
	OldPath   string     `json:"oldPath,omitempty"`
	Language  string     `json:"language"`
	Added     int        `json:"added"`
	Removed   int        `json:"removed"`
	Binary    bool       `json:"binary,omitempty"`
	TooLarge  bool       `json:"tooLarge,omitempty"`
	Image     bool       `json:"image,omitempty"`
	Lines     []DiffRow  `json:"lines"`
	Truncated *Truncated `json:"truncated,omitempty"`
}

func NewGitDiff(d GitDiff) GitDiff {
	d.Type = "git.diff"
	if d.Lines == nil {
		d.Lines = []DiffRow{}
	}
	return d
}

type LogTips struct {
	Head     string `json:"head,omitempty"`
	Branch   string `json:"branch,omitempty"`
	Upstream string `json:"upstream,omitempty"`
	Base     string `json:"base,omitempty"`
}

// LogCommit's At is the commit time in unix seconds.
type LogCommit struct {
	Sha     string   `json:"sha"`
	Parents []string `json:"parents"`
	Author  string   `json:"author"`
	At      int64    `json:"at"`
	Subject string   `json:"subject"`
}

type GitLog struct {
	Type    string      `json:"type"`
	ID      string      `json:"id"`
	Tips    LogTips     `json:"tips"`
	Commits []LogCommit `json:"commits"`
	More    bool        `json:"more"`
}

func NewGitLog(id string, tips LogTips, commits []LogCommit, more bool) GitLog {
	if commits == nil {
		commits = []LogCommit{}
	}
	for i := range commits {
		if commits[i].Parents == nil {
			commits[i].Parents = []string{}
		}
	}
	return GitLog{"git.log", id, tips, commits, more}
}

type CommitFile struct {
	Path    string `json:"path"`
	OldPath string `json:"oldPath,omitempty"`
	Status  string `json:"status"`
	Added   *int   `json:"added,omitempty"`
	Removed *int   `json:"removed,omitempty"`
}

type GitCommit struct {
	Type    string       `json:"type"`
	ID      string       `json:"id"`
	Sha     string       `json:"sha"`
	Subject string       `json:"subject"`
	Body    string       `json:"body,omitempty"`
	Author  string       `json:"author"`
	At      int64        `json:"at"`
	Files   []CommitFile `json:"files"`
}

func NewGitCommit(c GitCommit) GitCommit {
	c.Type = "git.commit"
	if c.Files == nil {
		c.Files = []CommitFile{}
	}
	return c
}

type GitCommitted struct {
	Type string `json:"type"`
	ID   string `json:"id"`
	Sha  string `json:"sha"`
}

func NewGitCommitted(id, sha string) GitCommitted { return GitCommitted{"git.committed", id, sha} }
