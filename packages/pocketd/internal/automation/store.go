package automation

import (
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

const (
	Version = 1
	// history is how many runs an automation keeps.
	history = 50
)

var (
	ErrInvalid = errors.New("This automation isn't valid")
	ErrUnknown = errors.New("This automation doesn't exist anymore")
	ErrBusy    = errors.New("This automation is already working")

	errNoChange = errors.New("no change")
)

type File struct {
	Version     int                `json:"version"`
	Automations []proto.Automation `json:"automations"`
	Runs        []proto.Run        `json:"runs"`
}

// Store is the automations and their runs, saved whole to Home/state/automations.json
// on every change. Runs are kept oldest first.
type Store struct {
	path string

	mu      sync.Mutex
	f       File
	changed chan struct{}
}

// Open reads home/state/automations.json. A file it can't use is renamed to
// .bad, so the next save doesn't erase it, and the store starts empty.
func Open(home string) *Store {
	s := &Store{path: filepath.Join(home, "state", "automations.json"), f: File{Version: Version}, changed: make(chan struct{})}
	raw, err := os.ReadFile(s.path)
	if errors.Is(err, fs.ErrNotExist) {
		return s
	}
	var f File
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err == nil && f.Version != Version {
		err = fmt.Errorf("version %d, want %d", f.Version, Version)
	}
	if err != nil {
		log.Printf("automations %s: %v; starting empty", s.path, err)
		if rerr := os.Rename(s.path, s.path+".bad"); rerr != nil {
			log.Print(rerr)
		}
		return s
	}
	s.f = f
	return s
}

// Changed closes at the next save; take a new one after each.
func (s *Store) Changed() <-chan struct{} {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.changed
}

// Snapshot is the automations and, newest first, every run.
func (s *Store) Snapshot() ([]proto.Automation, []proto.Run) {
	s.mu.Lock()
	defer s.mu.Unlock()
	runs := slices.Clone(s.f.Runs)
	slices.Reverse(runs)
	slices.SortStableFunc(runs, func(a, b proto.Run) int { return cmp.Compare(b.StartedAt, a.StartedAt) })
	return slices.Clone(s.f.Automations), runs
}

func (s *Store) Automation(id string) (proto.Automation, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if i := s.f.find(id); i >= 0 {
		return s.f.Automations[i], true
	}
	return proto.Automation{}, false
}

func (s *Store) Run(id string) (proto.Run, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if i := s.f.findRun(id); i >= 0 {
		return s.f.Runs[i], true
	}
	return proto.Run{}, false
}

func (f File) find(id string) int {
	return slices.IndexFunc(f.Automations, func(a proto.Automation) bool { return a.ID == id })
}

func (f File) findRun(id string) int {
	return slices.IndexFunc(f.Runs, func(r proto.Run) bool { return r.ID == id })
}

func (f File) active(automationID string) bool {
	return slices.ContainsFunc(f.Runs, func(r proto.Run) bool { return r.AutomationID == automationID && activeStatus(r.Status) })
}

// activeStatus is a status of a run that has started and not ended.
func activeStatus(status string) bool {
	return status == "pending" || status == "running" || status == "waiting"
}

// commit applies change to a copy and saves it; the store changes only if the
// save worked. A change that returns errNoChange saves nothing.
func (s *Store) commit(change func(f *File) error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next := File{Version: Version, Automations: slices.Clone(s.f.Automations), Runs: slices.Clone(s.f.Runs)}
	if err := change(&next); err != nil {
		if errors.Is(err, errNoChange) {
			return nil
		}
		return err
	}
	raw, err := json.MarshalIndent(next, "", "  ")
	if err == nil {
		err = os.MkdirAll(filepath.Dir(s.path), 0o700)
	}
	if err == nil {
		err = atomicfile.Write(s.path, raw, 0o600)
	}
	if err != nil {
		return err
	}
	s.f = next
	close(s.changed)
	s.changed = make(chan struct{})
	return nil
}

func nextRunAt(a proto.Automation, now time.Time) int64 {
	if !a.Enabled {
		return 0
	}
	return nextAfter(a.Schedule, now).UnixMilli()
}

// Save adds a, or replaces the automation with a's ID, and schedules its next
// run from now. An empty ID gets a new one.
func (s *Store) Save(a proto.Automation, now time.Time) (proto.Automation, error) {
	a.Name = strings.TrimSpace(a.Name)
	if !a.Valid() {
		return a, ErrInvalid
	}
	a.NextRunAt = nextRunAt(a, now)
	err := s.commit(func(f *File) error {
		i := f.find(a.ID)
		switch {
		case a.ID == "":
			a.ID = terminal.NewID()
			f.Automations = append(f.Automations, a)
		case i < 0:
			return ErrUnknown
		default:
			f.Automations[i] = a
		}
		return nil
	})
	return a, err
}

// Enable turns an automation on, scheduling its next run from now, or off.
func (s *Store) Enable(id string, on bool, now time.Time) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		a := &f.Automations[i]
		if a.Enabled == on {
			return errNoChange
		}
		a.Enabled = on
		a.NextRunAt = nextRunAt(*a, now)
		return nil
	})
}

// Delete removes an automation and its runs.
func (s *Store) Delete(id string) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		f.Automations = slices.Delete(f.Automations, i, i+1)
		f.Runs = slices.DeleteFunc(f.Runs, func(r proto.Run) bool { return r.AutomationID == id })
		return nil
	})
}

// Claim takes the run that was due at due. It wins only while the stored
// next run is still due and the automation is on, so of two claims on the same
// time one fails, also across a restart. It moves the next run to after now
// and records the run: skipped when more than Grace late or while another run
// is active, else pending.
func (s *Store) Claim(id string, due, now time.Time) (proto.Run, bool) {
	var run proto.Run
	err := s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 || !f.Automations[i].Enabled || f.Automations[i].NextRunAt != due.UnixMilli() {
			return ErrUnknown
		}
		f.Automations[i].NextRunAt = nextRunAt(f.Automations[i], now)
		run = proto.Run{ID: terminal.NewID(), AutomationID: id, Status: "pending", Trigger: "schedule", StartedAt: now.UnixMilli()}
		switch {
		case late(due, now):
			run.Status, run.Why, run.StartedAt = "skipped", "Mac asleep", due.UnixMilli()
		case f.active(id):
			run.Status, run.Why, run.StartedAt = "skipped", "Still working", due.UnixMilli()
		}
		f.addRun(run)
		return nil
	})
	return run, err == nil
}

// Begin records a manual run, leaving the next scheduled one alone.
func (s *Store) Begin(id string, now time.Time) (proto.Run, error) {
	var run proto.Run
	err := s.commit(func(f *File) error {
		switch {
		case f.find(id) < 0:
			return ErrUnknown
		case f.active(id):
			return ErrBusy
		}
		run = proto.Run{ID: terminal.NewID(), AutomationID: id, Status: "pending", Trigger: "manual", StartedAt: now.UnixMilli()}
		f.addRun(run)
		return nil
	})
	return run, err
}

// addRun appends r and drops the automation's oldest runs beyond history.
func (f *File) addRun(r proto.Run) {
	f.Runs = append(f.Runs, r)
	over := 0
	for _, x := range f.Runs {
		if x.AutomationID == r.AutomationID {
			over++
		}
	}
	over -= history
	f.Runs = slices.DeleteFunc(f.Runs, func(x proto.Run) bool {
		if x.AutomationID != r.AutomationID || over <= 0 {
			return false
		}
		over--
		return true
	})
}

// Update changes a run in place; a run deleted meanwhile is left alone.
func (s *Store) Update(runID string, change func(*proto.Run)) error {
	return s.commit(func(f *File) error {
		i := f.findRun(runID)
		if i < 0 {
			return ErrUnknown
		}
		before := f.Runs[i]
		change(&f.Runs[i])
		if f.Runs[i] == before {
			return errNoChange
		}
		return nil
	})
}

// Reconcile fails the runs a stopped pocketd left pending: whether their
// session started is unknown, and starting again could run the prompt twice.
func (s *Store) Reconcile(now time.Time) error {
	return s.commit(func(f *File) error {
		n := 0
		for i, r := range f.Runs {
			if r.Status == "pending" {
				f.Runs[i].Status, f.Runs[i].Why, f.Runs[i].FinishedAt = "failed", "Interrupted", now.UnixMilli()
				n++
			}
		}
		if n == 0 {
			return errNoChange
		}
		return nil
	})
}
