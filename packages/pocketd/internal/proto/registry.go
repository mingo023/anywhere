package proto

import "encoding/json"

// CapRegistryV2 gates registry.list and project.add, project.remove, project.set, worktree.track and
// settings.shared: pocketd owns the project list, each project's settings, the tracked worktrees and the shared settings.
const CapRegistryV2 = "registry.v2"

// ClientDesktop is the clientId of the desktop app, which must speak CapRegistryV2.
const ClientDesktop = "desktop"

// RepoConfig is a project's settings as the desktop serializes them. Launch and Icon are the desktop's own
// shapes, kept as raw JSON.
type RepoConfig struct {
	// ID is the project's; the wire carries it beside the path, not inside the settings.
	ID        string            `json:"-"`
	Name      string            `json:"name"`
	Color     uint32            `json:"color"`
	Setup     string            `json:"setup"`
	Teardown  string            `json:"teardown"`
	Launch    json.RawMessage   `json:"launch,omitempty"`
	DevURL    string            `json:"dev_url"`
	Icon      json.RawMessage   `json:"icon"`
	GhAccount string            `json:"gh_account"`
	Accounts  map[string]string `json:"accounts"`
	Hidden    []string          `json:"hidden"`
}

// GitSettings is Settings › Git, which phone commits and pushes follow too.
type GitSettings struct {
	// Remote is where pushes go, when the repository has a remote by that name.
	Remote string `json:"remote"`
	// CommitAll: with nothing staged, a commit takes every change. Nil is the desktop's default, on.
	CommitAll *bool `json:"commit_all"`
	// ForceWithLease lets a push replace a rewritten branch on the remote; a phone asks first.
	ForceWithLease bool `json:"force_with_lease"`
}

// PushRemote is Remote, or the desktop's default.
func (g GitSettings) PushRemote() string {
	if g.Remote == "" {
		return "origin"
	}
	return g.Remote
}

func (g GitSettings) CommitsAll() bool { return g.CommitAll == nil || *g.CommitAll }

// SharedSettings are the desktop settings a host's pocketd reads too.
type SharedSettings struct {
	Git GitSettings `json:"git"`
	// Access is the permission mode new sessions start with.
	Access string `json:"access"`
	// WorktreeRoot is where new worktrees go.
	WorktreeRoot string `json:"worktree_root"`
}

type RegistryProject struct {
	ID   string     `json:"id"`
	Path string     `json:"path"`
	Repo RepoConfig `json:"repo"`
}

// RegistryList is the whole shared state, sent on hello and on each change.
type RegistryList struct {
	Type     string            `json:"type"`
	Projects []RegistryProject `json:"projects"`
	Tracked  []string          `json:"tracked"`
	Settings SharedSettings    `json:"settings"`
}

// NewRegistryList writes empty collections as [] and {}; an unset launch stays absent, which the desktop reads as its default.
func NewRegistryList(projects []RegistryProject, tracked []string, settings SharedSettings) RegistryList {
	out := make([]RegistryProject, len(projects))
	for i, p := range projects {
		if p.Repo.Accounts == nil {
			p.Repo.Accounts = map[string]string{}
		}
		if p.Repo.Hidden == nil {
			p.Repo.Hidden = []string{}
		}
		out[i] = p
	}
	if tracked == nil {
		tracked = []string{}
	}
	return RegistryList{"registry.list", out, tracked, settings}
}

func (f fields) repo(dst **RepoConfig) bool {
	var r RepoConfig
	if !f.get("repo", &r) {
		return false
	}
	if string(r.Launch) == "null" {
		r.Launch = nil
	}
	*dst = &r
	return true
}

var registryVerbs = map[string]verbDecoder{
	"project.add": func(f fields, m *ClientMessage) bool {
		return f.text("path", &m.Path) && (!f.has("repo") || f.repo(&m.Repo))
	},
	"project.remove": func(f fields, m *ClientMessage) bool { return f.text("path", &m.Path) },
	"project.set": func(f fields, m *ClientMessage) bool {
		return f.text("path", &m.Path) && f.repo(&m.Repo)
	},
	"worktree.track": func(f fields, m *ClientMessage) bool {
		return f.text("path", &m.Path) && f.get("tracked", &m.Tracked)
	},
	"settings.shared": func(f fields, m *ClientMessage) bool {
		var s SharedSettings
		if !f.get("settings", &s) {
			return false
		}
		m.Settings = &s
		return true
	},
}
