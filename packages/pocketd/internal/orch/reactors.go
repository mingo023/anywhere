package orch

import (
	"encoding/json"
	"fmt"
	"log"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/proto"
)

// OnTurn reacts to a session's turn starting or ending: a child prompted after it settled opens a follow-up, a child
// that settles finalizes its delegation, and a session that goes idle sends what waits for it.
func (e *Engine) OnTurn(c TurnChange) {
	busy := func(s string) bool { return s == "working" || s == "needsYou" }
	if busy(c.To) {
		// A compaction the user starts from idle isn't a turn of the task.
		if c.From != "" && !busy(c.From) && !c.Summary.Compacting {
			e.followUp(c.Session, c.Turn)
		}
		return
	}
	if c.To == "closed" {
		e.dropChildren(c.Session)
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

// dropChildren cancels a closed session's unsettled tasks and closes the sessions it delegated to, as nothing is
// left to read them. A stopping or starting pocketd skips it: its sessions end with it and come back on restore.
func (e *Engine) dropChildren(parent string) {
	e.mu.Lock()
	stopped := e.stopped
	e.mu.Unlock()
	if stopped {
		return
	}
	ts, err := e.store.Delegations(`parent_id = ? AND status IN ('queued', 'starting', 'working')`, parent)
	if err != nil {
		log.Printf("orch: tasks of %s: %v", parent, err)
	}
	for _, t := range ts {
		e.dispatch(NewCommand("", CmdDelegateCancel, parent, TaskPayload{TaskID: t.TaskID}))
	}
	kids, err := e.store.Sessions(`parent_id = ? AND relationship = 'subagent' AND closed_at = 0`, parent)
	if err != nil {
		log.Printf("orch: children of %s: %v", parent, err)
	}
	for _, k := range kids {
		e.dispatch(Command{Kind: CmdSessionClose, Session: k.ID})
	}
}

// prompted is the text of the prompt pocketd sent a session that started the turn that began; a turn the agent starts
// on its own, as on a background task's notice, has none. A wake counts: it is how a child hears that its own children
// finished, and its turn may give the report it held back for them. A prompt sent to an idle agent waits, running, for
// its turn and then says it started one; pocketd may send it before the sink records the turn before as ended. Without
// that word, as from a host, only a prompt sent after that end and not turnless counts.
const prompted = `SELECT json_extract(payload, '$.text') FROM outbox WHERE session_id = ?1 AND kind = '` + EffTurnStart + `'
	AND json_extract(payload, '$.origin') IN ('` + OriginUser + `', '` + OriginWake + `')
	AND created_at <= (SELECT started_at FROM turns WHERE id = ?2)
	AND CASE
		WHEN state = 'running' THEN 1
		WHEN state != 'done' THEN 0
		WHEN json_extract(result, '$.started') THEN created_at > COALESCE((SELECT MAX(started_at) FROM turns WHERE session_id = ?1 AND id < ?2), 0)
		ELSE COALESCE(json_extract(result, '$.turnless'), 0) = 0
			AND created_at >= COALESCE((SELECT MAX(ended_at) FROM turns WHERE session_id = ?1 AND id < ?2), 0)
	END
	ORDER BY id DESC LIMIT 1`

func (e *Engine) followUp(child string, turn int64) {
	var brief string
	if e.store.db.QueryRow(prompted, child, turn).Scan(&brief) != nil {
		return
	}
	ts, err := e.store.Delegations(`child_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1`, child)
	if err != nil || len(ts) == 0 || !ts[0].Terminal() {
		return
	}
	e.dispatch(NewCommand(fmt.Sprintf("follow_%s_%d", child, turn), CmdDelegateFollow, ts[0].ParentID, TaskPayload{TaskID: ts[0].TaskID, Brief: brief}))
}

func (e *Engine) settleChild(c TurnChange) {
	if c.Restarted {
		return
	}
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
		p.Summary, p.Turn = e.store.LatestReply(c.Session), c.Turn
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

// FailHost fails each unsettled task on host, which was removed, so nothing will settle them; name is how the
// task's error names it.
func (e *Engine) FailHost(host, name string) {
	ts, err := e.store.Delegations(`status IN ('queued', 'starting', 'working') AND environment_id = ?`, host)
	if err != nil {
		log.Printf("orch: tasks on %s: %v", host, err)
		return
	}
	for _, t := range ts {
		e.dispatch(NewCommand("", CmdDelegateSettle, t.ParentID, TaskPayload{TaskID: t.TaskID, Status: TaskFailed, Error: "The host " + name + " was removed"}))
	}
	e.pumpOthers("")
}

// Relink sends again each of host's sessions' stop that failed, as one does while the link is down: a task cancelled
// then would run on there. Only a session's last effect is sent again, so a stop older than a prompt stays undone.
func (e *Engine) Relink(host string) {
	_, err := e.store.db.Exec(`UPDATE outbox SET state = 'pending' WHERE state = 'failed' AND kind IN (?, ?)
		AND id IN (SELECT MAX(id) FROM outbox WHERE session_id LIKE ? GROUP BY session_id)`, EffInterrupt, EffClose, host+"/%")
	if err != nil {
		log.Printf("orch: stops on %s: %v", host, err)
		return
	}
	e.KickAll()
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

// turnEndWait bounds how long a settle waits for the turn's reply to reach the timeline.
const turnEndWait = 3 * time.Second

// awaitTurnEnd waits, up to turnEndWait, for the session's last turn to reach its
// timeline: a turn ends on the agent's stream, and its reply comes through
// the transcript after.
func (e *Engine) awaitTurnEnd(session string) {
	var before int
	e.store.db.QueryRow(`SELECT results_before FROM turns WHERE session_id = ? ORDER BY id DESC LIMIT 1`, session).Scan(&before)
	deadline := time.Now().Add(turnEndWait)
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

// LatestReply is the newest assistant text outside sub-agents of the session's latest turn: after the
// result that ended the turn before it.
func (s *Store) LatestReply(session string) string {
	var before int
	s.db.QueryRow(`SELECT results_before FROM turns WHERE session_id = ? ORDER BY id DESC LIMIT 1`, session).Scan(&before)
	var after int64
	if before > 0 {
		s.db.QueryRow(`SELECT seq FROM items WHERE session_id = ? AND kind = 'result' AND parent_tool = '' ORDER BY seq LIMIT 1 OFFSET ?`, session, before-1).Scan(&after)
	}
	var text string
	s.db.QueryRow(`SELECT text FROM items WHERE session_id = ? AND kind = 'assistant' AND parent_tool = '' AND seq > ? ORDER BY seq DESC LIMIT 1`, session, after).Scan(&text)
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
	if ts, err := s.Delegations(`child_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1`, id); err == nil && len(ts) == 1 {
		l.Delegation = &proto.DelegationRef{TaskID: ts[0].TaskID, Status: ts[0].Status, WorkState: ts[0].WorkState}
	}
	return l
}
