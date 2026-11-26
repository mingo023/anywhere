package automation

import (
	"cmp"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

const (
	Version = 1
	// history is how many runs an automation keeps unless Store.History says otherwise.
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

// diskAutomation keeps WebhookSecret, which proto omits.
type diskAutomation struct {
	proto.Automation
	WebhookSecret string `json:"webhookSecret,omitempty"`
}

type diskRun struct {
	proto.Run
	TurnAfter  int64  `json:"turnAfter,omitempty"`
	TurnPrompt string `json:"turnPrompt,omitempty"`
	TurnEntry  string `json:"turnEntry,omitempty"`
}

type diskFile struct {
	Version     int              `json:"version"`
	Automations []diskAutomation `json:"automations"`
	Runs        []diskRun        `json:"runs"`
}

func (f File) MarshalJSON() ([]byte, error) {
	d := diskFile{Version: f.Version}
	for _, a := range f.Automations {
		d.Automations = append(d.Automations, diskAutomation{a, a.WebhookSecret})
	}
	for _, r := range f.Runs {
		d.Runs = append(d.Runs, diskRun{r, r.TurnAfter, r.TurnPrompt, r.TurnEntry})
	}
	return json.Marshal(d)
}

func (f *File) UnmarshalJSON(raw []byte) error {
	var d diskFile
	if err := json.Unmarshal(raw, &d); err != nil {
		return err
	}
	*f = File{Version: d.Version}
	for _, a := range d.Automations {
		a.Automation.WebhookSecret = a.WebhookSecret
		f.Automations = append(f.Automations, a.Automation)
	}
	for _, r := range d.Runs {
		r.Run.TurnAfter, r.Run.TurnPrompt, r.Run.TurnEntry = r.TurnAfter, r.TurnPrompt, r.TurnEntry
		f.Runs = append(f.Runs, r.Run)
	}
	return nil
}

// Each change writes the rows it changed to pocketd.db.
// Runs are kept oldest first.
type Store struct {
	db    *sql.DB
	files func() registry.File
	// Grace and History are read on each claim and run, so a changed setting applies to the next one.
	Grace   func() time.Duration
	History func() int

	mu                sync.Mutex
	f                 File
	nextAuto, nextRun int64
	changed           chan struct{}
}

// An automation is saved only in a folder files places in a project.
func Open(db *sql.DB, files func() registry.File) (*Store, error) {
	s := &Store{db: db, files: files, changed: make(chan struct{})}
	var err error
	if s.f, s.nextAuto, s.nextRun, err = load(db); err != nil {
		return nil, fmt.Errorf("automations: %w", err)
	}
	return s, nil
}

func load(db *sql.DB) (f File, nextAuto, nextRun int64, err error) {
	f.Version = Version
	rows, err := db.Query(`SELECT a.id, COALESCE(a.tree, p.path), a.name, a.prompt, a.provider, a.schedule, a.enabled, a.next_run_at, a.access, a.new_worktree,
		a.base, a.model, a.effort, a.created_by, a.bound, a.webhook, a.webhook_secret, a.position
		FROM automations a JOIN projects p ON p.id = a.project_id ORDER BY a.position`)
	if err != nil {
		return f, 0, 0, err
	}
	defer rows.Close()
	for rows.Next() {
		var a proto.Automation
		var schedule string
		var pos int64
		if err := rows.Scan(&a.ID, &a.Folder, &a.Name, &a.Prompt, &a.Provider, &schedule, &a.Enabled, &a.NextRunAt, &a.Access, &a.NewWorktree,
			&a.Base, &a.Model, &a.Effort, &a.CreatedBy, &a.Bound, &a.Webhook, &a.WebhookSecret, &pos); err != nil {
			return f, 0, 0, err
		}
		if err := json.Unmarshal([]byte(schedule), &a.Schedule); err != nil {
			return f, 0, 0, fmt.Errorf("automation %s: %w", a.ID, err)
		}
		f.Automations = append(f.Automations, a)
		nextAuto = pos + 1
	}
	if err := rows.Err(); err != nil {
		return f, 0, 0, err
	}
	rows.Close()
	if rows, err = db.Query(`SELECT id, automation_id, status, trigger, why, summary, started_at, finished_at, agent_id, terminal_id, access, worktree,
		turn_after, turn_prompt, turn_entry, position FROM runs ORDER BY position`); err != nil {
		return f, 0, 0, err
	}
	defer rows.Close()
	for rows.Next() {
		var r proto.Run
		var pos int64
		if err := rows.Scan(&r.ID, &r.AutomationID, &r.Status, &r.Trigger, &r.Why, &r.Summary, &r.StartedAt, &r.FinishedAt, &r.AgentID, &r.TerminalID,
			&r.Access, &r.Worktree, &r.TurnAfter, &r.TurnPrompt, &r.TurnEntry, &pos); err != nil {
			return f, 0, 0, err
		}
		f.Runs = append(f.Runs, r)
		nextRun = pos + 1
	}
	return f, nextAuto, nextRun, rows.Err()
}

// Reload takes the automations and runs the database holds, which are fewer after a project's removal
// cascaded, and wakes whoever waits on Changed when they are.
func (s *Store) Reload() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next, nextAuto, nextRun, err := load(s.db)
	if err != nil || len(next.Automations) == len(s.f.Automations) && len(next.Runs) == len(s.f.Runs) {
		return err
	}
	s.f, s.nextAuto, s.nextRun = next, nextAuto, nextRun
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
	nextAuto, nextRun, err := s.write(next)
	if err != nil {
		return err
	}
	s.f, s.nextAuto, s.nextRun = next, nextAuto, nextRun
	s.wake()
	return nil
}

// One transaction.
func (s *Store) write(next File) (nextAuto, nextRun int64, err error) {
	// Read before the transaction holds the one connection: a registry write holds its lock waiting for it.
	files := s.files()
	tx, err := s.db.Begin()
	if err != nil {
		return 0, 0, err
	}
	defer tx.Rollback()
	prev := make(map[string]proto.Automation, len(s.f.Automations))
	for _, a := range s.f.Automations {
		prev[a.ID] = a
	}
	nextAuto = s.nextAuto
	kept := make(map[string]bool, len(next.Automations))
	for _, a := range next.Automations {
		kept[a.ID] = true
		old, had := prev[a.ID]
		switch {
		case !had:
			err = insert(tx, files, a, nextAuto)
			nextAuto++
		case !reflect.DeepEqual(a, old):
			err = update(tx, files, a, old.Folder)
		}
		if err != nil {
			return 0, 0, err
		}
	}
	for id := range prev {
		if !kept[id] {
			if _, err := tx.Exec(`DELETE FROM automations WHERE id = ?`, id); err != nil {
				return 0, 0, err
			}
		}
	}
	prevRuns := make(map[string]proto.Run, len(s.f.Runs))
	for _, r := range s.f.Runs {
		prevRuns[r.ID] = r
	}
	nextRun = s.nextRun
	keptRuns := make(map[string]bool, len(next.Runs))
	for _, r := range next.Runs {
		keptRuns[r.ID] = true
		old, had := prevRuns[r.ID]
		switch {
		case !had:
			_, err = tx.Exec(`INSERT INTO runs (id, automation_id, status, trigger, why, summary, started_at, finished_at, agent_id, terminal_id,
				access, worktree, turn_after, turn_prompt, turn_entry, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				r.ID, r.AutomationID, r.Status, r.Trigger, r.Why, r.Summary, r.StartedAt, r.FinishedAt, r.AgentID, r.TerminalID,
				r.Access, r.Worktree, r.TurnAfter, r.TurnPrompt, r.TurnEntry, nextRun)
			nextRun++
		case r != old:
			_, err = tx.Exec(`UPDATE runs SET status = ?, trigger = ?, why = ?, summary = ?, started_at = ?, finished_at = ?, agent_id = ?, terminal_id = ?,
				access = ?, worktree = ?, turn_after = ?, turn_prompt = ?, turn_entry = ? WHERE id = ?`,
				r.Status, r.Trigger, r.Why, r.Summary, r.StartedAt, r.FinishedAt, r.AgentID, r.TerminalID,
				r.Access, r.Worktree, r.TurnAfter, r.TurnPrompt, r.TurnEntry, r.ID)
		}
		if err != nil {
			return 0, 0, err
		}
	}
	for id, r := range prevRuns {
		// A deleted automation's runs went with it.
		if !keptRuns[id] && kept[r.AutomationID] {
			if _, err := tx.Exec(`DELETE FROM runs WHERE id = ?`, id); err != nil {
				return 0, 0, err
			}
		}
	}
	return nextAuto, nextRun, tx.Commit()
}

func place(files registry.File, folder string) (string, *string, error) {
	id, tree, ok := worktree.Belong(files, folder)
	if !ok {
		return "", nil, registry.ErrUnknownProject
	}
	return id, tree, nil
}

func insert(tx *sql.Tx, files registry.File, a proto.Automation, pos int64) error {
	id, tree, err := place(files, a.Folder)
	if err != nil {
		return err
	}
	_, err = tx.Exec(`INSERT INTO automations (id, project_id, tree, name, prompt, provider, schedule, enabled, next_run_at, access, new_worktree,
		base, model, effort, created_by, bound, webhook, webhook_secret, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		a.ID, id, tree, a.Name, a.Prompt, a.Provider, scheduleJSON(a.Schedule), a.Enabled, a.NextRunAt, a.Access, a.NewWorktree,
		a.Base, a.Model, a.Effort, a.CreatedBy, a.Bound, a.Webhook, a.WebhookSecret, pos)
	return err
}

// update writes a's columns, and its project and tree when its folder moved from was.
func update(tx *sql.Tx, files registry.File, a proto.Automation, was string) error {
	if a.Folder != was {
		id, tree, err := place(files, a.Folder)
		if err != nil {
			return err
		}
		if _, err := tx.Exec(`UPDATE automations SET project_id = ?, tree = ? WHERE id = ?`, id, tree, a.ID); err != nil {
			return err
		}
	}
	_, err := tx.Exec(`UPDATE automations SET name = ?, prompt = ?, provider = ?, schedule = ?, enabled = ?, next_run_at = ?, access = ?, new_worktree = ?,
		base = ?, model = ?, effort = ?, created_by = ?, bound = ?, webhook = ?, webhook_secret = ? WHERE id = ?`,
		a.Name, a.Prompt, a.Provider, scheduleJSON(a.Schedule), a.Enabled, a.NextRunAt, a.Access, a.NewWorktree,
		a.Base, a.Model, a.Effort, a.CreatedBy, a.Bound, a.Webhook, a.WebhookSecret, a.ID)
	return err
}

func scheduleJSON(s proto.Schedule) string {
	raw, _ := json.Marshal(s)
	return string(raw)
}

func nextRunAt(a proto.Automation, now time.Time) int64 {
	if !a.Enabled || a.Schedule.Kind == "webhook" {
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
// and records the run: skipped when later than the grace allows or while another run
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
		case late(due, now, s.grace()):
			run.Status, run.Why, run.StartedAt = "skipped", "Mac asleep", due.UnixMilli()
		case f.active(id):
			run.Status, run.Why, run.StartedAt = "skipped", "Still working", due.UnixMilli()
		}
		f.addRun(run, s.history())
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
		f.addRun(run, s.history())
		return nil
	})
	return run, err
}

func (s *Store) grace() time.Duration {
	if s.Grace == nil {
		return Grace
	}
	return s.Grace()
}

func (s *Store) history() int {
	if s.History == nil {
		return history
	}
	return s.History()
}

// addRun appends r and drops the automation's oldest runs beyond keep.
func (f *File) addRun(r proto.Run, keep int) {
	f.Runs = append(f.Runs, r)
	over := 0
	for _, x := range f.Runs {
		if x.AutomationID == r.AutomationID {
			over++
		}
	}
	over -= keep
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
