// Package push decides what to push to each phone and sends it through the
// relay (docs/mobile/protocol.md, push.v1): sealed alerts when a session needs
// the user or ends, and one Live Activity per phone with this Mac's counts.
package push

import (
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"sync"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
)

var ErrNotRegistered = errors.New("This phone hasn't registered for pushes")

// Record is one phone's registration.
type Record struct {
	Device string `json:"device"`
	// Key seals alert content; only this phone holds it too.
	Key []byte `json:"key"`
	Env string `json:"env"`
	// Alert is nil once the relay said it is gone.
	Alert    *proto.PushHandle `json:"alert,omitempty"`
	Start    *proto.PushHandle `json:"start,omitempty"`
	Activity *Live             `json:"activity,omitempty"`
	Prefs    proto.PushPrefs   `json:"prefs"`
	// Frequent is ActivityKit's frequentPushesEnabled; off, updates slow down.
	Frequent  bool  `json:"frequent"`
	UpdatedAt int64 `json:"updatedAt"`
}

// Live is the phone's Live Activity for this Mac. Handle is nil until the phone has its push token.
type Live struct {
	ID        string            `json:"id"`
	Handle    *proto.PushHandle `json:"handle,omitempty"`
	StartedAt int64             `json:"startedAt"`
}

// Registration is what push.register carries.
type Registration struct {
	Key          []byte
	Env          string
	Alert, Start *proto.PushHandle
	Prefs        proto.PushPrefs
	Frequent     bool
}

// Sent is one send, for `pocketd push status`. It names no content.
type Sent struct {
	At       int64  `json:"at"`
	Device   string `json:"device"`
	Kind     string `json:"kind"`
	Agent    string `json:"agent,omitempty"`
	Priority int    `json:"priority"`
	Outcome  string `json:"outcome"`
}

const keepSends = 30

type file struct {
	Records map[string]*Record `json:"records"`
	Sends   []Sent             `json:"sends"`
}

// Store keeps push.json, 0600 beside devices.json.
type Store struct {
	path string
	mu   sync.Mutex
	f    file
}

func Path(home string) string { return filepath.Join(home, "push.json") }

func Open(path string) (*Store, error) {
	s := &Store{path: path, f: file{Records: map[string]*Record{}}}
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return s, nil
	}
	if err != nil {
		return s, err
	}
	if err := json.Unmarshal(raw, &s.f); err != nil {
		return s, err
	}
	if s.f.Records == nil {
		s.f.Records = map[string]*Record{}
	}
	return s, nil
}

func (s *Store) save() error {
	raw, _ := json.MarshalIndent(s.f, "", "  ")
	return atomicfile.Write(s.path, append(raw, '\n'), 0o600)
}

// change runs f on device's record under the lock and saves; false from f saves nothing.
func (s *Store) change(device string, f func(r *Record) bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	r := s.f.Records[device]
	if r == nil {
		return ErrNotRegistered
	}
	if !f(r) {
		return nil
	}
	return s.save()
}

// Register replaces device's registration; a Live Activity it had stays.
func (s *Store) Register(device string, reg Registration, now time.Time) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	r := &Record{Device: device, Key: reg.Key, Env: reg.Env, Alert: reg.Alert, Start: reg.Start, Prefs: reg.Prefs, Frequent: reg.Frequent, UpdatedAt: now.UnixMilli()}
	if old := s.f.Records[device]; old != nil {
		r.Activity = old.Activity
	}
	s.f.Records[device] = r
	return s.save()
}

// SetActivity names device's Live Activity, or forgets it when ended.
func (s *Store) SetActivity(device, id string, h *proto.PushHandle, ended bool, now time.Time) error {
	return s.change(device, func(r *Record) bool {
		switch {
		case ended:
			if r.Activity == nil || r.Activity.ID != id {
				return false
			}
			r.Activity = nil
		case r.Activity != nil && r.Activity.ID == id:
			if h == nil {
				return false
			}
			r.Activity.Handle = h
		default:
			r.Activity = &Live{ID: id, Handle: h, StartedAt: now.UnixMilli()}
		}
		return true
	})
}

func (s *Store) SetPrefs(device string, p proto.PushPrefs) error {
	return s.change(device, func(r *Record) bool {
		r.Prefs = p
		return true
	})
}

// Remove forgets device: it unregistered or was revoked.
func (s *Store) Remove(device string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.f.Records[device] == nil {
		return nil
	}
	delete(s.f.Records, device)
	return s.save()
}

// Which handle of a record a send used.
const (
	AlertHandle    = "alert"
	StartHandle    = "start"
	ActivityHandle = "activity"
)

// Drop forgets the handle the relay called gone, unless a newer one replaced it meanwhile.
func (s *Store) Drop(device, which, handle string) error {
	return s.change(device, func(r *Record) bool {
		switch {
		case which == AlertHandle && r.Alert != nil && r.Alert.Handle == handle:
			r.Alert = nil
		case which == StartHandle && r.Start != nil && r.Start.Handle == handle:
			r.Start = nil
		case which == ActivityHandle && r.Activity != nil && r.Activity.Handle != nil && r.Activity.Handle.Handle == handle:
			r.Activity = nil
		default:
			return false
		}
		return true
	})
}

func clone(r *Record) Record {
	c := *r
	if r.Activity != nil {
		a := *r.Activity
		c.Activity = &a
	}
	return c
}

func (s *Store) Get(device string) (Record, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	r := s.f.Records[device]
	if r == nil {
		return Record{}, false
	}
	return clone(r), true
}

// Records are every registration, by device id.
func (s *Store) Records() []Record {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]Record, 0, len(s.f.Records))
	for _, r := range s.f.Records {
		out = append(out, clone(r))
	}
	slices.SortFunc(out, func(a, b Record) int { return int(a.UpdatedAt - b.UpdatedAt) })
	return out
}

// Note keeps the last sends.
func (s *Store) Note(sent Sent) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.f.Sends = append(s.f.Sends, sent)
	if n := len(s.f.Sends); n > keepSends {
		s.f.Sends = slices.Clone(s.f.Sends[n-keepSends:])
	}
	s.save()
}

func (s *Store) Sends() []Sent {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.f.Sends)
}
