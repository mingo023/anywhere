// Package registry keeps the Projects, their settings, the tracked worktrees and
// the shared settings in state/registry.json. pocketd is its only writer; clients
// change it by messages. desktop.json is read once, to fill the file the first time.
package registry

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"sync"

	"pocketd/internal/atomicfile"
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
	Sidebar Sidebar  `json:"sidebar"`
	Agents  Agents   `json:"agents"`
	Git     Git      `json:"git"`
}

// Git is Settings › Git, which phone commits and pushes follow too.
type Git = proto.GitSettings

type Agents struct {
	// Access is Settings › Permission mode, the access new sessions start with.
	Access string `json:"access"`
}

type Sidebar struct {
	// ExternalWorktrees lists the worktrees Anywhere didn't make too.
	ExternalWorktrees bool `json:"external_worktrees"`
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

type state struct {
	Projects []proto.RegistryProject `json:"projects"`
	Tracked  []string                `json:"tracked"`
	Settings proto.SharedSettings    `json:"settings"`
}

func (s state) file() File {
	f := File{
		Repos:    make(map[string]Repo, len(s.Projects)),
		Tracked:  s.Tracked,
		Worktree: Worktree{Root: s.Settings.WorktreeRoot},
		Sidebar:  Sidebar{ExternalWorktrees: s.Settings.ExternalWorktrees},
		Agents:   Agents{Access: s.Settings.Access},
		Git:      s.Settings.Git,
	}
	for _, p := range s.Projects {
		f.Projects = append(f.Projects, p.Path)
		p.Repo.ID = p.ID
		f.Repos[p.Path] = p.Repo
	}
	return f
}

func (s state) index(path string) int {
	return slices.IndexFunc(s.Projects, func(p proto.RegistryProject) bool { return p.Path == path })
}

type Registry struct {
	path string
	hub  *hub.Hub

	mu   sync.Mutex
	st   state
	file File
}

// Open reads home/state/registry.json. While that file is missing it is made from home/desktop.json, once:
// the projects there that aren't another host's ("//host/..." paths), their settings, the tracked worktrees and
// the shared settings, each project given an id. A broken file is kept beside it as registry.json.broken and
// made again the same way.
func Open(home string) *Registry {
	r := &Registry{path: filepath.Join(home, "state", "registry.json"), hub: hub.New()}
	raw, err := os.ReadFile(r.path)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		r.st = importDesktop(filepath.Join(home, "desktop.json"))
		if err := r.write(r.st); err != nil {
			log.Printf("registry: %v", err)
		}
	case err != nil || json.Unmarshal(raw, &r.st) != nil:
		// Kept aside rather than served empty, which a client would take as every project removed.
		log.Printf("registry: %s unreadable, importing again: %v", r.path, err)
		os.Rename(r.path, r.path+".broken")
		r.st = importDesktop(filepath.Join(home, "desktop.json"))
		if err := r.write(r.st); err != nil {
			log.Printf("registry: %v", err)
		}
	}
	r.file = r.st.file()
	return r
}

func importDesktop(path string) state {
	var f File
	raw, err := os.ReadFile(path)
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err != nil {
		if !errors.Is(err, fs.ErrNotExist) {
			log.Printf("registry: %s: %v", path, err)
		}
		return state{}
	}
	local := func(p string) bool { return !strings.HasPrefix(p, "//") }
	s := state{Settings: proto.SharedSettings{Git: f.Git, Access: f.Agents.Access, WorktreeRoot: f.Worktree.Root, ExternalWorktrees: f.Sidebar.ExternalWorktrees}}
	for _, p := range f.Projects {
		if local(p) && s.index(p) < 0 {
			s.Projects = append(s.Projects, proto.RegistryProject{ID: newID(), Path: p, Repo: f.Repos[p]})
		}
	}
	s.Tracked = slices.DeleteFunc(slices.Clone(f.Tracked), func(p string) bool { return !local(p) })
	return s
}

func newID() string {
	var b [8]byte
	rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func (r *Registry) Subscribe() (<-chan []byte, func()) { return r.hub.Subscribe() }

// Load is the registry as readers see it, without reading a file.
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

func list(s state) proto.RegistryList {
	return proto.NewRegistryList(s.Projects, s.Tracked, s.Settings)
}

// AddProject lists the folder at path; one already listed is left as it is.
func (r *Registry) AddProject(path string, repo *Repo) error {
	if fi, err := os.Stat(path); !filepath.IsAbs(path) || err != nil || !fi.IsDir() {
		return ErrNotAFolder
	}
	return r.update(func(s *state) error {
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
	return r.update(func(s *state) error {
		if i := s.index(path); i >= 0 {
			s.Projects = slices.Delete(s.Projects, i, i+1)
		}
		return nil
	})
}

// SetRepo replaces the settings of the project at path; its id stays.
func (r *Registry) SetRepo(path string, repo Repo) error {
	return r.update(func(s *state) error {
		i := s.index(path)
		if i < 0 {
			return ErrUnknownProject
		}
		s.Projects[i].Repo = repo
		return nil
	})
}

func (r *Registry) Track(path string, tracked bool) error {
	return r.update(func(s *state) error {
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

func (r *Registry) SetSettings(settings proto.SharedSettings) error {
	return r.update(func(s *state) error {
		s.Settings = settings
		return nil
	})
}

// update applies change to a copy and saves it, unless it changed nothing.
func (r *Registry) update(change func(*state) error) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	next := state{Projects: slices.Clone(r.st.Projects), Tracked: slices.Clone(r.st.Tracked), Settings: r.st.Settings}
	if err := change(&next); err != nil {
		return err
	}
	if reflect.DeepEqual(next, r.st) {
		return nil
	}
	return r.save(next)
}

// save writes next and only then takes it, so a failed write changes nothing.
func (r *Registry) save(next state) error {
	if err := r.write(next); err != nil {
		return err
	}
	r.st, r.file = next, next.file()
	r.hub.Publish(list(next))
	return nil
}

func (r *Registry) write(s state) error {
	raw, _ := json.Marshal(s)
	if err := os.MkdirAll(filepath.Dir(r.path), 0o700); err != nil {
		return err
	}
	return atomicfile.Write(r.path, raw, 0o600)
}
