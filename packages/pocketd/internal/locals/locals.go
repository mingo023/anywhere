// Package locals keeps the Locals the user added beside a project's
// Worktrees, in creation order, in state/locals.json. A project's main
// Worktree row is a Local without a record here.
package locals

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/hub"
	"pocketd/internal/names"
	"pocketd/internal/proto"
)

var (
	ErrExists  = errors.New("A Local with this id already exists")
	ErrUnknown = errors.New("This Local was deleted")
	ErrNoName  = errors.New("A Local needs a name")
)

type Locals struct {
	path string
	hub  *hub.Hub

	mu     sync.Mutex
	locals []entry
}

type entry struct {
	proto.Local
	// Auto: Name is a placeholder that naming replaces from the first session started in it; a rename clears it for good.
	Auto bool `json:"auto,omitempty"`
}

func plain(entries []entry) []proto.Local {
	out := make([]proto.Local, len(entries))
	for i, e := range entries {
		out[i] = e.Local
	}
	return out
}

// Open reads home/state/locals.json; a missing or broken file is empty.
func Open(home string) *Locals {
	l := &Locals{path: filepath.Join(home, "state", "locals.json"), hub: hub.New()}
	if raw, err := os.ReadFile(l.path); err == nil && json.Unmarshal(raw, &l.locals) != nil {
		l.locals = nil
	}
	return l
}

func (l *Locals) Subscribe() (<-chan []byte, func()) { return l.hub.Subscribe() }

func (l *Locals) List() []proto.Local {
	l.mu.Lock()
	defer l.mu.Unlock()
	return plain(l.locals)
}

func (l *Locals) Get(id string) (proto.Local, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if i := l.index(id); i >= 0 {
		return l.locals[i].Local, true
	}
	return proto.Local{}, false
}

func (l *Locals) Create(id, project, name string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if name = names.Clip(name); name == "" {
		return ErrNoName
	}
	if l.index(id) >= 0 {
		return ErrExists
	}
	return l.save(append(slices.Clone(l.locals), entry{proto.Local{ID: id, Project: project, Name: name}, true}))
}

// Rename trims title; a blank one changes nothing.
func (l *Locals) Rename(id, title string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	i := l.index(id)
	if i < 0 {
		return ErrUnknown
	}
	if title = names.Clip(title); title == "" {
		return nil
	}
	next := slices.Clone(l.locals)
	next[i].Name, next[i].Auto = title, false
	return l.save(next)
}

// Auto is naming's title for id; false when the user renamed it, naming settled it, or the write failed.
func (l *Locals) Auto(id, title string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	i := l.index(id)
	if i < 0 || !l.locals[i].Auto {
		return false
	}
	if title = names.Clip(title); title == "" {
		return false
	}
	next := slices.Clone(l.locals)
	next[i].Name = title
	return l.save(next) == nil
}

// Settle keeps id's name: naming has named it, so later sessions in it leave it be.
func (l *Locals) Settle(id string) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if i := l.index(id); i >= 0 && l.locals[i].Auto {
		next := slices.Clone(l.locals)
		next[i].Auto = false
		l.save(next)
	}
}

func (l *Locals) Delete(id string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	i := l.index(id)
	if i < 0 {
		return ErrUnknown
	}
	return l.save(slices.Delete(slices.Clone(l.locals), i, i+1))
}

func (l *Locals) index(id string) int {
	return slices.IndexFunc(l.locals, func(x entry) bool { return x.ID == id })
}

// save writes next and only then takes it, so a failed write changes nothing.
func (l *Locals) save(next []entry) error {
	raw, _ := json.Marshal(next)
	err := os.MkdirAll(filepath.Dir(l.path), 0o700)
	if err == nil {
		err = atomicfile.Write(l.path, raw, 0o600)
	}
	if err != nil {
		return err
	}
	l.locals = next
	l.hub.Publish(proto.NewLocalList(plain(next)))
	return nil
}
