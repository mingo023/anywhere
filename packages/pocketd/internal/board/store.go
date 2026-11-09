// Package board keeps the tickets of the Board and starts them, at most a
// limit at a time, each in its own Worktree.
package board

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

const Version = 1

var (
	ErrInvalid = errors.New("This ticket isn't valid")
	ErrUnknown = errors.New("This ticket doesn't exist anymore")
	ErrBusy    = errors.New("This ticket can't do that right now")
	ErrStarted = errors.New("This ticket has already started, so it can't be edited")

	errNoChange = errors.New("no change")
)

type File struct {
	Version int            `json:"version"`
	Limit   int            `json:"limit"`
	Tickets []proto.Ticket `json:"tickets"`
}

// Store is the Board, saved whole to Home/state/board.json on every change.
type Store struct {
	path string

	mu      sync.Mutex
	f       File
	changed chan struct{}
}

// Open reads home/state/board.json. A file it can't use is renamed to .bad,
// so the next save doesn't erase it, and the store starts empty.
func Open(home string) *Store {
	s := &Store{path: filepath.Join(home, "state", "board.json"), f: File{Version: Version, Limit: proto.DefaultBoardLimit}, changed: make(chan struct{})}
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
		log.Printf("board %s: %v; starting empty", s.path, err)
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

func (s *Store) Snapshot() (tickets []proto.Ticket, limit int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.f.Tickets), s.f.Limit
}

func (f File) find(id string) int {
	return slices.IndexFunc(f.Tickets, func(t proto.Ticket) bool { return t.ID == id })
}

func (f File) nextOrder() int64 {
	var top int64
	for _, t := range f.Tickets {
		top = max(top, t.Order)
	}
	return top + 1
}

// commit applies change to a copy and saves it; the store changes only if the
// save worked. A change that returns errNoChange saves nothing.
func (s *Store) commit(change func(f *File) error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next := File{Version: Version, Limit: s.f.Limit, Tickets: slices.Clone(s.f.Tickets)}
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

func (f *File) enqueue(t *proto.Ticket, now time.Time) {
	t.State, t.Order, t.Status, t.Why, t.AgentID, t.TerminalID, t.ChangedAt = proto.TicketQueued, f.nextOrder(), "", "", "", "", now.UnixMilli()
}

// Save adds t to To do, or edits the ticket with t's ID while it has not
// started; start also queues it. Only what the dialog edits is taken from t.
func (s *Store) Save(t proto.Ticket, start bool, now time.Time) (proto.Ticket, error) {
	t.Title, t.Prompt = strings.TrimSpace(t.Title), strings.TrimSpace(t.Prompt)
	if !t.Valid() {
		return t, ErrInvalid
	}
	err := s.commit(func(f *File) error {
		if t.ID == "" {
			t = proto.Ticket{ID: terminal.NewID(), Title: t.Title, Prompt: t.Prompt, Provider: t.Provider, Folder: t.Folder, Base: t.Base,
				State: proto.TicketTodo, Order: f.nextOrder(), CreatedAt: now.UnixMilli(), ChangedAt: now.UnixMilli()}
			if start {
				f.enqueue(&t, now)
			}
			f.Tickets = append(f.Tickets, t)
			return nil
		}
		i := f.find(t.ID)
		if i < 0 {
			return ErrUnknown
		}
		x := &f.Tickets[i]
		if x.State != proto.TicketTodo && x.State != proto.TicketQueued {
			return ErrStarted
		}
		x.Title, x.Prompt, x.Provider, x.Base, x.ChangedAt = t.Title, t.Prompt, t.Provider, t.Base, now.UnixMilli()
		if start && x.State == proto.TicketTodo {
			f.enqueue(x, now)
		}
		t = *x
		return nil
	})
	return t, err
}

// Delete removes a ticket that has no session.
func (s *Store) Delete(id string) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		switch {
		case i < 0:
			return ErrUnknown
		case f.Tickets[i].State == proto.TicketLive:
			return ErrBusy
		}
		f.Tickets = slices.Delete(f.Tickets, i, i+1)
		return nil
	})
}

// ended is whether t's session is gone for good, so it can start again.
func ended(t proto.Ticket) bool {
	return t.State == proto.TicketLive && (t.Status == proto.TicketStopped || t.Status == proto.TicketFailed && t.AgentID == "")
}

// Start queues a To do ticket. One whose session ended launches again right
// away in the Worktree it has: it already started, so it doesn't wait in line.
func (s *Store) Start(id string, now time.Time) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		t := &f.Tickets[i]
		switch {
		case t.State == proto.TicketTodo:
			f.enqueue(t, now)
		case ended(*t):
			t.Status, t.Why, t.AgentID, t.TerminalID, t.Relaunch, t.ChangedAt = proto.TicketStarting, "", "", "", true, now.UnixMilli()
		default:
			return ErrBusy
		}
		return nil
	})
}

// Move puts an unstarted ticket in column just before the ticket with ID
// before, or last when there is none.
func (s *Store) Move(id, column, before string, now time.Time) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		t := &f.Tickets[i]
		if t.State != proto.TicketTodo && t.State != proto.TicketQueued {
			return ErrBusy
		}
		t.State, t.ChangedAt = column, now.UnixMilli()
		var line []*proto.Ticket
		for j := range f.Tickets {
			if x := &f.Tickets[j]; j != i && x.State == column {
				line = append(line, x)
			}
		}
		slices.SortFunc(line, func(a, b *proto.Ticket) int { return cmp.Compare(a.Order, b.Order) })
		at := slices.IndexFunc(line, func(x *proto.Ticket) bool { return x.ID == before })
		if at < 0 {
			at = len(line)
		}
		line = slices.Insert(line, at, t)
		// Every ticket starts above all the others, so numbers only need be right within a column.
		for n, x := range line {
			x.Order = int64(n)
		}
		return nil
	})
}

// Close takes a live ticket off the working columns; its session and Worktree stay.
func (s *Store) Close(id, why string, now time.Time) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		t := &f.Tickets[i]
		if t.State != proto.TicketLive {
			return ErrBusy
		}
		t.State, t.ClosedWhy, t.ClosedAt = proto.TicketClosed, why, now.UnixMilli()
		return nil
	})
}

func gone(path string) bool {
	_, err := os.Stat(path)
	return errors.Is(err, os.ErrNotExist)
}

// Reopen brings a closed ticket back with the status it had, unless what
// closed it still holds: a merged PR or a Worktree that is gone.
func (s *Store) Reopen(id string, now time.Time) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		t := &f.Tickets[i]
		if t.State != proto.TicketClosed || t.ClosedWhy != proto.ClosedByUser || t.Worktree != "" && gone(t.Worktree) {
			return ErrBusy
		}
		t.State, t.ClosedWhy, t.ClosedAt, t.ChangedAt = proto.TicketLive, "", 0, now.UnixMilli()
		return nil
	})
}

func (s *Store) SetLimit(n int) error {
	if n < proto.MinBoardLimit || n > proto.MaxBoardLimit {
		return ErrInvalid
	}
	return s.commit(func(f *File) error {
		if f.Limit == n {
			return errNoChange
		}
		f.Limit = n
		return nil
	})
}

// Claim returns the tickets to launch now: every one asked to start again,
// then the first in line that fit under the limit, marked as starting.
func (s *Store) Claim(now time.Time) []proto.Ticket {
	var claimed []proto.Ticket
	err := s.commit(func(f *File) error {
		claimed = nil
		busy := 0
		var queued []int
		for i, t := range f.Tickets {
			if t.HoldsSlot() {
				busy++
			}
			if t.State == proto.TicketQueued {
				queued = append(queued, i)
			}
			if t.Relaunch {
				f.Tickets[i].Relaunch = false
				claimed = append(claimed, f.Tickets[i])
			}
		}
		slices.SortFunc(queued, func(a, b int) int { return cmp.Compare(f.Tickets[a].Order, f.Tickets[b].Order) })
		for _, i := range queued {
			if busy >= f.Limit {
				break
			}
			t := &f.Tickets[i]
			t.State, t.Status, t.ChangedAt = proto.TicketLive, proto.TicketStarting, now.UnixMilli()
			claimed = append(claimed, *t)
			busy++
		}
		if len(claimed) == 0 {
			return errNoChange
		}
		return nil
	})
	if err != nil {
		log.Printf("board: claim: %v", err)
		return nil
	}
	return claimed
}

// Update changes a ticket in place; one deleted meanwhile is left alone.
func (s *Store) Update(id string, change func(*proto.Ticket)) error {
	return s.commit(func(f *File) error {
		i := f.find(id)
		if i < 0 {
			return ErrUnknown
		}
		before := f.Tickets[i]
		change(&f.Tickets[i])
		if f.Tickets[i] == before {
			return errNoChange
		}
		return nil
	})
}

// Reconcile fails the tickets a stopped pocketd left starting with no agent:
// whether their session started is unknown, and starting again could run the prompt twice.
func (s *Store) Reconcile(now time.Time) error {
	return s.commit(func(f *File) error {
		n := 0
		for i, t := range f.Tickets {
			if t.State == proto.TicketLive && t.Status == proto.TicketStarting && t.AgentID == "" {
				f.Tickets[i].Status, f.Tickets[i].Why, f.Tickets[i].ChangedAt, f.Tickets[i].Relaunch = proto.TicketFailed, "Interrupted", now.UnixMilli(), false
				n++
			}
		}
		if n == 0 {
			return errNoChange
		}
		return nil
	})
}
