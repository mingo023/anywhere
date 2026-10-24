// Package workspace serves what a phone needs to manage a Mac's projects:
// the project list, cached and pushed on change, branches and accounts.
package workspace

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/worktree"
)

// Poll is how often Run looks for a changed registry or worktree set.
const Poll = 2 * time.Second

// Projects caches the project list and publishes it whole when it changes,
// so no client waits on git to read it.
type Projects struct {
	Registry *registry.Registry
	hub      *hub.Hub
	kick     chan struct{}
	// scan keeps one Scan at a time, so an older list never lands after a newer one.
	scan sync.Mutex

	mu    sync.Mutex
	ready bool
	list  []proto.Project
	trees map[string]cached
}

// cached is one project's worktrees as of stamp, what its git folder looked like.
type cached struct {
	stamp string
	trees []proto.Worktree
}

func NewProjects(reg *registry.Registry) *Projects {
	return &Projects{Registry: reg, hub: hub.New(), kick: make(chan struct{}, 1), trees: map[string]cached{}}
}

func (p *Projects) Subscribe() (<-chan []byte, func()) { return p.hub.Subscribe() }

// List is the cached list; false until the first scan finished.
func (p *Projects) List() ([]proto.Project, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.list, p.ready
}

// Changed asks Run to scan now: a worktree was just made or removed.
func (p *Projects) Changed() {
	select {
	case p.kick <- struct{}{}:
	default:
	}
}

// Run scans at once, then every Poll and on each Changed, until ctx ends.
func (p *Projects) Run(ctx context.Context) {
	tick := time.NewTicker(Poll)
	defer tick.Stop()
	for {
		p.Scan()
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		case <-p.kick:
		}
	}
}

// Scan re-lists only the projects whose git folder changed since the last
// scan, and publishes the list when anything in it moved.
func (p *Projects) Scan() {
	p.scan.Lock()
	defer p.scan.Unlock()
	f := p.Registry.Load()
	p.mu.Lock()
	trees := p.trees
	p.mu.Unlock()
	next := make(map[string]cached, len(f.Projects))
	list := []proto.Project{}
	for _, path := range f.Projects {
		s := stamp(path)
		if s == "" {
			continue
		}
		c, ok := trees[path]
		if !ok || c.stamp != s {
			ws, err := worktree.List(path)
			if err != nil {
				continue
			}
			c = cached{stamp: s, trees: make([]proto.Worktree, len(ws))}
			for i, w := range ws {
				c.trees[i] = proto.Worktree{Name: w.Name, Path: w.Path, Branch: w.Branch, IsMain: w.Main}
			}
		}
		next[path] = c
		list = append(list, proto.Project{Path: path, Name: f.Name(path), Worktrees: c.trees})
	}
	p.mu.Lock()
	changed := !p.ready || !reflect.DeepEqual(list, p.list)
	p.list, p.trees, p.ready = list, next, true
	p.mu.Unlock()
	if changed {
		p.hub.Publish(proto.NewProjectList("", list))
	}
}

// stamp is what changes when git adds, removes or switches a worktree of the
// project at path: the times of its git folder's HEAD, its worktrees folder
// and each linked worktree's HEAD. "" when the folder is gone. A folder that
// isn't a repository stamps as itself, so it's listed once.
func stamp(path string) string {
	fi, err := os.Stat(path)
	if err != nil || !fi.IsDir() {
		return ""
	}
	git := filepath.Join(path, ".git")
	gi, err := os.Stat(git)
	if err != nil {
		return "dir"
	}
	if !gi.IsDir() {
		// A gitfile: the project is itself a linked worktree or a submodule; read its HEAD's folder.
		raw, err := os.ReadFile(git)
		dir, ok := strings.CutPrefix(strings.TrimSpace(string(raw)), "gitdir: ")
		if err != nil || !ok {
			return "gitfile"
		}
		if !filepath.IsAbs(dir) {
			dir = filepath.Join(path, dir)
		}
		git = dir
	}
	var b strings.Builder
	at := func(p string) {
		if fi, err := os.Stat(p); err == nil {
			b.WriteString(strconv.FormatInt(fi.ModTime().UnixNano(), 36))
		}
		b.WriteByte(';')
	}
	at(filepath.Join(git, "HEAD"))
	linked := filepath.Join(git, "worktrees")
	at(linked)
	entries, _ := os.ReadDir(linked)
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		names = append(names, e.Name())
	}
	slices.Sort(names)
	for _, n := range names {
		b.WriteString(n)
		at(filepath.Join(linked, n, "HEAD"))
	}
	return b.String()
}
