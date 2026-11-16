package orch

import (
	"encoding/json"
	"log"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/proto"
)

// OnTurn reacts to a session's turn starting or ending: a child that settles
// finalizes its delegation, and a session that goes idle sends what waits for it.
func (e *Engine) OnTurn(c TurnChange) {
	busy := func(s string) bool { return s == "working" || s == "needsYou" }
	if busy(c.To) {
		return
	}
	if c.To == "closed" || busy(c.From) {
		e.settleChild(c)
		// A session an agent started on its own counts against the Mac's limit while busy.
		e.pumpOthers("")
	}
	if busy(c.From) && c.To != "closed" {
		e.dispatch(Command{Kind: CmdParentIdle, Session: c.Session})
	}
	if c.To == "closed" && e.OnClosed != nil {
		e.OnClosed(c.Session)
	}
}

func (e *Engine) settleChild(c TurnChange) {
	ts, err := e.store.Delegations(`child_id = ? AND status IN ('starting', 'working') ORDER BY created_at DESC LIMIT 1`, c.Session)
	if err != nil || len(ts) == 0 {
		return
	}
	t := ts[0]
	p := TaskPayload{TaskID: t.TaskID, Status: TaskCompleted}
	switch {
	case c.To == "closed":
		p.Status, p.Error = TaskCancelled, "The session was closed"
	case c.Stopped:
		p.Status, p.Summary, p.Stopped = TaskCancelled, "Stopped by the user", true
		e.dispatch(NewCommand("", CmdDelegateSettle, t.ParentID, p))
		return
	case c.Failed:
		p.Status, p.Error = TaskFailed, c.Error
	}
	if c.To == "closed" || c.Failed {
		p.Summary = e.store.LatestReply(c.Session)
		e.dispatch(NewCommand("", CmdDelegateSettle, t.ParentID, p))
		return
	}
	// OnTurn runs on the sink's writer, which the reply still has to pass through.
	e.running.Add(1)
	go func() {
		defer e.running.Done()
		e.awaitTurnEnd(c.Session)
		p.Summary = e.store.LatestReply(c.Session)
		e.dispatch(NewCommand("", CmdDelegateSettle, t.ParentID, p))
	}()
}

// catchUp settles a child whose turn ended before its task recorded it: the
// launcher prompts the child before it returns the child's id.
func (e *Engine) catchUp(child string) {
	var state, msg, status string
	err := e.store.db.QueryRow(`SELECT t.state, t.error, s.status FROM turns t JOIN sessions s ON s.id = t.session_id WHERE t.session_id = ? AND t.ended_at > 0 ORDER BY t.id DESC LIMIT 1`, child).Scan(&state, &msg, &status)
	if err != nil || status == "working" || status == "needsYou" {
		return
	}
	e.settleChild(TurnChange{Session: child, From: "working", To: status, Failed: state == "failed", Stopped: state == "interrupted", Error: msg})
}

// FailOrphans fails each working task whose child isn't running: a cold
// restart ended its turn and no restore brought it back, so nothing else will
// settle it. It runs once the restore has listed every agent it brings back.
func (e *Engine) FailOrphans() {
	if e.Live == nil {
		return
	}
	// A host's child runs on there; its mirror settles it once the link is back.
	ts, err := e.store.Delegations(`status = 'working' AND child_id != '' AND environment_id = ''`)
	if err != nil {
		log.Printf("orch: orphans: %v", err)
		return
	}
	for _, t := range ts {
		if _, ok := e.Live(t.ChildID); !ok {
			e.dispatch(NewCommand("", CmdDelegateSettle, t.ParentID, TaskPayload{TaskID: t.TaskID, Status: TaskFailed, Error: "pocketd restarted during the turn"}))
		}
	}
}

// pumpOthers lets every other chat with a queued task use the room a settled
// task left under the per-Mac limit, oldest queue first.
func (e *Engine) pumpOthers(parent string) {
	rows, err := e.store.db.Query(`SELECT parent_id FROM delegations WHERE status = 'queued' AND parent_id != ? GROUP BY parent_id ORDER BY MIN(created_at)`, parent)
	if err != nil {
		return
	}
	var parents []string
	for rows.Next() {
		var p string
		rows.Scan(&p)
		parents = append(parents, p)
	}
	rows.Close()
	for _, p := range parents {
		e.dispatch(Command{Kind: CmdDelegatePump, Session: p})
	}
}

// awaitTurnEnd waits, up to 3 s, for the session's last turn to reach its
// timeline: a turn ends on the agent's stream, and its reply comes through
// the transcript after.
func (e *Engine) awaitTurnEnd(session string) {
	var before int
	e.store.db.QueryRow(`SELECT results_before FROM turns WHERE session_id = ? ORDER BY id DESC LIMIT 1`, session).Scan(&before)
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		var n int
		e.store.db.QueryRow(countResults, session).Scan(&n)
		if n > before {
			return
		}
		select {
		case <-e.ctx.Done():
			return
		case <-time.After(50 * time.Millisecond):
		}
	}
}

// LatestReply is the session's newest assistant text outside sub-agents.
func (s *Store) LatestReply(session string) string {
	var text string
	s.db.QueryRow(`SELECT text FROM items WHERE session_id = ? AND kind = 'assistant' AND parent_tool = '' ORDER BY seq DESC LIMIT 1`, session).Scan(&text)
	return text
}

func (e *Engine) dispatch(cmd Command) {
	if _, err := e.Dispatch(cmd); err != nil {
		log.Printf("orch: %s for %s: %v", cmd.Kind, cmd.Session, err)
	}
}

// AskOpened records a question or permission ask; a child's question reaches its parent.
func (e *Engine) AskOpened(session, requestID, kind string, payload json.RawMessage) {
	e.dispatch(NewCommand("", CmdRequestOpen, session, RequestPayload{RequestID: requestID, Kind: kind, Payload: payload}))
}

// AskClosed records an ask answered or gone, and drops the parent's notice of it.
func (e *Engine) AskClosed(session, requestID string) {
	e.dispatch(NewCommand("", CmdRequestClose, session, RequestPayload{RequestID: requestID}))
}

// Lineage is what the store kept of where session id came from.
func (s *Store) Lineage(id string) agent.Lineage {
	x, err := s.Session(id)
	if err != nil || x == nil {
		return agent.Lineage{}
	}
	l := agent.Lineage{Parent: x.ParentID, Relationship: x.Relationship, CreatedBy: x.CreatedBy, Title: x.Title}
	if ts, err := s.Delegations(`child_id = ? ORDER BY created_at DESC LIMIT 1`, id); err == nil && len(ts) == 1 {
		l.Delegation = &proto.DelegationRef{TaskID: ts[0].TaskID, Status: ts[0].Status, WorkState: ts[0].WorkState}
	}
	return l
}
