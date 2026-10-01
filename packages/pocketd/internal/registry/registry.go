// Package registry reads the Projects the desktop keeps in desktop.json. The
// desktop is the file's only writer.
package registry

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"sync"

	"pocketd/internal/config"
)

type Repo struct {
	Name      string   `json:"name"`
	Base      string   `json:"base"`
	Worktrees string   `json:"worktrees"`
	Setup     string   `json:"setup"`
	Copy      []string `json:"copy"`
}

type File struct {
	Projects []string        `json:"projects"`
	Repos    map[string]Repo `json:"repos"`
}

// Path is where the desktop writes desktop.json: next to the socket it talks to.
func Path() string { return filepath.Join(filepath.Dir(config.Sock()), "desktop.json") }

// Name is how clients show project.
func (f File) Name(project string) string {
	if n := f.Repos[project].Name; n != "" {
		return n
	}
	return filepath.Base(project)
}

type stamp struct{ mtime, size int64 }

type Registry struct {
	path string

	mu     sync.Mutex
	seen   stamp
	good   File
	broken bool
}

func New(path string) *Registry { return &Registry{path: path} }

// Load stats the file and re-reads it when it changed or the last read failed
// to parse. A missing file is empty; any other failure keeps the last good copy.
func (r *Registry) Load() File {
	r.mu.Lock()
	defer r.mu.Unlock()
	fi, err := os.Stat(r.path)
	if errors.Is(err, fs.ErrNotExist) {
		r.seen, r.good, r.broken = stamp{}, File{}, false
		return r.good
	}
	if err != nil {
		return r.good
	}
	now := stamp{fi.ModTime().UnixNano(), fi.Size()}
	if now == r.seen && !r.broken {
		return r.good
	}
	var f File
	raw, err := os.ReadFile(r.path)
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err != nil {
		if now != r.seen || !r.broken {
			log.Printf("registry: %s: %v", r.path, err)
		}
		r.seen, r.broken = now, true
		return r.good
	}
	r.seen, r.good, r.broken = now, f, false
	return f
}
