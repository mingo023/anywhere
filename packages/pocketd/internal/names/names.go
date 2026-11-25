// Package names keeps each Worktree's display name, by path, in pocketd.db.
package names

import (
	"database/sql"
	"fmt"
	"strings"
	"sync"

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
	db  *sql.DB
	hub *hub.Hub

	mu      sync.Mutex
	entries map[string]Entry
}

func Open(db *sql.DB) (*Names, error) {
	n := &Names{db: db, hub: hub.New(), entries: map[string]Entry{}}
	rows, err := db.Query(`SELECT path, title, auto FROM names`)
	if err != nil {
		return nil, fmt.Errorf("names: %w", err)
	}
	defer rows.Close()
	for rows.Next() {
		var path string
		var e Entry
		if err := rows.Scan(&path, &e.Title, &e.Auto); err != nil {
			return nil, fmt.Errorf("names: %w", err)
		}
		n.entries[path] = e
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("names: %w", err)
	}
	return n, nil
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

// set stores e for path, or drops path when e is nil, and takes it only once it is written.
func (n *Names) set(path string, e *Entry) error {
	var err error
	if e == nil {
		_, err = n.db.Exec(`DELETE FROM names WHERE path = ?`, path)
	} else {
		_, err = n.db.Exec(`INSERT INTO names (path, title, auto) VALUES (?, ?, ?) ON CONFLICT (path) DO UPDATE SET title = excluded.title, auto = excluded.auto`, path, e.Title, e.Auto)
	}
	if err != nil {
		return err
	}
	if e == nil {
		delete(n.entries, path)
	} else {
		n.entries[path] = *e
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
