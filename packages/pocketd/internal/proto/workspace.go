package proto

import "pocketd/internal/providers"

// CapWorkspace gates worktree.create/delete, branch.list, account.list,
// agent.account and project.list pushes.
const CapWorkspace = "workspace.v1"

// CapWorktrees gates worktree.inspect and worktree.delete's trash, teardownSecs, teardown and branch.
const CapWorktrees = "worktrees.v1"

// CodeTeardownFailed: a worktree's teardown script failed, so nothing was deleted; Path names the worktree, Detail
// holds the end of the script's output.
const CodeTeardownFailed = "teardown_failed"

// CodeHostOutdated: a linked host's pocketd can't inspect or delete its worktrees for this one.
const CodeHostOutdated = "host_outdated"

// CodeMainWorktree: the main worktree can't be deleted.
const CodeMainWorktree = "main_worktree"

// CodeUnpushedCommits: deleting the branch would lose commits no other branch, remote or tag holds; Count says how many.
const CodeUnpushedCommits = "unpushed_commits"

var workspaceVerbs = map[string]verbDecoder{
	"worktree.create": func(f fields, m *ClientMessage) bool {
		if !f.requestID(m) || !f.text("project", &m.Project) || !f.get("name", &m.Name) ||
			!f.optionalString("base", &m.Base) || !f.optionalString("branch", &m.Branch) ||
			!f.optionalInt("pr", &m.Number, 1, maxCount) || !f.optionalFlag("setup", &m.Setup) {
			return false
		}
		set := 0
		for _, has := range []bool{m.Base != "", m.Branch != "", m.Number != 0} {
			if has {
				set++
			}
		}
		return set <= 1 && (m.Name != "" || m.Branch != "" || m.Number != 0)
	},
	"worktree.delete": func(f fields, m *ClientMessage) bool {
		return f.text("path", &m.Path) && f.optionalBool("deleteBranch", &m.DeleteBranch) && f.optionalBool("force", &m.Force) && f.optionalList("subagents", &m.Subagents) &&
			f.optionalBool("trash", &m.Trash) && f.optionalInt("teardownSecs", &m.TeardownSecs, 1, 3600) && f.optionalFlag("teardown", &m.Teardown) &&
			f.optionalString("branch", &m.Branch)
	},
	"worktree.inspect": func(f fields, m *ClientMessage) bool { return f.text("path", &m.Path) },
	"branch.list":      func(f fields, m *ClientMessage) bool { return f.text("tree", &m.Tree) },
	"account.list": func(f fields, m *ClientMessage) bool {
		return f.optionalString("provider", &m.Provider) && (!f.has("provider") || providers.Get(m.Provider) != nil)
	},
	"agent.account": func(f fields, m *ClientMessage) bool {
		return f.get("agentId", &m.AgentID) && f.text("account", &m.Account)
	},
}

// InspectedTree is one worktree a worktree.delete would take: what deleting it closes and loses. Host is the linked
// host's id, and Path that host's own path, for a subagent's worktree there.
type InspectedTree struct {
	Host      string `json:"host,omitempty"`
	Path      string `json:"path"`
	Branch    string `json:"branch"`
	Dirty     int    `json:"dirty"`
	Terminals int    `json:"terminals"`
}

// WorktreeInspect is what the delete sheet shows: the worktree first, then its subagents'. Lost counts the commits
// lost when every branch goes, LostAnyway those only the subagents' detached HEADs hold.
type WorktreeInspect struct {
	Type       string          `json:"type"`
	ID         string          `json:"id"`
	Trees      []InspectedTree `json:"trees"`
	Lost       int             `json:"lost"`
	LostAnyway int             `json:"lostAnyway"`
}

func NewWorktreeInspect(id string, trees []InspectedTree, lost, lostAnyway int) WorktreeInspect {
	return WorktreeInspect{"worktree.inspect", id, trees, lost, lostAnyway}
}

type WorktreeProgress struct {
	Type      string `json:"type"`
	ID        string `json:"id"`
	RequestID string `json:"requestId"`
	Step      string `json:"step"`
	Note      string `json:"note,omitempty"`
}

// NewWorktreeProgress is the step a worktree.create is on, named as AgentProgress's.
func NewWorktreeProgress(id, requestID, step, note string) WorktreeProgress {
	return WorktreeProgress{"worktree.progress", id, requestID, step, note}
}

type WorktreeCreated struct {
	Type      string `json:"type"`
	ID        string `json:"id"`
	RequestID string `json:"requestId"`
	Path      string `json:"path"`
	Branch    string `json:"branch"`
}

func NewWorktreeCreated(id, requestID, path, branch string) WorktreeCreated {
	return WorktreeCreated{"worktree.created", id, requestID, path, branch}
}

// Branch is a branch and when its tip was committed, in unix seconds.
type Branch struct {
	Name string `json:"name"`
	At   int64  `json:"at"`
}

// BranchList holds each side newest first, at most 200 each.
type BranchList struct {
	Type    string   `json:"type"`
	ID      string   `json:"id"`
	Current string   `json:"current,omitempty"`
	Local   []Branch `json:"local"`
	Remote  []Branch `json:"remote"`
}

func NewBranchList(id, current string, local, remote []Branch) BranchList {
	if local == nil {
		local = []Branch{}
	}
	if remote == nil {
		remote = []Branch{}
	}
	return BranchList{"branch.list", id, current, local, remote}
}

// Usage is one limit window: Pct used of 100, and when it resets in unix seconds.
type Usage struct {
	Pct      float64 `json:"pct"`
	ResetsAt int64   `json:"resetsAt,omitempty"`
}

type Account struct {
	ID        string `json:"id"`
	Provider  string `json:"provider"`
	Name      string `json:"name"`
	Plan      string `json:"plan,omitempty"`
	Email     string `json:"email,omitempty"`
	SignedIn  bool   `json:"signedIn"`
	IsDefault bool   `json:"isDefault,omitempty"`
	FiveHour  *Usage `json:"fiveHour,omitempty"`
	Week      *Usage `json:"week,omitempty"`
}

type AccountList struct {
	Type     string    `json:"type"`
	ID       string    `json:"id"`
	Accounts []Account `json:"accounts"`
}

func NewAccountList(id string, accounts []Account) AccountList {
	if accounts == nil {
		accounts = []Account{}
	}
	return AccountList{"account.list", id, accounts}
}
