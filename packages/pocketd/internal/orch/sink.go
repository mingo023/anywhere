package orch

import (
	"cmp"
	"database/sql"
	"encoding/json"
	"errors"
	"log"
	"maps"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
)

// Sink takes what the agent registry hears for every session, chat or
// terminal, and writes it to the store in the order heard. The registry calls
// it under an agent's lock, so it only queues; one goroutine writes.
type Sink struct {
	store *Store
	// OnTurn hears each turn that started or ended, after its rows commit.
	OnTurn func(TurnChange)
	// Mirrored says a session id belongs to a host's pocketd; such a session may have finished while pocketd was down.
	Mirrored func(id string) bool

	mu     sync.Mutex
	wake   *sync.Cond
	queue  []sinkOp
	closed bool
	last   map[string]proto.AgentSummary
	// pending is each session's newest summary queued and not yet committed; n numbers them.
	pending map[string]pendingSummary
	n       uint64
}

type pendingSummary struct {
	sum proto.AgentSummary
	n   uint64
}

// TurnChange is a session's turn starting (To = working) or ending.
type TurnChange struct {
	Session  string
	From, To string
	Failed   bool
	Error    string
	Stopped  bool // the turn ended because the user or an agent stopped it
	// Restarted is the turn cut for a message sent in restart mode, whose turn follows.
	Restarted bool
	Turn      int64 // the turn that began or ended
	Summary   proto.AgentSummary
}

type sinkOp struct {
	summary  *proto.AgentSummary
	item     *proto.Item
	released []string
	session  string
	epoch    int64
	n        uint64
	done     chan struct{}
}

func NewSink(s *Store) *Sink {
	k := &Sink{store: s, last: map[string]proto.AgentSummary{}, pending: map[string]pendingSummary{}}
	k.wake = sync.NewCond(&k.mu)
	go k.run()
	return k
}

func (k *Sink) Summary(sum proto.AgentSummary) {
	k.push(sinkOp{summary: &sum, session: sum.ID})
}

// Item queues one timeline item; drafts never reach the store.
func (k *Sink) Item(session string, epoch int64, it proto.Item) {
	k.push(sinkOp{item: &it, session: session, epoch: epoch})
}

// Released drops the held rows of background tasks that ended, so an upgrade doesn't hold them again.
func (k *Sink) Released(session string, tasks []string) {
	k.push(sinkOp{released: tasks, session: session})
}

func (k *Sink) push(op sinkOp) {
	k.mu.Lock()
	if !k.closed {
		if op.summary != nil {
			k.n++
			op.n = k.n
			k.pending[op.session] = pendingSummary{*op.summary, op.n}
		}
		k.queue = append(k.queue, op)
		k.wake.Signal()
	}
	k.mu.Unlock()
}

// Flush returns once everything queued before it is written.
func (k *Sink) Flush() {
	done := make(chan struct{})
	k.mu.Lock()
	if k.closed {
		k.mu.Unlock()
		return
	}
	k.queue = append(k.queue, sinkOp{done: done})
	k.wake.Signal()
	k.mu.Unlock()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		log.Printf("orch: flush timed out")
	}
}

func (k *Sink) Close() {
	k.Flush()
	k.mu.Lock()
	k.closed = true
	k.wake.Signal()
	k.mu.Unlock()
}

func (k *Sink) run() {
	for {
		k.mu.Lock()
		for len(k.queue) == 0 && !k.closed {
			k.wake.Wait()
		}
		if len(k.queue) == 0 && k.closed {
			k.mu.Unlock()
			return
		}
		batch := k.queue
		k.queue = nil
		k.mu.Unlock()
		var changes []TurnChange
		var dones []chan struct{}
		err := k.store.Tx(func(tx *Tx) error {
			for _, op := range batch {
				switch {
				case op.done != nil:
					dones = append(dones, op.done)
				case op.summary != nil:
					c, err := k.putSummary(tx, *op.summary)
					if err != nil {
						return err
					}
					if c != nil {
						changes = append(changes, *c)
					}
				case op.item != nil:
					if err := putItem(tx, op.session, op.epoch, *op.item); err != nil {
						return err
					}
				case op.released != nil:
					for _, id := range op.released {
						if _, err := tx.Exec(`DELETE FROM held WHERE session_id = ? AND task_id = ?`, op.session, id); err != nil {
							return err
						}
					}
				}
			}
			return nil
		})
		if err != nil {
			log.Printf("orch: write: %v", err)
		} else {
			k.written(batch)
		}
		for _, d := range dones {
			close(d)
		}
		if k.OnTurn != nil {
			for _, c := range changes {
				k.OnTurn(c)
			}
		}
	}
}

// written forgets the summaries of batch that committed, except where a newer one for the session is queued.
func (k *Sink) written(batch []sinkOp) {
	k.mu.Lock()
	defer k.mu.Unlock()
	for _, op := range batch {
		if op.summary != nil && k.pending[op.session].n == op.n {
			delete(k.pending, op.session)
		}
	}
}

// OpenSessions is the store's open sessions with the summaries queued and not
// yet written laid over them, so it needs no Flush: a session whose close is
// queued is left out, and a queued status wins. Sessions the store hasn't
// heard of yet are added.
func (k *Sink) OpenSessions() ([]Session, error) {
	// Taken before the read: a summary that commits in between is laid over a row that already holds it.
	pending := k.unwritten()
	stored, err := k.store.OpenSessions()
	if err != nil {
		return nil, err
	}
	return overlay(stored, pending), nil
}

func (k *Sink) unwritten() map[string]pendingSummary {
	k.mu.Lock()
	defer k.mu.Unlock()
	return maps.Clone(k.pending)
}

func overlay(stored []Session, pending map[string]pendingSummary) []Session {
	out := make([]Session, 0, len(stored))
	stays := map[string]bool{}
	for _, x := range stored {
		stays[x.ID] = true
		p, ok := pending[x.ID]
		switch {
		case !ok:
			out = append(out, x)
		case p.sum.Status != "closed":
			out = append(out, laid(x, p.sum))
		}
	}
	added := false
	for id, p := range pending {
		// An id with a slash is a host's session, which the store lists only by its own pocketd's summary.
		if !stays[id] && p.sum.Status != "closed" && !strings.Contains(id, "/") {
			out = append(out, laid(Session{ID: id, ParentID: p.sum.Parent, Relationship: p.sum.Relationship, CreatedBy: p.sum.CreatedBy, CreatedAt: p.sum.CreatedAt}, p.sum))
			added = true
		}
	}
	if added {
		slices.SortFunc(out, func(a, b Session) int { return cmp.Or(cmp.Compare(a.CreatedAt, b.CreatedAt), cmp.Compare(a.ID, b.ID)) })
	}
	return out
}

// laid is x with what putSummary writes from s.
func laid(x Session, s proto.AgentSummary) Session {
	x.Project, x.Worktree, x.Cwd, x.Branch, x.Agent, x.Access, x.Model, x.Effort = s.Project, s.Worktree, s.Cwd, s.Branch, s.Provider, s.Access, s.Model, s.Effort
	x.Conversation, x.Terminal, x.Chat, x.Status, x.Failed, x.Pinned, x.Title, x.Origin, x.UpdatedAt, x.Error = s.ProviderSessionID, s.TerminalID, s.ChatOnly, s.Status, s.Failed, s.Pinned, s.Title, s.Origin, s.UpdatedAt, s.Error
	if s.Recap != (proto.Recap{}) {
		x.Recap = s.Recap
	}
	return x
}

func (k *Sink) putSummary(tx *Tx, s proto.AgentSummary) (*TurnChange, error) {
	before, seen := k.last[s.ID]
	k.last[s.ID] = s
	working := func(st string) bool { return st == "working" || st == "needsYou" }
	from := before.Status
	created := false
	if !seen {
		var updated int64
		err := tx.QueryRow(`SELECT status, updated_at FROM sessions WHERE id = ?`, s.ID).Scan(&from, &updated)
		// Only the sink sets updated_at: a row without it is one a command or a lineage wrote ahead of the session's first summary.
		created = errors.Is(err, sql.ErrNoRows) || err == nil && updated == 0
	}
	// A session stored mid-turn that comes back idle lost its turn to a cold restart, unless its host's agent kept running.
	if !seen && working(from) && !working(s.Status) && s.Status != "closed" && !s.Failed && (k.Mirrored == nil || !k.Mirrored(s.ID)) {
		s.Failed, s.Error = true, "pocketd restarted during the turn"
	}
	now := time.Now().UnixMilli()
	closedAt := int64(0)
	if s.Status == "closed" {
		closedAt = now
	}
	recap := ""
	if s.Recap != (proto.Recap{}) {
		raw, _ := json.Marshal(s.Recap)
		recap = string(raw)
	}
	_, err := tx.Exec(`INSERT INTO sessions (id, project, worktree, cwd, branch, agent, account, access, model, effort, conversation, terminal, chat, status, failed, error, pinned, title, origin, resets, recap, created_at, updated_at, closed_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (id) DO UPDATE SET project = excluded.project, worktree = excluded.worktree, cwd = excluded.cwd, branch = excluded.branch,
			agent = excluded.agent, account = excluded.account, access = excluded.access, model = excluded.model, effort = excluded.effort,
			conversation = excluded.conversation, terminal = excluded.terminal, chat = excluded.chat, status = excluded.status, failed = excluded.failed,
			error = excluded.error, pinned = excluded.pinned, title = excluded.title, origin = excluded.origin, resets = excluded.resets,
			recap = CASE WHEN excluded.recap != '' THEN excluded.recap ELSE sessions.recap END, updated_at = excluded.updated_at, closed_at = CASE WHEN excluded.closed_at > 0 AND sessions.closed_at = 0 THEN excluded.closed_at WHEN excluded.closed_at = 0 THEN 0 ELSE sessions.closed_at END`,
		s.ID, s.Project, s.Worktree, s.Cwd, s.Branch, s.Provider, s.Account, s.Access, s.Model, s.Effort, s.ProviderSessionID, s.TerminalID,
		s.ChatOnly, s.Status, s.Failed, s.Error, s.Pinned, s.Title, s.Origin, s.Resets, recap, s.CreatedAt, s.UpdatedAt, closedAt)
	if err != nil {
		return nil, err
	}
	if seen && s.Resets > before.Resets {
		// The conversation was cleared: what it held is gone for clients, so it goes here too.
		if _, err := tx.Exec(`DELETE FROM items_fts WHERE rowid IN (SELECT rowid FROM items WHERE session_id = ?)`, s.ID); err != nil {
			return nil, err
		}
		if _, err := tx.Exec(`DELETE FROM items WHERE session_id = ?`, s.ID); err != nil {
			return nil, err
		}
		if _, err := tx.Exec(`UPDATE sessions SET recap = '' WHERE id = ?`, s.ID); err != nil {
			return nil, err
		}
	}
	if created {
		if err := event(tx, s.ID, "session.created", map[string]string{"agent": s.Provider, "status": s.Status}); err != nil {
			return nil, err
		}
	}
	if from == s.Status {
		return nil, nil
	}
	kind := "status.changed"
	if s.Status == "closed" {
		kind = "session.closed"
	}
	if from != "" {
		if err := event(tx, s.ID, kind, map[string]string{"from": from, "to": s.Status}); err != nil {
			return nil, err
		}
	}
	stopped, restarted := false, false
	var turn int64
	switch {
	case working(s.Status) && !working(from):
		res, err := tx.Exec(`INSERT INTO turns (session_id, state, started_at, results_before) VALUES (?, 'working', ?, (`+countResults+`))`, s.ID, now, s.ID)
		if err != nil {
			return nil, err
		}
		if turn, err = res.LastInsertId(); err != nil {
			return nil, err
		}
		if err := event(tx, s.ID, "turn.started", nil); err != nil {
			return nil, err
		}
	case working(from) && !working(s.Status):
		state := "completed"
		stop := asked(tx, s.ID, "turn.interrupted")
		restarted = s.Status != "closed" && !stop && asked(tx, s.ID, "turn.restarted")
		switch {
		case s.Status == "closed":
			state = "cancelled"
		case restarted || s.Failed && s.Error == "interrupted" || stop:
			state = "interrupted"
		case s.Failed:
			state = "failed"
		}
		stopped = state == "interrupted" && !restarted
		if err := tx.QueryRow(`SELECT COALESCE(MAX(id), 0) FROM turns WHERE session_id = ?`, s.ID).Scan(&turn); err != nil {
			return nil, err
		}
		if _, err := tx.Exec(`UPDATE turns SET state = ?, ended_at = ?, error = ? WHERE id = (SELECT MAX(id) FROM turns WHERE session_id = ?) AND ended_at = 0`, state, now, s.Error, s.ID); err != nil {
			return nil, err
		}
		if err := event(tx, s.ID, "turn.ended", map[string]string{"state": state}); err != nil {
			return nil, err
		}
	}
	return &TurnChange{Session: s.ID, From: from, To: s.Status, Failed: s.Failed, Error: s.Error, Stopped: stopped, Restarted: restarted, Turn: turn, Summary: s}, nil
}

// asked says whether an event of kind, a stop or a restart, was decided for id since its last turn ended.
func asked(tx *Tx, id, kind string) bool {
	var yes bool
	tx.QueryRow(`SELECT EXISTS (SELECT 1 FROM events WHERE session_id = ? AND kind = ?
		AND seq > COALESCE((SELECT MAX(seq) FROM events WHERE session_id = ? AND kind = 'turn.ended'), 0))`, id, kind, id).Scan(&yes)
	return yes
}

// countResults counts a session's turn ends that reached its timeline.
const countResults = `SELECT count(*) FROM items WHERE session_id = ? AND kind = 'result' AND parent_tool = ''`

func event(tx *Tx, session, kind string, payload any) error {
	b, _ := json.Marshal(payload)
	_, err := tx.Exec(`INSERT INTO events (session_id, kind, payload, at) VALUES (?, ?, ?, ?)`, session, kind, string(b), time.Now().UnixMilli())
	return err
}

func putItem(tx *Tx, session string, epoch int64, it proto.Item) error {
	// JSON, the wire's own form: gob drops a pointer to a zero value, such as a call's durationMs of 0.
	body, err := json.Marshal(it)
	if err != nil {
		return err
	}
	text := itemText(it)
	// Its search row shares its rowid, which only an update in place keeps; a VACUUM could renumber it.
	var row int64
	err = tx.QueryRow(`INSERT INTO items (session_id, item_id, seq, epoch, kind, parent_tool, text, body, ts) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (session_id, item_id) DO UPDATE SET seq = excluded.seq, epoch = excluded.epoch, kind = excluded.kind, text = excluded.text, body = excluded.body, ts = excluded.ts
		RETURNING rowid`,
		session, it.ID, it.Seq, epoch, it.Kind, it.ParentToolUseID, text, body, it.Ts).Scan(&row)
	if err != nil {
		return err
	}
	if _, err := tx.Exec(`INSERT INTO sessions (id, imported) VALUES (?, 1) ON CONFLICT (id) DO UPDATE SET imported = 1 WHERE imported = 0`, session); err != nil {
		return err
	}
	if it.ParentToolUseID != "" || text == "" {
		return nil
	}
	if _, err := tx.Exec(`DELETE FROM items_fts WHERE rowid = ?`, row); err != nil {
		return err
	}
	_, err = tx.Exec(`INSERT INTO items_fts (rowid, session_id, item_id, text) VALUES (?, ?, ?, ?)`, row, session, it.ID, text)
	return err
}

func itemText(it proto.Item) string {
	switch it.Kind {
	case "user", "assistant", "plan":
		return it.Text
	case "result":
		return it.Error
	case "tool":
		if it.Call != nil {
			return strings.TrimSpace(it.Call.Name + " " + it.Call.Detail.Command + it.Call.Detail.Path + it.Call.Detail.Query)
		}
	}
	return ""
}

// History is what a restored session shows from the store, and whether that
// is its whole conversation: it has items, or its transcript was imported
// once already, which the first item written marks.
func (s *Store) History(session string) ([]proto.Item, bool) {
	items, err := s.Items(session)
	if err != nil {
		log.Printf("orch: history of %s: %v", session, err)
		return nil, false
	}
	if len(items) > 0 {
		return items, true
	}
	var imported bool
	s.db.QueryRow(`SELECT imported FROM sessions WHERE id = ?`, session).Scan(&imported)
	return nil, imported
}

// Items reads session's stored timeline, oldest first.
func (s *Store) Items(session string) ([]proto.Item, error) {
	rows, err := s.db.Query(`SELECT body FROM items WHERE session_id = ? ORDER BY seq`, session)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []proto.Item
	for rows.Next() {
		var body []byte
		if err := rows.Scan(&body); err != nil {
			return nil, err
		}
		var it proto.Item
		if err := json.Unmarshal(body, &it); err != nil {
			return nil, err
		}
		out = append(out, it)
	}
	return out, rows.Err()
}
