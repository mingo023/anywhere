package orch

import (
	"encoding/json"
	"math"
	"slices"
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

// SubagentTrees is each worktree the subagents of tree's sessions worked in,
// every generation down, open or closed, except one an open session outside
// that line works in. treeOf is the worktree holding a folder that was there
// at a time in Unix milliseconds, "" for none: a closed session's folder may
// hold a newer worktree at the same path by now.
func (s *Store) SubagentTrees(tree string, treeOf func(cwd string, at int64) string) ([]string, error) {
	all, err := s.Sessions(`instr(id, '/') = 0 ORDER BY created_at, id`)
	if err != nil {
		return nil, err
	}
	trees := make(map[string]string, len(all))
	line := map[string]bool{}
	var parents []string
	for _, x := range all {
		at := int64(math.MaxInt64)
		if x.ClosedAt != 0 {
			at = max(x.UpdatedAt, x.ClosedAt)
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
	var out []string
	for _, x := range all {
		w := trees[x.ID]
		if !line[x.ID] || w == "" || w == tree || slices.Contains(out, w) {
			continue
		}
		outsider := func(o Session) bool {
			return trees[o.ID] == w && !line[o.ID] && o.ClosedAt == 0 && o.Status != "closed"
		}
		if !slices.ContainsFunc(all, outsider) {
			out = append(out, w)
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
