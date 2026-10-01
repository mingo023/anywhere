// Package devices keeps the phones paired with this Mac in devices.json. Only
// a hash of each device's token is stored.
package devices

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"slices"
	"strings"
	"sync"
	"time"
	"unicode"

	"pocketd/internal/atomicfile"
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
	CreatedAt  int64  `json:"createdAt"`
	LastSeenAt int64  `json:"lastSeenAt"`
	LastAddr   string `json:"lastAddr"`
	Legacy     bool   `json:"legacy,omitempty"`
}

type file struct {
	Version int      `json:"version"`
	Devices []Device `json:"devices"`
}

type Store struct {
	path string

	mu      sync.Mutex
	devices []Device
	savedAt map[string]time.Time
}

// Open reads path, or starts empty if it doesn't exist. A non-empty
// legacyToken is stored, hashed, as the legacy device.
func Open(path, legacyToken string) (*Store, error) {
	s := &Store{path: path, savedAt: map[string]time.Time{}}
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return s, s.EnsureLegacy(legacyToken)
	}
	if err != nil {
		return nil, err
	}
	var f file
	if err := json.Unmarshal(raw, &f); err != nil {
		return nil, err
	}
	if f.Version != 1 {
		return nil, fmt.Errorf("unknown version %d", f.Version)
	}
	s.devices = f.Devices
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
	s.devices = append(s.devices, d)
	if err := s.save(); err != nil {
		s.devices = s.devices[:len(s.devices)-1]
		return Device{}, "", err
	}
	d.TokenHash = ""
	return d, token, nil
}

func (s *Store) Lookup(token string) (Device, bool) {
	sum := hash(token)
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, d := range s.devices {
		if d.TokenHash == sum {
			d.TokenHash = ""
			return d, true
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

// Seen records a connection. The file is rewritten at most once a minute per device.
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
		s.save()
	}
}

// List returns paired devices oldest first, the legacy device first, without token hashes.
func (s *Store) List() []Device {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := []Device{}
	for _, d := range s.devices {
		d.TokenHash = ""
		out = append(out, d)
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
	old := s.devices[n].Name
	s.devices[n].Name = name
	if err := s.save(); err != nil {
		s.devices[n].Name = old
		return Device{}, err
	}
	d := s.devices[n]
	d.TokenHash = ""
	return d, nil
}

// Revoke unpairs a device. Its live sockets stay open until the caller closes them.
func (s *Store) Revoke(id string) (Device, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	n, err := s.find(id)
	if err != nil {
		return Device{}, err
	}
	before := s.devices
	s.devices = slices.Delete(slices.Clone(before), n, n+1)
	if err := s.save(); err != nil {
		s.devices = before
		return Device{}, err
	}
	d := before[n]
	d.TokenHash = ""
	return d, nil
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

func (s *Store) save() error {
	raw, _ := json.MarshalIndent(file{1, s.devices}, "", "  ")
	return atomicfile.Write(s.path, append(raw, '\n'), 0o600)
}
