// Package locals keeps the Locals the user added beside a project's
// Worktrees, in creation order, in pocketd.db. A project's main
// Worktree row is a Local without a record here.
package locals

import (
	"database/sql"
	"errors"
	"fmt"
	"path/filepath"
	"slices"
	"sync"

	"pocketd/internal/hub"
	"pocketd/internal/names"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

var (
	ErrExists  = errors.New("A Local with this id already exists")
	ErrUnknown = errors.New("This Local was deleted")
	ErrNoName  = errors.New("A Local needs a name")
)

type Locals struct {
	db    *sql.DB
	files func() registry.File
	hub   *hub.Hub

	mu     sync.Mutex
	locals []entry
}

type entry struct {
	proto.Local
	// Auto: Name is a placeholder that naming replaces from the first session started in it; a rename clears it for good.
	Auto bool
}

func plain(entries []entry) []proto.Local {
	out := make([]proto.Local, len(entries))
	for i, e := range entries {
		out[i] = e.Local
	}
	return out
}

// A Local is saved only for a project files lists.
func Open(db *sql.DB, files func() registry.File) (*Locals, error) {
	l := &Locals{db: db, files: files, hub: hub.New()}
	var err error
	if l.locals, err = load(db); err != nil {
		return nil, fmt.Errorf("locals: %w", err)
	}
	return l, nil
}

func load(db *sql.DB) ([]entry, error) {
	rows, err := db.Query(`SELECT l.id, p.path, l.name, l.auto FROM locals l JOIN projects p ON p.id = l.project_id ORDER BY l.position`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []entry
	for rows.Next() {
		var e entry
		if err := rows.Scan(&e.ID, &e.Project, &e.Name, &e.Auto); err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, rows.Err()
}

// Reload takes the Locals the database holds, which differ after a project's removal cascaded, and tells
// subscribers when they did.
func (l *Locals) Reload() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	next, err := load(l.db)
	if err != nil || slices.Equal(next, l.locals) {
		return err
	}
	l.locals = next
	l.hub.Publish(proto.NewLocalList(plain(next)))
	return nil
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
	path, projectID, ok := l.project(project)
	if !ok {
		return registry.ErrUnknownProject
	}
	return l.save(append(slices.Clone(l.locals), entry{proto.Local{ID: id, Project: path, Name: name}, true}), func(tx *sql.Tx) error {
		_, err := tx.Exec(`INSERT INTO locals (id, project_id, name, auto, position) VALUES (?, ?, ?, 1, (SELECT COALESCE(MAX(position) + 1, 0) FROM locals))`, id, projectID, name)
		return err
	})
}

func (l *Locals) project(path string) (string, string, bool) {
	f := l.files()
	for _, p := range f.Projects {
		if filepath.Clean(p) == filepath.Clean(path) {
			return p, f.Repos[p].ID, true
		}
	}
	return "", "", false
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
	return l.save(next, update(next[i]))
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
	return l.save(next, update(next[i])) == nil
}

// Settle keeps id's name: naming has named it, so later sessions in it leave it be.
func (l *Locals) Settle(id string) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if i := l.index(id); i >= 0 && l.locals[i].Auto {
		next := slices.Clone(l.locals)
		next[i].Auto = false
		l.save(next, update(next[i]))
	}
}

func (l *Locals) Delete(id string) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	i := l.index(id)
	if i < 0 {
		return ErrUnknown
	}
	return l.save(slices.Delete(slices.Clone(l.locals), i, i+1), func(tx *sql.Tx) error {
		_, err := tx.Exec(`DELETE FROM locals WHERE id = ?`, id)
		return err
	})
}

func (l *Locals) index(id string) int {
	return slices.IndexFunc(l.locals, func(x entry) bool { return x.ID == id })
}

func update(e entry) func(*sql.Tx) error {
	return func(tx *sql.Tx) error {
		_, err := tx.Exec(`UPDATE locals SET name = ?, auto = ? WHERE id = ?`, e.Name, e.Auto, e.ID)
		return err
	}
}

// A failed write changes nothing.
func (l *Locals) save(next []entry, write func(*sql.Tx) error) error {
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if err := write(tx); err != nil {
		return err
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	l.locals = next
	l.hub.Publish(proto.NewLocalList(plain(next)))
	return nil
}
