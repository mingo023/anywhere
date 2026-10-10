// Package names keeps each Worktree's display name, by path, in state/names.json.
package names

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

// MaxTitle is the longest display name, in runes.
const MaxTitle = 150

type Entry struct {
	Title string `json:"title"`
	// Auto: naming set Title and may set it again; a rename clears it for good.
	Auto bool `json:"auto"`
}

type Names struct {
	path string
	hub  *hub.Hub

	mu      sync.Mutex
	entries map[string]Entry
}

// Open reads home/state/names.json; a missing or broken file is empty.
func Open(home string) *Names {
	n := &Names{path: filepath.Join(home, "state", "names.json"), hub: hub.New(), entries: map[string]Entry{}}
	if raw, err := os.ReadFile(n.path); err == nil && json.Unmarshal(raw, &n.entries) != nil {
		n.entries = map[string]Entry{}
	}
	return n
}

func (n *Names) Subscribe() (<-chan []byte, func()) { return n.hub.Subscribe() }

// Titles is every non-empty display name, by path.
func (n *Names) Titles() map[string]string {
	n.mu.Lock()
	defer n.mu.Unlock()
	return n.titles()
}

func (n *Names) titles() map[string]string {
	out := map[string]string{}
	for p, e := range n.entries {
		if e.Title != "" {
			out[p] = e.Title
		}
	}
	return out
}

// Auto is naming's title for path; false when the user renamed it or the write failed.
func (n *Names) Auto(path, title string) bool {
	n.mu.Lock()
	defer n.mu.Unlock()
	if e, ok := n.entries[path]; ok && !e.Auto {
		return false
	}
	return n.set(path, &Entry{Clip(title), true}) == nil
}

// Rename is the user's name for path; "" clears it. Naming never changes it again.
func (n *Names) Rename(path, title string) error {
	n.mu.Lock()
	defer n.mu.Unlock()
	return n.set(path, &Entry{Clip(title), false})
}

// Failed tells clients naming gave up on agentID's session.
func (n *Names) Failed(agentID string) { n.hub.Publish(proto.NewNamingFailed(agentID)) }

// Forget drops path's name, so a new Worktree there starts unnamed.
func (n *Names) Forget(path string) {
	n.mu.Lock()
	defer n.mu.Unlock()
	if _, ok := n.entries[path]; ok {
		n.set(path, nil)
	}
}

// set stores e for path, or drops path when e is nil.
func (n *Names) set(path string, e *Entry) error {
	prev, had := n.entries[path]
	if e == nil {
		delete(n.entries, path)
	} else {
		n.entries[path] = *e
	}
	raw, _ := json.Marshal(n.entries)
	err := os.MkdirAll(filepath.Dir(n.path), 0o700)
	if err == nil {
		err = atomicfile.Write(n.path, raw, 0o600)
	}
	if err != nil {
		if had {
			n.entries[path] = prev
		} else {
			delete(n.entries, path)
		}
		return err
	}
	n.hub.Publish(proto.NewWorktreeNames(n.titles()))
	return nil
}

// Clip trims title and cuts it to MaxTitle runes.
func Clip(title string) string {
	title = strings.TrimSpace(title)
	if r := []rune(title); len(r) > MaxTitle {
		title = strings.TrimSpace(string(r[:MaxTitle]))
	}
	return title
}
