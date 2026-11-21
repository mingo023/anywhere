package orch

import (
	"encoding/json"
	"math"
	"slices"
	"strings"
	"time"
)

// Restorable is each session a restart brings back, by id, with the terminal
// entry it was last saved with, as terminals.json holds it.
func (s *Store) Restorable() (map[string][]byte, error) {
	rows, err := s.db.Query(`SELECT id, saved FROM sessions WHERE saved != ''`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[string][]byte{}
	for rows.Next() {
		var id, saved string
		if err := rows.Scan(&id, &saved); err != nil {
			return nil, err
		}
		out[id] = []byte(saved)
	}
	return out, rows.Err()
}

func (s *Store) OpenSessions() ([]Session, error) {
	return s.Sessions(`closed_at = 0 AND archived = 0 AND status != 'closed' AND instr(id, '/') = 0 ORDER BY created_at, id`)
}

// Subtrees are the worktrees the subagents of a tree's sessions worked in.
type Subtrees struct {
	// Local are this Mac's, every generation down.
	Local []string
	// Hosts are the folders of the subagents on linked hosts given worktrees
	// of their own there; each host deletes its own subagents' with them.
	Hosts []HostTree
}

// HostTree's Sessions are the line's on that host, untagged, for the host to
// tell its subagents' worktree from one someone else works in or made since.
type HostTree struct {
	Host, Path string
	Sessions   []string
}

// Archive keeps in lineage, past the retention Prune applies, each closed
// session whose folder kept says still holds a worktree Anywhere made, and the
// host subagents of those it keeps; it drops the rest. Run before Prune.
func (s *Store) Archive(now time.Time, kept func(cwd string) bool) error {
	cut := now.UnixMilli() - ClosedKept.Milliseconds()
	gone, err := s.Sessions(`closed_at > 0 AND closed_at < ?`, cut)
	if err != nil {
		return err
	}
	live, err := s.Sessions(`NOT (closed_at > 0 AND closed_at < ?)`, cut)
	if err != nil {
		return err
	}
	old, isolated, err := s.archived()
	if err != nil {
		return err
	}
	// Prune drops a delegation with either end, so a younger child loses its isolation with its parent's.
	rows, err := s.db.Query(`SELECT child_id FROM delegations WHERE isolation = 'worktree'
		AND (child_id IN (SELECT id FROM sessions WHERE closed_at > 0 AND closed_at < ?1) OR parent_id IN (SELECT id FROM sessions WHERE closed_at > 0 AND closed_at < ?1))`, cut)
	if err != nil {
		return err
	}
	for rows.Next() {
		var id string
		if rows.Scan(&id) == nil {
			isolated[id] = true
		}
	}
	rows.Close()
	stay := map[string]Session{}
	for _, x := range append(old, gone...) {
		stay[x.ID] = x
	}
	for _, x := range live {
		if isolated[x.ID] {
			if _, ok := stay[x.ID]; !ok {
				stay[x.ID] = x
			}
		}
	}
	present := func(id string) bool {
		_, ok := stay[id]
		return ok || slices.ContainsFunc(live, func(l Session) bool { return l.ID == id })
	}
	for dropped := true; dropped; {
		dropped = false
		for id, x := range stay {
			ok := kept(x.Cwd)
			if strings.Contains(id, "/") {
				ok = x.Relationship == "subagent" && present(x.ParentID)
			}
			if !ok {
				delete(stay, id)
				dropped = true
			}
		}
	}
	return s.Tx(func(tx *Tx) error {
		if _, err := tx.Exec(`DELETE FROM lineage`); err != nil {
			return err
		}
		for _, x := range stay {
			if _, err := tx.Exec(`INSERT INTO lineage (id, parent_id, relationship, cwd, created_at, isolated) VALUES (?, ?, ?, ?, ?, ?)`,
				x.ID, x.ParentID, x.Relationship, x.Cwd, x.CreatedAt, isolated[x.ID]); err != nil {
				return err
			}
		}
		return nil
	})
}

// archived is lineage's sessions, closed, and which of them were given worktrees of their own.
func (s *Store) archived() ([]Session, map[string]bool, error) {
	rows, err := s.db.Query(`SELECT id, parent_id, relationship, cwd, created_at, isolated FROM lineage`)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()
	var out []Session
	isolated := map[string]bool{}
	for rows.Next() {
		x := Session{Status: "closed", ClosedAt: 1}
		var iso bool
		if err := rows.Scan(&x.ID, &x.ParentID, &x.Relationship, &x.Cwd, &x.CreatedAt, &iso); err != nil {
			return nil, nil, err
		}
		out = append(out, x)
		isolated[x.ID] = iso
	}
	return out, isolated, rows.Err()
}

// Archived is the session lineage keeps for id, past retention; nil for none.
func (s *Store) Archived(id string) (*Session, error) {
	all, _, err := s.archived()
	if err != nil {
		return nil, err
	}
	if i := slices.IndexFunc(all, func(x Session) bool { return x.ID == id }); i >= 0 {
		return &all[i], nil
	}
	return nil, nil
}

// SubagentTrees is each worktree the subagents of tree's sessions worked in,
// open or closed, except one an open session outside that line works in.
// treeOf is the worktree holding a folder that was there at a time in Unix
// milliseconds, "" for none: a closed session's folder may hold a newer
// worktree at the same path by now.
func (s *Store) SubagentTrees(tree string, treeOf func(cwd string, at int64) string) (Subtrees, error) {
	all, err := s.Sessions(`1 = 1 ORDER BY created_at, id`)
	if err != nil {
		return Subtrees{}, err
	}
	old, isolated, err := s.archived()
	if err != nil {
		return Subtrees{}, err
	}
	for _, x := range old {
		if !slices.ContainsFunc(all, func(a Session) bool { return a.ID == x.ID }) {
			all = append(all, x)
		}
	}
	rows, err := s.db.Query(`SELECT child_id FROM delegations WHERE isolation = 'worktree' AND instr(child_id, '/') > 0`)
	if err != nil {
		return Subtrees{}, err
	}
	for rows.Next() {
		var id string
		if rows.Scan(&id) == nil {
			isolated[id] = true
		}
	}
	rows.Close()
	trees := make(map[string]string, len(all))
	line := map[string]bool{}
	var parents []string
	for _, x := range all {
		// A session starts in its worktree, and a newer one there is made once it's gone; its close time can move
		// later, as when a restart closes what it didn't bring back.
		at := int64(math.MaxInt64)
		if x.ClosedAt != 0 || x.Status == "closed" {
			at = x.CreatedAt
		}
		if strings.Contains(x.ID, "/") {
			continue
		}
		if trees[x.ID] = treeOf(x.Cwd, at); trees[x.ID] == tree {
			parents = append(parents, x.ID)
		}
	}
	for len(parents) > 0 {
		p := parents[len(parents)-1]
		parents = parents[:len(parents)-1]
		for _, x := range all {
			if x.ParentID == p && x.Relationship == "subagent" && !line[x.ID] {
				line[x.ID] = true
				parents = append(parents, x.ID)
			}
		}
	}
	onHost := map[string][]string{}
	for _, x := range all {
		if host, id, tagged := strings.Cut(x.ID, "/"); tagged && line[x.ID] {
			onHost[host] = append(onHost[host], id)
		}
	}
	var out Subtrees
	for _, x := range all {
		if host, _, tagged := strings.Cut(x.ID, "/"); tagged {
			if line[x.ID] && isolated[x.ID] && x.Cwd != "" {
				out.Hosts = append(out.Hosts, HostTree{host, x.Cwd, onHost[host]})
			}
			continue
		}
		w := trees[x.ID]
		if !line[x.ID] || w == "" || w == tree || slices.Contains(out.Local, w) {
			continue
		}
		outsider := func(o Session) bool {
			return trees[o.ID] == w && !line[o.ID] && o.ClosedAt == 0 && o.Status != "closed"
		}
		if !slices.ContainsFunc(all, outsider) {
			out.Local = append(out.Local, w)
		}
	}
	return out, nil
}

// CloseAbsent closes each open session that isn't running: after a start,
// those the pocketd before left that came back neither restored nor handed
// down. It returns the ids it closed.
func (s *Store) CloseAbsent(running func(id string) bool) ([]string, error) {
	open, err := s.OpenSessions()
	if err != nil {
		return nil, err
	}
	var closed []string
	err = s.Tx(func(tx *Tx) error {
		for _, x := range open {
			if running(x.ID) {
				continue
			}
			if _, err := tx.Exec(`UPDATE sessions SET status = 'closed', closed_at = ? WHERE id = ?`, time.Now().UnixMilli(), x.ID); err != nil {
				return err
			}
			if err := event(tx, x.ID, "session.closed", map[string]string{"from": x.Status, "to": "closed"}); err != nil {
				return err
			}
			closed = append(closed, x.ID)
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	return closed, nil
}

// Import writes each session the store hasn't heard of, as a pocketd
// without the store saved it; one it has stays as it is.
func (s *Store) Import(sessions []Session) error {
	return s.Tx(func(tx *Tx) error {
		for _, x := range sessions {
			if _, err := tx.Exec(`INSERT INTO sessions (id, agent, cwd, terminal, conversation, access, model, effort, pinned, chat, origin, status, failed, created_at)
				VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (id) DO NOTHING`,
				x.ID, x.Agent, x.Cwd, x.Terminal, x.Conversation, x.Access, x.Model, x.Effort, x.Pinned, x.Chat, x.Origin, x.Status, x.Failed, x.CreatedAt); err != nil {
				return err
			}
		}
		return nil
	})
}

func (s *Store) Unhold() error {
	return s.Tx(func(tx *Tx) error {
		_, err := tx.Exec(`DELETE FROM held`)
		return err
	})
}

// SetRestorable makes entries, by session id, all that a restart brings back.
func (s *Store) SetRestorable(entries map[string][]byte) error {
	return s.Tx(func(tx *Tx) error {
		if _, err := tx.Exec(`UPDATE sessions SET saved = '' WHERE saved != ''`); err != nil {
			return err
		}
		for id, saved := range entries {
			var e struct {
				CreatedAt int64 `json:"createdAt"`
			}
			json.Unmarshal(saved, &e)
			if _, err := tx.Exec(`INSERT INTO sessions (id, saved, created_at) VALUES (?, ?, ?) ON CONFLICT (id) DO UPDATE SET saved = excluded.saved`,
				id, string(saved), e.CreatedAt); err != nil {
				return err
			}
		}
		return nil
	})
}
