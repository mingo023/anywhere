// Package mcp serves Anywhere's orchestrator tools to the agents pocketd
// runs, over streamable HTTP on loopback. Each call is authenticated as the
// session its bearer token was issued to.
package mcp

import (
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"errors"
	"sync"
	"time"

	"pocketd/internal/orch"
)

// IdleExpiry is how long a token lives unused.
const IdleExpiry = 24 * time.Hour

// Tokens issues and checks per-session tokens; the store keeps only their SHA-256.
type Tokens struct {
	store *orch.Store
	now   func() time.Time

	mu   sync.Mutex
	seen map[string]time.Time // last use in this process, on the monotonic clock
}

func NewTokens(s *orch.Store) *Tokens {
	return &Tokens{store: s, now: time.Now, seen: map[string]time.Time{}}
}

func hash(token string) string {
	h := sha256.Sum256([]byte(token))
	return hex.EncodeToString(h[:])
}

// Issue gives terminal a fresh token and revokes its older ones: a session
// restored or relaunched has no use for the token its dead process held.
func (t *Tokens) Issue(terminal, agent string) (string, error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	token := "anyw_" + hex.EncodeToString(b)
	now := t.now().UnixMilli()
	err := t.store.Tx(func(tx *orch.Tx) error {
		if _, err := tx.Exec(`UPDATE tokens SET revoked = 1 WHERE session_id = ?`, terminal); err != nil {
			return err
		}
		_, err := tx.Exec(`INSERT INTO tokens (hash, session_id, agent, issued_at, last_alive) VALUES (?, ?, ?, ?, ?)`, hash(token), terminal, agent, now, now)
		return err
	})
	return token, err
}

// Grant is who a token speaks for.
type Grant struct {
	Terminal, Agent string
}

var ErrBadToken = errors.New("bad or expired token")

// Resolve checks token and marks it alive. A token idle past IdleExpiry, by
// wall clock or by the monotonic clock, whichever says longer, expires.
func (t *Tokens) Resolve(token string) (Grant, error) {
	if token == "" {
		return Grant{}, ErrBadToken
	}
	h := hash(token)
	var g Grant
	var last int64
	var expired, revoked bool
	err := t.store.DB().QueryRow(`SELECT session_id, agent, last_alive, expired, revoked FROM tokens WHERE hash = ?`, h).Scan(&g.Terminal, &g.Agent, &last, &expired, &revoked)
	if errors.Is(err, sql.ErrNoRows) || expired || revoked {
		return Grant{}, ErrBadToken
	}
	if err != nil {
		return Grant{}, err
	}
	now := t.now()
	idle := now.Sub(time.UnixMilli(last))
	t.mu.Lock()
	if at, ok := t.seen[h]; ok {
		idle = max(idle, now.Sub(at))
	}
	t.seen[h] = now
	t.mu.Unlock()
	if idle > IdleExpiry {
		t.store.Tx(func(tx *orch.Tx) error {
			_, err := tx.Exec(`UPDATE tokens SET expired = 1 WHERE hash = ?`, h)
			return err
		})
		return Grant{}, ErrBadToken
	}
	t.store.Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`UPDATE tokens SET last_alive = ? WHERE hash = ?`, now.UnixMilli(), h)
		return err
	})
	return g, nil
}

func (t *Tokens) Revoke(terminal string) error {
	return t.store.Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`UPDATE tokens SET revoked = 1 WHERE session_id = ?`, terminal)
		return err
	})
}
