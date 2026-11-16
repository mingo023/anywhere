package orch

import (
	"encoding/json"
	"log"
	"time"
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
	case c.Failed:
		p.Status, p.Error = TaskFailed, c.Error
	}
	if c.To == "closed" {
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
