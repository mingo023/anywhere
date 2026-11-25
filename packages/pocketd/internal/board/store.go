// Package board keeps the tickets of the Board, in pocketd.db, and starts them, at most a
// limit at a time, each in its own Worktree.
package board

import (
	"cmp"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"os"
	"reflect"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
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
	// LastNumber outlives its ticket, so a deleted ticket's number isn't given to another.
	LastNumber int64 `json:"lastNumber,omitempty"`
}

// Each change writes the rows it changed to pocketd.db.
type Store struct {
	db    *sql.DB
	files func() registry.File

	mu      sync.Mutex
	f       File
	nextPos int64
	changed chan struct{}
}

// A ticket is saved only in a folder files places in a project.
func Open(db *sql.DB, files func() registry.File) (*Store, error) {
	s := &Store{db: db, files: files, changed: make(chan struct{})}
	var err error
	if s.f, s.nextPos, err = load(db); err != nil {
		return nil, fmt.Errorf("board: %w", err)
	}
	return s, nil
}

func load(db *sql.DB) (File, int64, error) {
	f := File{Version: Version, Limit: proto.DefaultBoardLimit}
	meta, err := loadMeta(db)
	if err != nil {
		return f, 0, err
	}
	if v, ok := meta["board.limit"]; ok {
		if f.Limit, err = strconv.Atoi(v); err != nil {
			return f, 0, fmt.Errorf("board.limit: %w", err)
		}
	}
	if v, ok := meta["board.last_number"]; ok {
		if f.LastNumber, err = strconv.ParseInt(v, 10, 64); err != nil {
			return f, 0, fmt.Errorf("board.last_number: %w", err)
		}
	}
	rows, err := db.Query(`SELECT t.id, COALESCE(t.tree, p.path), t.title, t.prompt, t.provider, t.base, t.state, t.sort_order, t.status, t.why, t.agent_id,
		t.terminal_id, t.worktree, t.created_at, t.changed_at, t.closed_at, t.closed_why, t.relaunch, t.number, t.priority, t.labels, t.position
		FROM tickets t JOIN projects p ON p.id = t.project_id ORDER BY t.position`)
	if err != nil {
		return f, 0, err
	}
	defer rows.Close()
	var next int64
	for rows.Next() {
		var t proto.Ticket
		var labels string
		var pos int64
		if err := rows.Scan(&t.ID, &t.Folder, &t.Title, &t.Prompt, &t.Provider, &t.Base, &t.State, &t.Order, &t.Status, &t.Why, &t.AgentID,
			&t.TerminalID, &t.Worktree, &t.CreatedAt, &t.ChangedAt, &t.ClosedAt, &t.ClosedWhy, &t.Relaunch, &t.Number, &t.Priority, &labels, &pos); err != nil {
			return f, 0, err
		}
		if err := json.Unmarshal([]byte(labels), &t.Labels); err != nil {
			return f, 0, fmt.Errorf("ticket %s: %w", t.ID, err)
		}
		if len(t.Labels) == 0 {
			t.Labels = nil
		}
		f.Tickets = append(f.Tickets, t)
		next = pos + 1
	}
	return f, next, rows.Err()
}

func loadMeta(db *sql.DB) (map[string]string, error) {
	rows, err := db.Query(`SELECT key, value FROM meta WHERE key IN ('board.limit', 'board.last_number')`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	meta := map[string]string{}
	for rows.Next() {
		var k, v string
		if err := rows.Scan(&k, &v); err != nil {
			return nil, err
		}
		meta[k] = v
	}
	return meta, rows.Err()
}

// Reload takes the tickets the database holds, which differ after a project's removal cascaded, and wakes
// whoever waits on Changed when they did.
func (s *Store) Reload() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next, pos, err := load(s.db)
	if err != nil || slices.EqualFunc(next.Tickets, s.f.Tickets, func(a, b proto.Ticket) bool { return reflect.DeepEqual(a, b) }) {
		return err
	}
	s.f.Tickets, s.nextPos = next.Tickets, pos
	s.wake()
	return nil
}

func (s *Store) wake() {
	close(s.changed)
	s.changed = make(chan struct{})
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

// NextNumber returns the next ticket number and records it as the last one.
func (f *File) NextNumber() int64 {
	for _, t := range f.Tickets {
		f.LastNumber = max(f.LastNumber, t.Number)
	}
	f.LastNumber++
	return f.LastNumber
}

// commit applies change to a copy and saves it; the store changes only if the
// save worked. A change that returns errNoChange saves nothing.
func (s *Store) commit(change func(f *File) error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next := File{Version: Version, Limit: s.f.Limit, Tickets: slices.Clone(s.f.Tickets), LastNumber: s.f.LastNumber}
	if err := change(&next); err != nil {
		if errors.Is(err, errNoChange) {
			return nil
		}
		return err
	}
	pos, err := s.write(next)
	if err != nil {
		return err
	}
	s.f, s.nextPos = next, pos
	s.wake()
	return nil
}

// One transaction.
func (s *Store) write(next File) (int64, error) {
	// Read before the transaction holds the one connection: a registry write holds its lock waiting for it.
	files := s.files()
	tx, err := s.db.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()
	prev := make(map[string]proto.Ticket, len(s.f.Tickets))
	for _, t := range s.f.Tickets {
		prev[t.ID] = t
	}
	pos := s.nextPos
	kept := make(map[string]bool, len(next.Tickets))
	for _, t := range next.Tickets {
		kept[t.ID] = true
		old, had := prev[t.ID]
		switch {
		case !had:
			if err := insert(tx, files, t, pos); err != nil {
				return 0, err
			}
			pos++
		case !reflect.DeepEqual(t, old):
			if err := update(tx, files, t, old.Folder); err != nil {
				return 0, err
			}
		}
	}
	for id := range prev {
		if !kept[id] {
			if _, err := tx.Exec(`DELETE FROM tickets WHERE id = ?`, id); err != nil {
				return 0, err
			}
		}
	}
	for key, v := range map[string]struct {
		was, now int64
	}{"board.limit": {int64(s.f.Limit), int64(next.Limit)}, "board.last_number": {s.f.LastNumber, next.LastNumber}} {
		if v.was == v.now {
			continue
		}
		if _, err := tx.Exec(`INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value`, key, strconv.FormatInt(v.now, 10)); err != nil {
			return 0, err
		}
	}
	return pos, tx.Commit()
}

func place(files registry.File, folder string) (string, *string, error) {
	id, tree, ok := worktree.Belong(files, folder)
	if !ok {
		return "", nil, registry.ErrUnknownProject
	}
	return id, tree, nil
}

func insert(tx *sql.Tx, files registry.File, t proto.Ticket, pos int64) error {
	id, tree, err := place(files, t.Folder)
	if err != nil {
		return err
	}
	_, err = tx.Exec(`INSERT INTO tickets (id, project_id, tree, title, prompt, provider, base, state, sort_order, status, why, agent_id, terminal_id,
		worktree, created_at, changed_at, closed_at, closed_why, relaunch, number, priority, labels, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		t.ID, id, tree, t.Title, t.Prompt, t.Provider, t.Base, t.State, t.Order, t.Status, t.Why, t.AgentID, t.TerminalID,
		t.Worktree, t.CreatedAt, t.ChangedAt, t.ClosedAt, t.ClosedWhy, t.Relaunch, t.Number, t.Priority, labelsJSON(t.Labels), pos)
	return err
}

// update writes t's columns, and its project and tree when its folder moved from was.
func update(tx *sql.Tx, files registry.File, t proto.Ticket, was string) error {
	if t.Folder != was {
		id, tree, err := place(files, t.Folder)
		if err != nil {
			return err
		}
		if _, err := tx.Exec(`UPDATE tickets SET project_id = ?, tree = ? WHERE id = ?`, id, tree, t.ID); err != nil {
			return err
		}
	}
	_, err := tx.Exec(`UPDATE tickets SET title = ?, prompt = ?, provider = ?, base = ?, state = ?, sort_order = ?, status = ?, why = ?, agent_id = ?, terminal_id = ?,
		worktree = ?, created_at = ?, changed_at = ?, closed_at = ?, closed_why = ?, relaunch = ?, number = ?, priority = ?, labels = ? WHERE id = ?`,
		t.Title, t.Prompt, t.Provider, t.Base, t.State, t.Order, t.Status, t.Why, t.AgentID, t.TerminalID,
		t.Worktree, t.CreatedAt, t.ChangedAt, t.ClosedAt, t.ClosedWhy, t.Relaunch, t.Number, t.Priority, labelsJSON(t.Labels), t.ID)
	return err
}

func labelsJSON(labels []string) string {
	if labels == nil {
		labels = []string{}
	}
	raw, _ := json.Marshal(labels)
	return string(raw)
}

func (f *File) enqueue(t *proto.Ticket, now time.Time) {
	t.State, t.Order, t.Status, t.Why, t.AgentID, t.TerminalID, t.ChangedAt = proto.TicketQueued, f.nextOrder(), "", "", "", "", now.UnixMilli()
}

// Save adds t to To do, or edits the ticket with t's ID while it has not
// started; start also queues it. Only what the ticket page edits is taken from t.
// A started ticket takes a new priority and labels, and nothing else.
func (s *Store) Save(t proto.Ticket, start bool, now time.Time) (proto.Ticket, error) {
	t.Title, t.Prompt = strings.TrimSpace(t.Title), strings.TrimSpace(t.Prompt)
	if !t.Valid() {
		return t, ErrInvalid
	}
	err := s.commit(func(f *File) error {
		if t.ID == "" {
			t = proto.Ticket{ID: terminal.NewID(), Title: t.Title, Prompt: t.Prompt, Provider: t.Provider, Folder: t.Folder, Base: t.Base,
				Number: f.NextNumber(), Priority: t.Priority, Labels: t.Labels,
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
			if start || t.Title != x.Title || t.Prompt != x.Prompt || t.Provider != x.Provider || t.Folder != x.Folder || t.Base != x.Base {
				return ErrStarted
			}
			x.Priority, x.Labels = t.Priority, t.Labels
			t = *x
			return nil
		}
		x.Title, x.Prompt, x.Provider, x.Folder, x.Base, x.Priority, x.Labels, x.ChangedAt = t.Title, t.Prompt, t.Provider, t.Folder, t.Base, t.Priority, t.Labels, now.UnixMilli()
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
		if reflect.DeepEqual(f.Tickets[i], before) {
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
