// Package push decides what to push to each phone and sends it through the
// relay (docs/mobile/protocol.md, push.v1): sealed alerts when a session needs
// the user or ends, and one Live Activity per phone with this Mac's counts.
package push

import (
	"database/sql"
	"encoding/json"
	"errors"
	"log"
	"path/filepath"
	"slices"
	"sync"
	"time"

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

// Store keeps the push_records and push_sends tables of pocketd.db.
type Store struct {
	db      *sql.DB
	mu      sync.Mutex
	records map[string]*Record
	sends   []Sent
	// lastSend is the position of the newest row in push_sends, -1 when it is empty.
	lastSend int64
}

// Path is push.json, read only by the import.
func Path(home string) string { return filepath.Join(home, "push.json") }

// Open loads the registrations and sends from db, which may be read-only. A store that can't load is still
// returned, empty, with the error.
func Open(db *sql.DB) (*Store, error) {
	s := &Store{db: db, records: map[string]*Record{}, lastSend: -1}
	if err := s.load(); err != nil {
		*s = Store{db: db, records: map[string]*Record{}, lastSend: -1}
		return s, err
	}
	return s, nil
}

func (s *Store) load() error {
	rows, err := s.db.Query(`SELECT device, key, env, alert, start, activity, prefs, frequent, updated_at FROM push_records`)
	if err != nil {
		return err
	}
	defer rows.Close()
	for rows.Next() {
		r := &Record{}
		var alert, start, activity sql.NullString
		var prefs string
		if err := rows.Scan(&r.Device, &r.Key, &r.Env, &alert, &start, &activity, &prefs, &r.Frequent, &r.UpdatedAt); err != nil {
			return err
		}
		for _, f := range []struct {
			raw sql.NullString
			to  any
		}{{alert, &r.Alert}, {start, &r.Start}, {activity, &r.Activity}, {sql.NullString{String: prefs, Valid: true}, &r.Prefs}} {
			if f.raw.Valid {
				if err := json.Unmarshal([]byte(f.raw.String), f.to); err != nil {
					return err
				}
			}
		}
		s.records[r.Device] = r
	}
	if err := rows.Err(); err != nil {
		return err
	}
	rows.Close()
	if rows, err = s.db.Query(`SELECT position, at, device, kind, agent, priority, outcome FROM push_sends ORDER BY position`); err != nil {
		return err
	}
	defer rows.Close()
	for rows.Next() {
		var x Sent
		if err := rows.Scan(&s.lastSend, &x.At, &x.Device, &x.Kind, &x.Agent, &x.Priority, &x.Outcome); err != nil {
			return err
		}
		s.sends = append(s.sends, x)
	}
	return rows.Err()
}

func nullJSON[T any](v *T) *string {
	if v == nil {
		return nil
	}
	raw, _ := json.Marshal(v)
	out := string(raw)
	return &out
}

// put writes r's row and, once it is committed, takes r.
func (s *Store) put(r *Record) error {
	prefs, _ := json.Marshal(r.Prefs)
	if _, err := s.db.Exec(`INSERT INTO push_records (device, key, env, alert, start, activity, prefs, frequent, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (device) DO UPDATE SET key = excluded.key, env = excluded.env, alert = excluded.alert, start = excluded.start,
		activity = excluded.activity, prefs = excluded.prefs, frequent = excluded.frequent, updated_at = excluded.updated_at`,
		r.Device, r.Key, r.Env, nullJSON(r.Alert), nullJSON(r.Start), nullJSON(r.Activity), string(prefs), r.Frequent, r.UpdatedAt); err != nil {
		return err
	}
	s.records[r.Device] = r
	return nil
}

// change runs f on device's record under the lock and saves; false from f saves nothing.
func (s *Store) change(device string, f func(r *Record) bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	old := s.records[device]
	if old == nil {
		return ErrNotRegistered
	}
	r := clone(old)
	if !f(&r) {
		return nil
	}
	return s.put(&r)
}

// Register replaces device's registration; a Live Activity it had stays.
func (s *Store) Register(device string, reg Registration, now time.Time) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	r := &Record{Device: device, Key: reg.Key, Env: reg.Env, Alert: reg.Alert, Start: reg.Start, Prefs: reg.Prefs, Frequent: reg.Frequent, UpdatedAt: now.UnixMilli()}
	if old := s.records[device]; old != nil {
		r.Activity = old.Activity
	}
	return s.put(r)
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
	if s.records[device] == nil {
		return nil
	}
	if _, err := s.db.Exec(`DELETE FROM push_records WHERE device = ?`, device); err != nil {
		return err
	}
	delete(s.records, device)
	return nil
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
	r := s.records[device]
	if r == nil {
		return Record{}, false
	}
	return clone(r), true
}

// Records are every registration, by device id.
func (s *Store) Records() []Record {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]Record, 0, len(s.records))
	for _, r := range s.records {
		out = append(out, clone(r))
	}
	slices.SortFunc(out, func(a, b Record) int { return int(a.UpdatedAt - b.UpdatedAt) })
	return out
}

// Note keeps the last sends.
func (s *Store) Note(sent Sent) {
	s.mu.Lock()
	defer s.mu.Unlock()
	at := s.lastSend + 1
	if err := s.insertSend(at, sent); err != nil {
		log.Printf("push: a send isn't kept: %v", err)
		return
	}
	s.lastSend = at
	s.sends = append(s.sends, sent)
	if n := len(s.sends); n > keepSends {
		s.sends = slices.Clone(s.sends[n-keepSends:])
	}
}

// insertSend writes sent at position at and drops the sends past keepSends, in one transaction.
func (s *Store) insertSend(at int64, sent Sent) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`INSERT INTO push_sends (position, at, device, kind, agent, priority, outcome) VALUES (?, ?, ?, ?, ?, ?, ?)`,
		at, sent.At, sent.Device, sent.Kind, sent.Agent, sent.Priority, sent.Outcome); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM push_sends WHERE position <= ?`, at-keepSends); err != nil {
		return err
	}
	return tx.Commit()
}

func (s *Store) Sends() []Sent {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.sends)
}
