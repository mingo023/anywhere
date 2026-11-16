package orch

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"log"
	"strings"
	"sync"
	"time"
)

// Effector runs effects outside the store; the daemon implements it.
type Effector interface {
	Run(ctx context.Context, e Effect) (map[string]any, error)
}

// Change is what a committed command changed, for clients.
type Change struct {
	Delegations []Delegation
	Queues      []string // sessions whose queue changed
}

// Engine runs commands: one transaction per command, then its effects in
// each lane's order, at most one at a time per lane.
type Engine struct {
	store    *Store
	effector Effector
	// Limits and Agent are read on each command; nil uses the defaults and knows no agent.
	Limits func() Limits
	Agent  func(kind string) AgentInfo
	// OnChange hears each commit's changes, after it.
	OnChange func(Change)
	// OnClosed hears each session that closed, after its delegation settled.
	OnClosed func(session string)
	// Live is a running session as the registry has it this instant; the
	// store's row may lag it by a write. Nil reads the store alone.
	Live func(id string) (Session, bool)
	Now  func() time.Time

	mu      sync.Mutex
	locks   map[string]*sync.Mutex
	lanes   map[string]*lane
	waits   map[int64]chan error
	stopped bool
	running sync.WaitGroup
	ctx     context.Context
	cancel  context.CancelFunc
}

func NewEngine(s *Store, eff Effector) *Engine {
	ctx, cancel := context.WithCancel(context.Background())
	return &Engine{store: s, effector: eff, locks: map[string]*sync.Mutex{}, lanes: map[string]*lane{}, waits: map[int64]chan error{}, ctx: ctx, cancel: cancel, Now: time.Now}
}

func (e *Engine) Store() *Store { return e.store }

func (e *Engine) lock(session string) func() {
	e.mu.Lock()
	m := e.locks[session]
	if m == nil {
		m = &sync.Mutex{}
		e.locks[session] = m
	}
	e.mu.Unlock()
	m.Lock()
	return m.Unlock
}

type Receipt struct {
	Result  json.RawMessage
	Effects []int64
	Replay  bool
}

func specHash(cmd Command) string {
	h := sha256.Sum256([]byte(cmd.Kind + "\x00" + cmd.Session + "\x00" + string(cmd.Payload)))
	return hex.EncodeToString(h[:])
}

func randomID() string {
	b := make([]byte, 8)
	rand.Read(b)
	return hex.EncodeToString(b)
}

// Dispatch commits cmd once. The same ID again returns the first receipt; the
// same ID with another spec is refused.
func (e *Engine) Dispatch(cmd Command) (Receipt, error) {
	keep := cmd.ID != ""
	if !keep {
		cmd.ID = randomID()
	}
	unlock := e.lock(cmd.Session)
	var r Receipt
	var d Decision
	var effects []Effect
	err := e.store.Tx(func(tx *Tx) error {
		if keep {
			var hash, result string
			err := tx.QueryRow(`SELECT spec_hash, result FROM receipts WHERE command_id = ?`, cmd.ID).Scan(&hash, &result)
			if err == nil {
				if hash != specHash(cmd) {
					return refuse(CodeInvalid, "This request id was already used with a different request")
				}
				r = Receipt{Result: json.RawMessage(result), Replay: true}
				return nil
			}
			if !errors.Is(err, sql.ErrNoRows) {
				return err
			}
		}
		st, err := e.load(tx, cmd)
		if err != nil {
			return err
		}
		d, err = decide(st, cmd)
		if err != nil {
			return err
		}
		effects, err = e.apply(tx, cmd, d)
		if err != nil {
			return err
		}
		res, _ := json.Marshal(d.Result)
		r.Result = res
		for _, x := range effects {
			r.Effects = append(r.Effects, x.ID)
		}
		if keep {
			_, err = tx.Exec(`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES (?, ?, ?, ?)`, cmd.ID, specHash(cmd), string(res), e.Now().UnixMilli())
		}
		return err
	})
	unlock()
	if err != nil || r.Replay {
		return r, err
	}
	e.changed(d)
	for _, x := range effects {
		e.kick(x.Lane)
	}
	for _, f := range d.Follow {
		if _, err := e.Dispatch(f); err != nil {
			log.Printf("orch: %s for %s: %v", f.Kind, f.Session, err)
		}
	}
	return r, nil
}

// DispatchWait is Dispatch that waits up to wait for the command's effects,
// returning the first one's error.
func (e *Engine) DispatchWait(cmd Command, wait time.Duration) (Receipt, error) {
	r, err := e.Dispatch(cmd)
	if err != nil {
		return r, err
	}
	return r, e.Wait(r.Effects, wait)
}

// Wait waits for effects to finish; the first error wins. Effects already
// done when it is called count as done.
func (e *Engine) Wait(ids []int64, wait time.Duration) error {
	deadline := time.After(wait)
	for _, id := range ids {
		ch := e.waiter(id)
		if ch == nil {
			continue
		}
		select {
		case err := <-ch:
			if err != nil {
				return err
			}
		case <-deadline:
			return refuse(CodeUnavailable, "The agent didn't take it in time")
		}
	}
	return nil
}

func (e *Engine) waiter(id int64) chan error {
	e.mu.Lock()
	ch, ok := e.waits[id]
	e.mu.Unlock()
	if ok {
		return ch
	}
	var state, last string
	if err := e.store.db.QueryRow(`SELECT state, last_error FROM outbox WHERE id = ?`, id).Scan(&state, &last); err != nil {
		return nil
	}
	if state == "failed" {
		ch := make(chan error, 1)
		ch <- errors.New(last)
		return ch
	}
	return nil
}

func (e *Engine) changed(d Decision) {
	if e.OnChange == nil {
		return
	}
	c := Change{Delegations: d.Delegations}
	seen := map[string]bool{}
	for _, q := range d.Queue {
		if !seen[q.Session] {
			seen[q.Session] = true
			c.Queues = append(c.Queues, q.Session)
		}
	}
	if len(c.Delegations) > 0 || len(c.Queues) > 0 {
		e.OnChange(c)
	}
}

func (e *Engine) apply(tx *Tx, cmd Command, d Decision) ([]Effect, error) {
	now := e.Now().UnixMilli()
	for _, ev := range d.Events {
		if err := event(tx, ev.Session, ev.Kind, ev.Payload); err != nil {
			return nil, err
		}
	}
	for _, t := range d.Delegations {
		if err := putDelegation(tx, t); err != nil {
			return nil, err
		}
	}
	for _, q := range d.Queue {
		refs, _ := json.Marshal(q.Refs)
		if _, err := tx.Exec(`INSERT INTO queue (id, session_id, text, origin, ord, state, refs, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET text = excluded.text, ord = excluded.ord, state = excluded.state, refs = excluded.refs`,
			q.ID, q.Session, q.Text, q.Origin, q.Ord, q.State, string(refs), q.CreatedAt); err != nil {
			return nil, err
		}
	}
	for _, r := range d.Requests {
		if _, err := tx.Exec(`INSERT INTO requests (id, session_id, kind, payload, state, created_at) VALUES (?, ?, ?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET state = excluded.state`, r.ID, r.Session, r.Kind, string(r.Payload), r.State, r.CreatedAt); err != nil {
			return nil, err
		}
	}
	for _, s := range d.Lineage {
		if _, err := tx.Exec(`INSERT INTO sessions (id, parent_id, relationship, depth, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET parent_id = excluded.parent_id, relationship = excluded.relationship, depth = excluded.depth, created_by = excluded.created_by`,
			s.ID, s.ParentID, s.Relationship, s.Depth, s.CreatedBy, now); err != nil {
			return nil, err
		}
	}
	for _, t := range d.Transfers {
		if _, err := tx.Exec(`INSERT INTO transfers (id, source_id, target_id, kind, source_point, state, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET target_id = excluded.target_id, state = excluded.state`, t.ID, t.Source, t.Target, t.Kind, t.SourcePoint, t.State, t.CreatedAt); err != nil {
			return nil, err
		}
	}
	for _, id := range d.Hold {
		if _, err := tx.Exec(`INSERT INTO held (session_id, task_id, until_at) VALUES (?, ?, ?) ON CONFLICT DO UPDATE SET until_at = excluded.until_at`, cmd.Session, id, now+HoldCap); err != nil {
			return nil, err
		}
	}
	for _, id := range d.Unhold {
		if _, err := tx.Exec(`DELETE FROM held WHERE session_id = ? AND task_id = ?`, cmd.Session, id); err != nil {
			return nil, err
		}
	}
	for _, id := range d.Archive {
		if _, err := tx.Exec(`UPDATE sessions SET archived = 1 WHERE id = ?`, id); err != nil {
			return nil, err
		}
	}
	out := make([]Effect, 0, len(d.Effects))
	for _, x := range d.Effects {
		x.Command = cmd.ID
		if x.Payload == nil {
			x.Payload = json.RawMessage("null")
		}
		res, err := tx.Exec(`INSERT INTO outbox (session_id, kind, payload, state, command_id, then_cmd, else_cmd, created_at) VALUES (?, ?, ?, 'pending', ?, ?, ?, ?)`,
			x.Lane, x.Kind, string(x.Payload), cmd.ID, x.Then, x.Else, now)
		if err != nil {
			return nil, err
		}
		x.ID, _ = res.LastInsertId()
		e.mu.Lock()
		e.waits[x.ID] = make(chan error, 1)
		e.mu.Unlock()
		out = append(out, x)
	}
	return out, nil
}

func putDelegation(tx *Tx, t Delegation) error {
	_, err := tx.Exec(`INSERT INTO delegations (task_id, parent_id, parent_turn, child_id, title, role, agent, brief, isolation, mode, status, work_state, summary, error, access, stepped_down, completion_wake, delivery, pending_question, client_request_id, created_at, settled_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (task_id) DO UPDATE SET child_id = excluded.child_id, title = excluded.title, status = excluded.status, work_state = excluded.work_state, summary = excluded.summary,
			error = excluded.error, completion_wake = excluded.completion_wake, delivery = excluded.delivery, pending_question = excluded.pending_question, settled_at = excluded.settled_at`,
		t.TaskID, t.ParentID, t.ParentTurn, t.ChildID, t.Title, t.Role, t.Agent, t.Brief, t.Isolation, t.Mode, t.Status, t.WorkState, t.Summary, t.Error, t.Access, t.SteppedDown,
		t.CompletionWake, t.Delivery, t.PendingQuestion, t.ClientRequestID, t.CreatedAt, t.SettledAt)
	return err
}

type peek struct {
	TaskID    string `json:"taskId"`
	RequestID string `json:"requestId"`
	TargetID  string `json:"targetId"`
	Agent     string `json:"agent"`
}

func (e *Engine) load(tx *Tx, cmd Command) (State, error) {
	st := State{Now: e.Now().UnixMilli(), Limits: DefaultLimits}
	if e.Limits != nil {
		st.Limits = e.Limits()
	}
	var p peek
	json.Unmarshal(cmd.Payload, &p)
	var err error
	if st.Session, err = e.session(tx, cmd.Session); err != nil {
		return st, err
	}
	agentInfo := func(kind string) AgentInfo {
		if e.Agent == nil || kind == "" {
			return AgentInfo{}
		}
		return e.Agent(kind)
	}
	if st.Session != nil {
		st.Self = agentInfo(st.Session.Agent)
		st.Target = st.Self
	}
	if p.Agent != "" {
		st.Target = agentInfo(p.Agent)
	}
	if st.Queue, err = loadQueue(tx, cmd.Session, true); err != nil {
		return st, err
	}
	if st.Children, err = loadDelegations(tx, `parent_id = ? ORDER BY created_at, task_id`, cmd.Session); err != nil {
		return st, err
	}
	switch {
	case p.TaskID != "":
		ts, err := loadDelegations(tx, `task_id = ?`, p.TaskID)
		if err != nil {
			return st, err
		}
		if len(ts) > 0 {
			st.Task = &ts[0]
		}
	case cmd.Kind == CmdRequestOpen || cmd.Kind == CmdRequestClose:
		ts, err := loadDelegations(tx, `child_id = ? ORDER BY created_at DESC LIMIT 1`, cmd.Session)
		if err != nil {
			return st, err
		}
		if len(ts) > 0 {
			st.Task = &ts[0]
		}
	}
	if st.Task != nil {
		if st.Parent, err = e.session(tx, st.Task.ParentID); err != nil {
			return st, err
		}
		if cmd.Kind == CmdDelegateCancel {
			if st.Subtree, err = subtree(tx, *st.Task); err != nil {
				return st, err
			}
		}
	}
	if p.RequestID != "" {
		var r Request
		var payload string
		err := tx.QueryRow(`SELECT id, session_id, kind, payload, state, created_at FROM requests WHERE id = ?`, p.RequestID).Scan(&r.ID, &r.Session, &r.Kind, &payload, &r.State, &r.CreatedAt)
		if err == nil {
			r.Payload = json.RawMessage(payload)
			st.Request = &r
		} else if !errors.Is(err, sql.ErrNoRows) {
			return st, err
		}
	}
	if p.TargetID != "" {
		if st.Other, err = e.session(tx, p.TargetID); err != nil {
			return st, err
		}
		if st.Other != nil && st.Session != nil {
			st.Related = st.Other.ParentID == st.Session.ID || st.Session.ParentID == st.Other.ID
			if st.Other.Agent != "" {
				st.Target = agentInfo(st.Other.Agent)
			}
		}
	}
	if err := tx.QueryRow(`SELECT
		(SELECT COUNT(*) FROM delegations WHERE status = 'working') +
		(SELECT COUNT(*) FROM outbox WHERE kind IN (?, ?) AND state IN ('pending', 'running')) +
		(SELECT COUNT(*) FROM sessions WHERE created_by = 'agent' AND relationship = '' AND closed_at = 0 AND status IN ('working', 'needsYou'))`, EffLaunch, EffForkLaunch).Scan(&st.Running); err != nil {
		return st, err
	}
	rows, err := tx.Query(`SELECT task_id FROM held WHERE session_id = ?`, cmd.Session)
	if err != nil {
		return st, err
	}
	defer rows.Close()
	for rows.Next() {
		var id string
		rows.Scan(&id)
		st.Held = append(st.Held, id)
	}
	return st, rows.Err()
}

// session is id's stored row with the registry's live view laid over it.
// A session the store hasn't heard of yet is written from the live view.
func (e *Engine) session(tx *Tx, id string) (*Session, error) {
	s, err := loadSession(tx, id)
	if err != nil || e.Live == nil || id == "" {
		return s, err
	}
	live, ok := e.Live(id)
	if !ok {
		return s, nil
	}
	if s == nil {
		if _, err := tx.Exec(`INSERT INTO sessions (id, agent, access, status, project, cwd, worktree, title, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING`,
			live.ID, live.Agent, live.Access, live.Status, live.Project, live.Cwd, live.Worktree, live.Title, live.CreatedAt); err != nil {
			return nil, err
		}
		return &live, nil
	}
	s.Status, s.Access, s.Agent, s.Project, s.Cwd, s.Worktree, s.Title = live.Status, live.Access, live.Agent, live.Project, live.Cwd, live.Worktree, live.Title
	if live.Conversation != "" {
		s.Conversation = live.Conversation
	}
	return s, nil
}

type querier interface {
	QueryRow(query string, args ...any) *sql.Row
	Query(query string, args ...any) (*sql.Rows, error)
}

const sessionCols = `id, project, worktree, cwd, branch, agent, access, model, effort, status, title, origin, parent_id, relationship, created_by, conversation, terminal, depth, chat, pinned, failed, archived, closed_at, created_at, updated_at`

func scanSession(row interface{ Scan(...any) error }) (Session, error) {
	var s Session
	err := row.Scan(&s.ID, &s.Project, &s.Worktree, &s.Cwd, &s.Branch, &s.Agent, &s.Access, &s.Model, &s.Effort, &s.Status, &s.Title, &s.Origin, &s.ParentID, &s.Relationship,
		&s.CreatedBy, &s.Conversation, &s.Terminal, &s.Depth, &s.Chat, &s.Pinned, &s.Failed, &s.Archived, &s.ClosedAt, &s.CreatedAt, &s.UpdatedAt)
	return s, err
}

func loadSession(q querier, id string) (*Session, error) {
	if id == "" {
		return nil, nil
	}
	s, err := scanSession(q.QueryRow(`SELECT `+sessionCols+` FROM sessions WHERE id = ?`, id))
	if errors.Is(err, sql.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &s, nil
}

func loadQueue(q querier, session string, liveOnly bool) ([]QueueEntry, error) {
	where := `session_id = ?`
	if liveOnly {
		where += ` AND state IN ('pending', 'held', 'sending')`
	}
	rows, err := q.Query(`SELECT id, session_id, text, origin, ord, state, refs, created_at FROM queue WHERE `+where+` ORDER BY ord`, session)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []QueueEntry
	for rows.Next() {
		var e QueueEntry
		var refs string
		if err := rows.Scan(&e.ID, &e.Session, &e.Text, &e.Origin, &e.Ord, &e.State, &refs, &e.CreatedAt); err != nil {
			return nil, err
		}
		json.Unmarshal([]byte(refs), &e.Refs)
		out = append(out, e)
	}
	return out, rows.Err()
}

const delegationCols = `task_id, parent_id, parent_turn, child_id, title, role, agent, brief, isolation, mode, status, work_state, summary, error, access, stepped_down, completion_wake, delivery, pending_question, client_request_id, created_at, settled_at`

func loadDelegations(q querier, where string, args ...any) ([]Delegation, error) {
	rows, err := q.Query(`SELECT `+delegationCols+` FROM delegations WHERE `+where, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Delegation
	for rows.Next() {
		var t Delegation
		if err := rows.Scan(&t.TaskID, &t.ParentID, &t.ParentTurn, &t.ChildID, &t.Title, &t.Role, &t.Agent, &t.Brief, &t.Isolation, &t.Mode, &t.Status, &t.WorkState, &t.Summary,
			&t.Error, &t.Access, &t.SteppedDown, &t.CompletionWake, &t.Delivery, &t.PendingQuestion, &t.ClientRequestID, &t.CreatedAt, &t.SettledAt); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}

// subtree is t and every delegation its child started, deepest first.
func subtree(q querier, t Delegation) ([]Delegation, error) {
	var out []Delegation
	if t.ChildID != "" {
		kids, err := loadDelegations(q, `parent_id = ?`, t.ChildID)
		if err != nil {
			return nil, err
		}
		for _, k := range kids {
			sub, err := subtree(q, k)
			if err != nil {
				return nil, err
			}
			out = append(out, sub...)
		}
	}
	return append(out, t), nil
}

func (s *Store) Session(id string) (*Session, error) { return loadSession(s.db, id) }

// Sessions reads sessions matching where, newest first.
func (s *Store) Sessions(where string, args ...any) ([]Session, error) {
	if where == "" {
		where = "1 = 1"
	}
	rows, err := s.db.Query(`SELECT `+sessionCols+` FROM sessions WHERE `+where, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Session
	for rows.Next() {
		x, err := scanSession(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, x)
	}
	return out, rows.Err()
}

func (s *Store) Delegations(where string, args ...any) ([]Delegation, error) {
	return loadDelegations(s.db, where, args...)
}

// Queue reads session's entries; liveOnly leaves out sent and cancelled ones.
func (s *Store) Queue(session string, liveOnly bool) ([]QueueEntry, error) {
	return loadQueue(s.db, session, liveOnly)
}

// Requests reads session's requests in state.
func (s *Store) Requests(session, state string) ([]Request, error) {
	rows, err := s.db.Query(`SELECT id, session_id, kind, payload, state, created_at FROM requests WHERE session_id = ? AND state = ? ORDER BY created_at`, session, state)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Request
	for rows.Next() {
		var r Request
		var payload string
		if err := rows.Scan(&r.ID, &r.Session, &r.Kind, &payload, &r.State, &r.CreatedAt); err != nil {
			return nil, err
		}
		r.Payload = json.RawMessage(payload)
		out = append(out, r)
	}
	return out, rows.Err()
}

// Transfers reads the transfers session is a side of.
func (s *Store) Transfers(session string) ([]Transfer, error) {
	rows, err := s.db.Query(`SELECT id, source_id, target_id, kind, source_point, state, created_at FROM transfers WHERE source_id = ? OR target_id = ? ORDER BY created_at`, session, session)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.ID, &t.Source, &t.Target, &t.Kind, &t.SourcePoint, &t.State, &t.CreatedAt); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}

type Hit struct {
	Session Session
	Snippet string
	Seq     int64
}

// Search finds sessions whose conversation or title matches query, best first.
func (s *Store) Search(query string, limit int) ([]Hit, error) {
	var terms []string
	for _, t := range strings.Fields(query) {
		terms = append(terms, `"`+strings.ReplaceAll(t, `"`, `""`)+`"`)
	}
	type match struct {
		session, snippet string
		seq              int64
	}
	var found []match
	rows, err := s.db.Query(`SELECT f.session_id, snippet(items_fts, 2, '', '', '…', 16), COALESCE(i.seq, 0) FROM items_fts f
		LEFT JOIN items i ON i.session_id = f.session_id AND i.item_id = f.item_id WHERE items_fts MATCH ? ORDER BY rank LIMIT ?`, strings.Join(terms, " "), limit*4)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var m match
		if err := rows.Scan(&m.session, &m.snippet, &m.seq); err != nil {
			rows.Close()
			return nil, err
		}
		found = append(found, m)
	}
	rows.Close()
	titled, err := s.Sessions(`title LIKE ? ORDER BY updated_at DESC LIMIT ?`, "%"+query+"%", limit)
	if err != nil {
		return nil, err
	}
	var out []Hit
	seen := map[string]bool{}
	for _, t := range titled {
		seen[t.ID] = true
		out = append(out, Hit{Session: t, Snippet: t.Title})
	}
	for _, m := range found {
		if seen[m.session] || len(out) >= limit {
			continue
		}
		seen[m.session] = true
		x, err := s.Session(m.session)
		if err != nil || x == nil {
			continue
		}
		out = append(out, Hit{Session: *x, Snippet: m.snippet, Seq: m.seq})
	}
	if len(out) > limit {
		out = out[:limit]
	}
	return out, nil
}
