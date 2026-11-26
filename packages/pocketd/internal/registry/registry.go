// Package registry keeps the Projects, their settings, the tracked worktrees and
// the shared settings in pocketd.db. pocketd is its only writer; clients change it
// by messages.
package registry

import (
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sync"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

var (
	ErrUnknownProject = errors.New("This folder isn't a project on your Mac")
	ErrNotAFolder     = errors.New("A project is an absolute path to a folder that exists")
)

// Repo is a project's settings; ID is the project's, minted when pocketd first listed it.
type Repo = proto.RepoConfig

type File struct {
	Projects []string        `json:"projects"`
	Repos    map[string]Repo `json:"repos"`
	Worktree Worktree        `json:"worktree"`
	// Tracked is the worktrees the desktop made or imported, by path.
	Tracked []string `json:"tracked"`
	// AgentWorktrees is the worktrees pocketd made for sessions an agent started, by path.
	AgentWorktrees []string `json:"-"`
	Agents         Agents   `json:"agents"`
	Git            Git      `json:"git"`
}

// Git is Settings › Git, which phone commits and pushes follow too.
type Git = proto.GitSettings

type Agents struct {
	// Access is Settings › Permission mode, the access new sessions start with.
	Access string `json:"access"`
}

// Worktree is where new worktrees go.
type Worktree struct {
	Root string `json:"root"`
}

// Name is how clients show project.
func (f File) Name(project string) string {
	if n := f.Repos[project].Name; n != "" {
		return n
	}
	return filepath.Base(project)
}

// State is everything the registry holds.
type State struct {
	Projects       []proto.RegistryProject `json:"projects"`
	Tracked        []string                `json:"tracked"`
	AgentWorktrees []string                `json:"agent_worktrees"`
	Settings       proto.SharedSettings    `json:"settings"`
}

// File is s as readers see it.
func (s State) File() File {
	f := File{
		Repos:          make(map[string]Repo, len(s.Projects)),
		Tracked:        s.Tracked,
		AgentWorktrees: s.AgentWorktrees,
		Worktree:       Worktree{Root: s.Settings.WorktreeRoot},
		Agents:         Agents{Access: s.Settings.Access},
		Git:            s.Settings.Git,
	}
	for _, p := range s.Projects {
		f.Projects = append(f.Projects, p.Path)
		p.Repo.ID = p.ID
		f.Repos[p.Path] = p.Repo
	}
	return f
}

func (s State) index(path string) int {
	return slices.IndexFunc(s.Projects, func(p proto.RegistryProject) bool { return p.Path == path })
}

type Registry struct {
	db  *sql.DB
	hub *hub.Hub

	mu       sync.Mutex
	st       State
	file     File
	onRemove []func(projectID string)
}

// Open loads the registry from db's projects, worktrees and shared_settings. It only reads, so db may be read-only.
func Open(db *sql.DB) (*Registry, error) {
	st, err := load(db)
	if err != nil {
		return nil, fmt.Errorf("registry: %w", err)
	}
	return &Registry{db: db, hub: hub.New(), st: st, file: st.File()}, nil
}

func load(db *sql.DB) (State, error) {
	var s State
	rows, err := db.Query(`SELECT id, path, repo FROM projects ORDER BY position`)
	if err != nil {
		return s, err
	}
	defer rows.Close()
	for rows.Next() {
		var p proto.RegistryProject
		var repo string
		if err := rows.Scan(&p.ID, &p.Path, &repo); err != nil {
			return s, err
		}
		if err := json.Unmarshal([]byte(repo), &p.Repo); err != nil {
			return s, fmt.Errorf("project %s: %w", p.Path, err)
		}
		s.Projects = append(s.Projects, p)
	}
	if err := rows.Err(); err != nil {
		return s, err
	}
	rows.Close()
	if rows, err = db.Query(`SELECT path, kind FROM worktrees ORDER BY position`); err != nil {
		return s, err
	}
	defer rows.Close()
	for rows.Next() {
		var path, kind string
		if err := rows.Scan(&path, &kind); err != nil {
			return s, err
		}
		if kind == kindAgent {
			s.AgentWorktrees = append(s.AgentWorktrees, path)
		} else {
			s.Tracked = append(s.Tracked, path)
		}
	}
	if err := rows.Err(); err != nil {
		return s, err
	}
	rows.Close()
	var commitAll sql.NullBool
	g := &s.Settings.Git
	err = db.QueryRow(`SELECT git_remote, git_commit_all, git_force_with_lease, access, worktree_root FROM shared_settings WHERE id = 1`).
		Scan(&g.Remote, &commitAll, &g.ForceWithLease, &s.Settings.Access, &s.Settings.WorktreeRoot)
	if errors.Is(err, sql.ErrNoRows) {
		return s, nil
	}
	if commitAll.Valid {
		g.CommitAll = &commitAll.Bool
	}
	return s, err
}

const (
	kindTracked = "tracked"
	kindAgent   = "agent"
)

func newID() string {
	var b [8]byte
	rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

// OnRemove hears the id of each project removed, once its removal is committed. The database dropped the
// project's rows with it; f drops them from memory.
func (r *Registry) OnRemove(f func(projectID string)) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.onRemove = append(r.onRemove, f)
}

func (r *Registry) Subscribe() (<-chan []byte, func()) { return r.hub.Subscribe() }

// Load is the registry as readers see it, from memory.
func (r *Registry) Load() File {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.file
}

// List is everything a client with registry.v2 is sent.
func (r *Registry) List() proto.RegistryList {
	r.mu.Lock()
	defer r.mu.Unlock()
	return list(r.st)
}

func list(s State) proto.RegistryList {
	return proto.NewRegistryList(s.Projects, s.Tracked, s.Settings)
}

// AddProject lists the folder at path; one already listed is left as it is.
func (r *Registry) AddProject(path string, repo *Repo) error {
	if fi, err := os.Stat(path); !filepath.IsAbs(path) || err != nil || !fi.IsDir() {
		return ErrNotAFolder
	}
	return r.update(func(s *State) error {
		if s.index(path) >= 0 {
			return nil
		}
		p := proto.RegistryProject{ID: newID(), Path: path}
		if repo != nil {
			p.Repo = *repo
		}
		s.Projects = append(s.Projects, p)
		return nil
	})
}

func (r *Registry) RemoveProject(path string) error {
	return r.update(func(s *State) error {
		if i := s.index(path); i >= 0 {
			s.Projects = slices.Delete(s.Projects, i, i+1)
		}
		return nil
	})
}

// SetRepo replaces the settings of the project at path; its id stays.
func (r *Registry) SetRepo(path string, repo Repo) error {
	return r.update(func(s *State) error {
		i := s.index(path)
		if i < 0 {
			return ErrUnknownProject
		}
		s.Projects[i].Repo = repo
		return nil
	})
}

func (r *Registry) Track(path string, tracked bool) error {
	return r.update(func(s *State) error {
		i := slices.Index(s.Tracked, path)
		switch {
		case tracked && i < 0:
			s.Tracked = append(s.Tracked, path)
		case !tracked && i >= 0:
			s.Tracked = slices.Delete(s.Tracked, i, i+1)
		}
		return nil
	})
}

// AgentWorktree records the worktree at path as one pocketd made for an agent's session, or forgets it.
func (r *Registry) AgentWorktree(path string, made bool) error {
	return r.AgentWorktrees(func(paths []string) []string {
		paths = slices.DeleteFunc(paths, func(p string) bool { return p == path })
		if made {
			paths = append(paths, path)
		}
		return paths
	})
}

// AgentWorktrees replaces the agents' worktrees with what change makes of them.
func (r *Registry) AgentWorktrees(change func([]string) []string) error {
	return r.update(func(s *State) error {
		s.AgentWorktrees = change(s.AgentWorktrees)
		return nil
	})
}

func (r *Registry) SetSettings(settings proto.SharedSettings) error {
	return r.update(func(s *State) error {
		s.Settings = settings
		return nil
	})
}

// update applies change to a copy and saves it, unless it changed nothing. Hooks hear what it removed
// once the lock is let go, so they may read the registry.
func (r *Registry) update(change func(*State) error) error {
	removed, err := r.apply(change)
	if len(removed) == 0 {
		return err
	}
	r.mu.Lock()
	hooks := slices.Clone(r.onRemove)
	r.mu.Unlock()
	for _, id := range removed {
		for _, f := range hooks {
			f(id)
		}
	}
	return err
}

func (r *Registry) apply(change func(*State) error) (removed []string, err error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	next := State{Projects: slices.Clone(r.st.Projects), Tracked: slices.Clone(r.st.Tracked), AgentWorktrees: slices.Clone(r.st.AgentWorktrees), Settings: r.st.Settings}
	if err := change(&next); err != nil {
		return nil, err
	}
	if reflect.DeepEqual(next, r.st) {
		return nil, nil
	}
	for _, p := range r.st.Projects {
		if !slices.ContainsFunc(next.Projects, func(n proto.RegistryProject) bool { return n.ID == p.ID }) {
			removed = append(removed, p.ID)
		}
	}
	if err := r.save(next); err != nil {
		return nil, err
	}
	return removed, nil
}

// save writes what changed from the state before to next, in one transaction, and only then takes next,
// so a failed write changes nothing.
func (r *Registry) save(next State) error {
	if err := r.write(r.st, next); err != nil {
		return err
	}
	before := list(r.st)
	r.st, r.file = next, next.File()
	if after := list(next); !reflect.DeepEqual(after, before) {
		r.hub.Publish(after)
	}
	return nil
}

func (r *Registry) write(prev, next State) error {
	tx, err := r.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if err := writeState(tx, prev, next); err != nil {
		return err
	}
	return tx.Commit()
}

// Insert writes s into a database that holds no registry yet: the import's way in, and tests'.
func Insert(tx *sql.Tx, s State) error { return writeState(tx, State{}, s) }

func writeState(tx *sql.Tx, prev, next State) error {
	if err := writeProjects(tx, prev.Projects, next.Projects); err != nil {
		return err
	}
	if err := writeWorktrees(tx, kindTracked, prev.Tracked, next.Tracked); err != nil {
		return err
	}
	if err := writeWorktrees(tx, kindAgent, prev.AgentWorktrees, next.AgentWorktrees); err != nil {
		return err
	}
	if reflect.DeepEqual(next.Settings, prev.Settings) {
		return nil
	}
	return writeSettings(tx, next.Settings)
}

func writeProjects(tx *sql.Tx, prev, next []proto.RegistryProject) error {
	for _, p := range prev {
		if !slices.ContainsFunc(next, func(n proto.RegistryProject) bool { return n.ID == p.ID }) {
			if _, err := tx.Exec(`DELETE FROM projects WHERE id = ?`, p.ID); err != nil {
				return err
			}
		}
	}
	for i, p := range next {
		if j := slices.IndexFunc(prev, func(o proto.RegistryProject) bool { return o.ID == p.ID }); j == i && reflect.DeepEqual(prev[j], p) {
			continue
		}
		repo, err := json.Marshal(p.Repo)
		if err != nil {
			return err
		}
		// An upsert, never a replace: REPLACE deletes the row first, and the delete would cascade.
		if _, err := tx.Exec(`INSERT INTO projects (id, path, position, repo) VALUES (?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET path = excluded.path, position = excluded.position, repo = excluded.repo`, p.ID, p.Path, i, string(repo)); err != nil {
			return err
		}
	}
	return nil
}

func writeWorktrees(tx *sql.Tx, kind string, prev, next []string) error {
	for _, p := range prev {
		if !slices.Contains(next, p) {
			if _, err := tx.Exec(`DELETE FROM worktrees WHERE path = ? AND kind = ?`, p, kind); err != nil {
				return err
			}
		}
	}
	for i, p := range next {
		if slices.Index(prev, p) == i {
			continue
		}
		if _, err := tx.Exec(`INSERT INTO worktrees (path, kind, position) VALUES (?, ?, ?)
			ON CONFLICT (path, kind) DO UPDATE SET position = excluded.position`, p, kind, i); err != nil {
			return err
		}
	}
	return nil
}

func writeSettings(tx *sql.Tx, s proto.SharedSettings) error {
	_, err := tx.Exec(`INSERT INTO shared_settings (id, git_remote, git_commit_all, git_force_with_lease, access, worktree_root) VALUES (1, ?, ?, ?, ?, ?)
		ON CONFLICT (id) DO UPDATE SET git_remote = excluded.git_remote, git_commit_all = excluded.git_commit_all,
		git_force_with_lease = excluded.git_force_with_lease, access = excluded.access, worktree_root = excluded.worktree_root`,
		s.Git.Remote, s.Git.CommitAll, s.Git.ForceWithLease, s.Access, s.WorktreeRoot)
	return err
}
