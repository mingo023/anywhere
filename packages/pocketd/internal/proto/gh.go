package proto

import "slices"

// CapGitHub gates the gh.* verbs.
const CapGitHub = "gh.v1"

const (
	CodeGhFailed    = "gh_failed"
	CodeGhMissing   = "gh_missing"
	CodeGhSignedOut = "gh_signed_out"
)

var MergeMethods = []string{"squash", "merge", "rebase"}

var ghVerbs = map[string]verbDecoder{
	"gh.status": func(f fields, m *ClientMessage) bool { return f.optionalString("tree", &m.Tree) },
	"gh.pr":     func(f fields, m *ClientMessage) bool { return f.text("tree", &m.Tree) },
	"gh.prs":    func(f fields, m *ClientMessage) bool { return f.text("project", &m.Project) },
	"gh.pr.create": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.text("title", &m.Title) && f.get("body", &m.Body) &&
			f.optionalBool("draft", &m.Draft) && f.optionalString("base", &m.Base)
	},
	"gh.pr.merge": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.integer("number", &m.Number, 1, maxCount) && f.get("method", &m.Method) && slices.Contains(MergeMethods, m.Method)
	},
	"gh.pr.ready": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.integer("number", &m.Number, 1, maxCount)
	},
	"gh.thread.reply": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.text("thread", &m.Thread) && f.text("body", &m.Body)
	},
	"gh.thread.resolve": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.text("thread", &m.Thread) && f.get("resolved", &m.Resolved)
	},
}

// GhStatus.State is missing, signedOut or ready.
type GhStatus struct {
	Type  string `json:"type"`
	ID    string `json:"id"`
	State string `json:"state"`
	Login string `json:"login,omitempty"`
}

func NewGhStatus(id, state, login string) GhStatus { return GhStatus{"gh.status", id, state, login} }

type Checks struct {
	Passed  int `json:"passed"`
	Failed  int `json:"failed"`
	Pending int `json:"pending"`
}

// CheckRun.Outcome is passed, failed, pending or skipped.
type CheckRun struct {
	Name    string `json:"name"`
	Outcome string `json:"outcome"`
	URL     string `json:"url,omitempty"`
}

type Review struct {
	Login string `json:"login"`
	State string `json:"state"`
}

// ThreadComment's At is unix seconds; Hunk the diff lines it sits under.
type ThreadComment struct {
	Author string `json:"author"`
	Body   string `json:"body"`
	At     int64  `json:"at"`
	Hunk   string `json:"hunk,omitempty"`
}

type Thread struct {
	ID       string          `json:"id"`
	Path     string          `json:"path"`
	Line     int             `json:"line,omitempty"`
	Left     bool            `json:"left,omitempty"`
	Outdated bool            `json:"outdated"`
	Resolved bool            `json:"resolved"`
	Comments []ThreadComment `json:"comments"`
}

// PR is a pull request as the desktop's Pr shows it; Gate is Pr::gate in
// packages/desktop/crates/git/src/github.rs.
type PR struct {
	Number    int        `json:"number"`
	Title     string     `json:"title"`
	URL       string     `json:"url"`
	State     string     `json:"state"`
	Draft     bool       `json:"draft"`
	Base      string     `json:"base"`
	Head      string     `json:"head"`
	Checks    Checks     `json:"checks"`
	Runs      []CheckRun `json:"runs"`
	Review    string     `json:"review,omitempty"`
	Reviews   []Review   `json:"reviews"`
	Requested []string   `json:"requested"`
	Mergeable string     `json:"mergeable,omitempty"`
	Gate      string     `json:"gate"`
	Threads   []Thread   `json:"threads"`
}

// GhPR has neither PR nor Problem when the branch has no PR.
type GhPR struct {
	Type    string `json:"type"`
	ID      string `json:"id"`
	Tree    string `json:"tree"`
	Problem string `json:"problem,omitempty"`
	PR      *PR    `json:"pr,omitempty"`
}

func NewGhPR(id, tree, problem string, pr *PR) GhPR {
	if pr != nil {
		p := *pr
		p.Runs = orEmpty(p.Runs)
		p.Reviews = orEmpty(p.Reviews)
		p.Requested = orEmpty(p.Requested)
		p.Threads = orEmpty(p.Threads)
		for i := range p.Threads {
			p.Threads[i].Comments = orEmpty(p.Threads[i].Comments)
		}
		pr = &p
	}
	return GhPR{"gh.pr", id, tree, problem, pr}
}

type PRItem struct {
	Tree   string `json:"tree"`
	Number int    `json:"number"`
	Title  string `json:"title"`
	State  string `json:"state"`
	Draft  bool   `json:"draft"`
	Gate   string `json:"gate"`
	Checks Checks `json:"checks"`
}

type GhPRs struct {
	Type  string   `json:"type"`
	ID    string   `json:"id"`
	Items []PRItem `json:"items"`
}

func NewGhPRs(id string, items []PRItem) GhPRs { return GhPRs{"gh.prs", id, orEmpty(items)} }

type GhPRCreated struct {
	Type   string `json:"type"`
	ID     string `json:"id"`
	Number int    `json:"number"`
	URL    string `json:"url"`
}

func NewGhPRCreated(id string, number int, url string) GhPRCreated {
	return GhPRCreated{"gh.pr.created", id, number, url}
}

// orEmpty is list, or an empty one so it marshals as [] rather than null.
func orEmpty[T any](list []T) []T {
	if list == nil {
		return []T{}
	}
	return list
}
