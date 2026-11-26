// Package devices keeps the phones paired with this Mac in pocketd.db. Only
// a hash of each device's token is stored.
package devices

import (
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"reflect"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"
)

type Scope string

var (
	PhoneScopes  = []Scope{"observe", "drive", "approve", "spawn"}
	LegacyScopes = []Scope{"observe", "drive", "approve"}
)

const (
	LegacyID    = "legacy"
	maxName     = 64
	defaultName = "iPhone"
	saveSeenGap = time.Minute
)

var (
	ErrNotFound  = errors.New("no such device")
	ErrAmbiguous = errors.New("more than one device matches; type more of the id")
	ErrLegacy    = errors.New("the shared token can't be renamed or removed")
)

type Device struct {
	ID        string  `json:"id"`
	Name      string  `json:"name"`
	Platform  string  `json:"platform"`
	Scopes    []Scope `json:"scopes"`
	TokenHash string  `json:"tokenHash,omitempty"`
	// Times are ms since the epoch.
	CreatedAt   int64  `json:"createdAt"`
	LastSeenAt  int64  `json:"lastSeenAt"`
	LastAddr    string `json:"lastAddr"`
	Legacy      bool   `json:"legacy,omitempty"`
	GraceEndsAt int64  `json:"graceEndsAt,omitempty"`
}

type Store struct {
	db *sql.DB

	mu         sync.Mutex
	devices    []Device
	graceEnded bool
	savedAt    map[string]time.Time
}

// Open loads the devices from db, which may be read-only while legacyToken is empty. A non-empty
// legacyToken is stored, hashed, as the legacy device.
func Open(db *sql.DB, legacyToken string) (*Store, error) {
	s := &Store{db: db, savedAt: map[string]time.Time{}}
	rows, err := db.Query(`SELECT id, name, platform, scopes, token_hash, created_at, last_seen_at, last_addr, legacy, grace_ends_at FROM devices ORDER BY position`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	for rows.Next() {
		var d Device
		var scopes string
		if err := rows.Scan(&d.ID, &d.Name, &d.Platform, &scopes, &d.TokenHash, &d.CreatedAt, &d.LastSeenAt, &d.LastAddr, &d.Legacy, &d.GraceEndsAt); err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(scopes), &d.Scopes); err != nil {
			return nil, err
		}
		s.devices = append(s.devices, d)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	rows.Close()
	var ended string
	if err := db.QueryRow(`SELECT value FROM meta WHERE key = 'devices.grace_ended'`).Scan(&ended); err != nil && !errors.Is(err, sql.ErrNoRows) {
		return nil, err
	}
	s.graceEnded = ended == "true"
	return s, s.EnsureLegacy(legacyToken)
}

func NewToken() string {
	b := make([]byte, 32)
	rand.Read(b)
	return base64.RawURLEncoding.EncodeToString(b)
}

func hash(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}

func CleanName(name string) string {
	name = strings.TrimSpace(strings.Map(func(r rune) rune {
		if unicode.IsControl(r) {
			return -1
		}
		return r
	}, name))
	if r := []rune(name); len(r) > maxName {
		name = strings.TrimSpace(string(r[:maxName]))
	}
	return name
}

// Add pairs a device and returns its token. The token is never stored or shown again.
func (s *Store) Add(name, platform string, scopes []Scope) (Device, string, error) {
	id := make([]byte, 16)
	rand.Read(id)
	token := NewToken()
	name = CleanName(name)
	if name == "" {
		name = defaultName
	}
	d := Device{ID: hex.EncodeToString(id), Name: name, Platform: platform, Scopes: scopes, TokenHash: hash(token), CreatedAt: time.Now().UnixMilli()}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.save(append(slices.Clone(s.devices), d), s.graceEnded); err != nil {
		return Device{}, "", err
	}
	return d.public(), token, nil
}

// public is d without the hash of its token.
func (d Device) public() Device {
	d.TokenHash = ""
	return d
}

func (s *Store) Lookup(token string) (Device, bool) {
	sum := hash(token)
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, d := range s.devices {
		if d.TokenHash == sum && !d.graceOver(time.Now()) {
			return d.public(), true
		}
	}
	return Device{}, false
}

func (s *Store) Has(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.index(id) >= 0
}

func (s *Store) index(id string) int {
	return slices.IndexFunc(s.devices, func(d Device) bool { return d.ID == id })
}

// Rewrites the row at most once a minute.
func (s *Store) Seen(id, addr string, now time.Time) {
	s.mu.Lock()
	defer s.mu.Unlock()
	n := s.index(id)
	if n < 0 {
		return
	}
	s.devices[n].LastSeenAt, s.devices[n].LastAddr = now.UnixMilli(), addr
	if now.Sub(s.savedAt[id]) >= saveSeenGap {
		s.savedAt[id] = now
		s.db.Exec(`UPDATE devices SET last_seen_at = ?, last_addr = ? WHERE id = ?`, s.devices[n].LastSeenAt, addr, id)
	}
}

// List returns paired devices oldest first, the legacy device first, without token hashes.
func (s *Store) List() []Device {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := []Device{}
	for _, d := range s.devices {
		out = append(out, d.public())
	}
	return out
}

// Resolve turns a unique id prefix, as typed in `pocketd devices`, into an id.
func (s *Store) Resolve(prefix string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var found []string
	for _, d := range s.devices {
		if prefix != "" && strings.HasPrefix(d.ID, prefix) {
			found = append(found, d.ID)
		}
	}
	switch len(found) {
	case 0:
		return "", ErrNotFound
	case 1:
		return found[0], nil
	}
	return "", ErrAmbiguous
}

func (s *Store) Rename(id, name string) (Device, error) {
	if name = CleanName(name); name == "" {
		return Device{}, errors.New("the name is empty")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	n, err := s.find(id)
	if err != nil {
		return Device{}, err
	}
	next := slices.Clone(s.devices)
	next[n].Name = name
	if err := s.save(next, s.graceEnded); err != nil {
		return Device{}, err
	}
	return s.devices[n].public(), nil
}

// Revoke unpairs a device. Its live sockets stay open until the caller closes them.
func (s *Store) Revoke(id string) (Device, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	n, err := s.find(id)
	if err != nil {
		return Device{}, err
	}
	gone := s.devices[n]
	if err := s.save(slices.Delete(slices.Clone(s.devices), n, n+1), s.graceEnded); err != nil {
		return Device{}, err
	}
	return gone.public(), nil
}

func (s *Store) find(id string) (int, error) {
	if id == LegacyID {
		return 0, ErrLegacy
	}
	if n := s.index(id); n >= 0 {
		return n, nil
	}
	return 0, ErrNotFound
}

// A failed write changes nothing.
func (s *Store) save(next []Device, graceEnded bool) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for _, d := range s.devices {
		if !slices.ContainsFunc(next, func(n Device) bool { return n.ID == d.ID }) {
			if _, err := tx.Exec(`DELETE FROM devices WHERE id = ?`, d.ID); err != nil {
				return err
			}
		}
	}
	for i, d := range next {
		if i < len(s.devices) && reflect.DeepEqual(s.devices[i], d) {
			continue
		}
		scopes := d.Scopes
		if scopes == nil {
			scopes = []Scope{}
		}
		raw, _ := json.Marshal(scopes)
		if _, err := tx.Exec(`INSERT INTO devices (id, name, platform, scopes, token_hash, created_at, last_seen_at, last_addr, legacy, grace_ends_at, position)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (id) DO UPDATE SET name = excluded.name, platform = excluded.platform, scopes = excluded.scopes,
			token_hash = excluded.token_hash, created_at = excluded.created_at, last_seen_at = excluded.last_seen_at, last_addr = excluded.last_addr,
			legacy = excluded.legacy, grace_ends_at = excluded.grace_ends_at, position = excluded.position`,
			d.ID, d.Name, d.Platform, string(raw), d.TokenHash, d.CreatedAt, d.LastSeenAt, d.LastAddr, d.Legacy, d.GraceEndsAt, i); err != nil {
			return err
		}
	}
	if graceEnded != s.graceEnded {
		if _, err := tx.Exec(`INSERT INTO meta (key, value) VALUES ('devices.grace_ended', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value`, strconv.FormatBool(graceEnded)); err != nil {
			return err
		}
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	s.devices, s.graceEnded = next, graceEnded
	return nil
}
