// Package owners keeps the tokens that make a TCP peer the owner. The CLI
// writes the file and pocketd re-reads it on every check: on Windows no local
// process is owner by its pid, so minting can't go through pocketd.
package owners

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/devices"
)

var (
	ErrNotFound  = errors.New("no such owner token")
	ErrAmbiguous = errors.New("more than one owner token matches; type more of the id")
)

type Owner struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	// CreatedAt is ms since the epoch.
	CreatedAt int64  `json:"createdAt"`
	TokenHash string `json:"tokenHash,omitempty"`
}

type file struct {
	Version int     `json:"version"`
	Owners  []Owner `json:"owners"`
}

// Store is the file at Path, read afresh by every call.
type Store struct{ Path string }

func Path(home string) string { return filepath.Join(home, "state", "owner-tokens.json") }

func (s Store) read() ([]Owner, error) {
	raw, err := os.ReadFile(s.Path)
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
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
	return f.Owners, nil
}

func (s Store) write(list []Owner) error {
	if err := os.MkdirAll(filepath.Dir(s.Path), 0o700); err != nil {
		return err
	}
	raw, _ := json.MarshalIndent(file{1, list}, "", "  ")
	return atomicfile.Write(s.Path, append(raw, '\n'), 0o600)
}

func hash(token string) []byte {
	sum := sha256.Sum256([]byte(token))
	return sum[:]
}

// Mint adds an owner token and returns it. The token is never stored or shown again.
func (s Store) Mint(name string) (Owner, string, error) {
	list, err := s.read()
	if err != nil {
		return Owner{}, "", err
	}
	id := make([]byte, 16)
	rand.Read(id)
	token := devices.NewToken()
	o := Owner{ID: hex.EncodeToString(id), Name: devices.CleanName(name), CreatedAt: time.Now().UnixMilli(), TokenHash: hex.EncodeToString(hash(token))}
	if err := s.write(append(list, o)); err != nil {
		return Owner{}, "", err
	}
	o.TokenHash = ""
	return o, token, nil
}

// List returns the owner tokens oldest first, without their hashes.
func (s Store) List() ([]Owner, error) {
	list, err := s.read()
	for i := range list {
		list[i].TokenHash = ""
	}
	return list, err
}

// Lookup finds token's owner. Every entry is compared, in constant time.
func (s Store) Lookup(token string) (Owner, bool) {
	list, err := s.read()
	if err != nil || token == "" {
		return Owner{}, false
	}
	sum, found := hash(token), -1
	for i, o := range list {
		h, _ := hex.DecodeString(o.TokenHash)
		if subtle.ConstantTimeCompare(h, sum) == 1 {
			found = i
		}
	}
	if found < 0 {
		return Owner{}, false
	}
	o := list[found]
	o.TokenHash = ""
	return o, true
}

// Has reports whether id is still an owner token. A file it can't read
// revokes them all.
func (s Store) Has(id string) bool {
	list, err := s.read()
	return err == nil && slices.ContainsFunc(list, func(o Owner) bool { return o.ID == id })
}

// WatchEvery is how soon a revoke reaches a live conn.
var WatchEvery = 2 * time.Second

// Watch calls revoked once id stops being an owner token, unless ctx ends first.
func (s Store) Watch(ctx context.Context, id string, revoked func()) {
	t := time.NewTicker(WatchEvery)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			if !s.Has(id) {
				revoked()
				return
			}
		}
	}
}

// Revoke removes the owner token whose id starts with prefix. Conns that
// authed with it close within WatchEvery.
func (s Store) Revoke(prefix string) (Owner, error) {
	list, err := s.read()
	if err != nil {
		return Owner{}, err
	}
	found := -1
	for i, o := range list {
		if prefix == "" || !strings.HasPrefix(o.ID, prefix) {
			continue
		}
		if found >= 0 {
			return Owner{}, ErrAmbiguous
		}
		found = i
	}
	if found < 0 {
		return Owner{}, ErrNotFound
	}
	o := list[found]
	if err := s.write(slices.Delete(list, found, found+1)); err != nil {
		return Owner{}, err
	}
	o.TokenHash = ""
	return o, nil
}
