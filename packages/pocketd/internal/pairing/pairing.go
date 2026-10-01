// Package pairing hands out one-time codes that a phone trades for a device token.
package pairing

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"net/url"
	"sync"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/proto"
)

const (
	ttl       = 5 * time.Minute
	keepSpent = 10 * time.Minute
	maxFails  = 5
	lockFor   = time.Minute
)

var (
	ErrExpired = errors.New("Code expired. Make a new one on your Mac.")
	ErrUsed    = errors.New("This code was already used.")
	ErrInvalid = errors.New("This code doesn't match. Check it on your Mac.")
	ErrLocked  = errors.New("Too many tries. Make a new code on your Mac in a minute.")
)

var codes = map[error]string{ErrExpired: proto.CodePairExpired, ErrUsed: proto.CodePairUsed, ErrInvalid: proto.CodePairInvalid, ErrLocked: proto.CodePairLocked}

// Code is the wire code for a pairing error, or "" for any other error.
func Code(err error) string { return codes[err] }

// Offer is what the owner shows the phone. ExpiresAt is in ms.
type Offer struct {
	URL       string `json:"url"`
	Code      string `json:"code"`
	ExpiresAt int64  `json:"expiresAt"`
}

// Result ends an offer. Code is "" on success, else "pair_expired", "pair_locked" or "pair_failed".
type Result struct{ DeviceID, Name, Code string }

type offer struct {
	hash    [32]byte
	expires time.Time
	done    chan Result
}

type spent struct {
	err error
	at  time.Time
}

// Manager holds at most one open code. Every way an offer ends except Cancel
// sends exactly one Result on its channel.
type Manager struct {
	now func() time.Time

	mu          sync.Mutex
	open        *offer
	spent       map[[32]byte]spent
	fails       int
	lockedUntil time.Time
}

func New(now func() time.Time) *Manager {
	return &Manager{now: now, spent: map[[32]byte]spent{}}
}

// Begin opens a new code for host ("ip:port") and voids the previous one.
func (m *Manager) Begin(host, macName string) (Offer, <-chan Result, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	now := m.now()
	if now.Before(m.lockedUntil) {
		return Offer{}, nil, ErrLocked
	}
	m.end(ErrExpired, Result{Code: proto.CodePairExpired}, now)
	m.fails = 0
	b := make([]byte, 16)
	rand.Read(b)
	code := base64.RawURLEncoding.EncodeToString(b)
	m.open = &offer{hash: sha256.Sum256([]byte(code)), expires: now.Add(ttl), done: make(chan Result, 1)}
	link := "anywhere://pair?v=1&h=" + url.QueryEscape(host) + "&c=" + code + "&n=" + url.QueryEscape(macName)
	return Offer{URL: link, Code: code, ExpiresAt: m.open.expires.UnixMilli()}, m.open.done, nil
}

// Watch hands deliver how offer ends: the result from done, or an expired one
// once its time runs out with the code still open. A cancelled ctx voids the
// code and delivers nothing.
func (m *Manager) Watch(ctx context.Context, offer Offer, done <-chan Result, deliver func(Result)) {
	expiry := time.NewTimer(time.Until(time.UnixMilli(offer.ExpiresAt)))
	defer expiry.Stop()
	select {
	case r := <-done:
		deliver(r)
	case <-expiry.C:
		if m.Cancel(offer.Code) {
			deliver(Result{Code: proto.CodePairExpired})
		} else {
			deliver(<-done)
		}
	case <-ctx.Done():
		m.Cancel(offer.Code)
	}
}

func (m *Manager) Cancel(code string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.open == nil || m.open.hash != sha256.Sum256([]byte(code)) {
		return false
	}
	m.spend(m.open.hash, ErrExpired, m.now())
	m.open = nil
	return true
}

func (m *Manager) Redeem(code string, add func() (devices.Device, string, error)) (devices.Device, string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	now := m.now()
	if now.Before(m.lockedUntil) {
		return devices.Device{}, "", ErrLocked
	}
	for h, s := range m.spent {
		if now.Sub(s.at) > keepSpent {
			delete(m.spent, h)
		}
	}
	h := sha256.Sum256([]byte(code))
	if m.open != nil && m.open.hash == h {
		if now.After(m.open.expires) {
			m.end(ErrExpired, Result{Code: proto.CodePairExpired}, now)
			return devices.Device{}, "", ErrExpired
		}
		d, token, err := add()
		if err != nil {
			m.end(ErrUsed, Result{Code: proto.CodePairFailed}, now)
			return devices.Device{}, "", err
		}
		m.end(ErrUsed, Result{DeviceID: d.ID, Name: d.Name}, now)
		return d, token, nil
	}
	if s, ok := m.spent[h]; ok {
		return devices.Device{}, "", s.err
	}
	if m.open == nil {
		return devices.Device{}, "", ErrInvalid
	}
	if m.fails++; m.fails < maxFails {
		return devices.Device{}, "", ErrInvalid
	}
	m.fails = 0
	m.lockedUntil = now.Add(lockFor)
	m.end(ErrExpired, Result{Code: proto.CodePairLocked}, now)
	return devices.Device{}, "", ErrLocked
}

func (m *Manager) end(err error, r Result, now time.Time) {
	if m.open == nil {
		return
	}
	m.spend(m.open.hash, err, now)
	m.open.done <- r
	m.open = nil
}

func (m *Manager) spend(h [32]byte, err error, now time.Time) {
	m.spent[h] = spent{err, now}
}
