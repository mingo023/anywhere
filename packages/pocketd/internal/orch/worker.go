package orch

import (
	"database/sql"
	"encoding/json"
	"errors"
	"log"
	"time"
)

type lane struct{ running, dirty bool }

// kick starts lane's runner, or tells a running one to look again.
func (e *Engine) kick(name string) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.stopped || e.effector == nil {
		return
	}
	l := e.lanes[name]
	if l == nil {
		l = &lane{}
		e.lanes[name] = l
	}
	if l.running {
		l.dirty = true
		return
	}
	l.running = true
	e.running.Add(1)
	go e.runLane(name)
}

func (e *Engine) runLane(name string) {
	defer e.running.Done()
	for {
		x, ok := e.claim(name)
		if !ok {
			return
		}
		res, err := e.effector.Run(e.ctx, x)
		e.finish(x, res, err)
	}
}

// claim marks the lane's oldest pending row running. It ends the runner when
// there is none, or when the engine is draining. It holds e.mu only between
// queries, as a command's transaction takes e.mu while it holds the store.
func (e *Engine) claim(name string) (Effect, bool) {
	for {
		e.mu.Lock()
		stopped := e.stopped
		e.mu.Unlock()
		if !stopped {
			x, found, err := e.pending(name)
			if err != nil {
				log.Printf("orch: claim %s: %v", name, err)
			}
			if found {
				return x, true
			}
		}
		e.mu.Lock()
		l := e.lanes[name]
		if l.dirty && !e.stopped {
			l.dirty = false
			e.mu.Unlock()
			continue
		}
		l.running, l.dirty = false, false
		e.mu.Unlock()
		return Effect{}, false
	}
}

func (e *Engine) pending(name string) (Effect, bool, error) {
	var x Effect
	var payload string
	err := e.store.db.QueryRow(`SELECT id, kind, payload, command_id, then_cmd, else_cmd FROM outbox WHERE session_id = ? AND state = 'pending' ORDER BY id LIMIT 1`, name).
		Scan(&x.ID, &x.Kind, &payload, &x.Command, &x.Then, &x.Else)
	if errors.Is(err, sql.ErrNoRows) {
		return x, false, nil
	}
	if err != nil {
		return x, false, err
	}
	x.Lane, x.Payload = name, json.RawMessage(payload)
	if _, err := e.store.db.Exec(`UPDATE outbox SET state = 'running', attempts = attempts + 1 WHERE id = ?`, x.ID); err != nil {
		return x, false, err
	}
	return x, true, nil
}

func (e *Engine) finish(x Effect, res map[string]any, err error) {
	state, last := "done", ""
	if err != nil {
		state, last = "failed", err.Error()
		log.Printf("orch: %s on %s: %v", x.Kind, x.Lane, err)
	}
	out, _ := json.Marshal(res)
	if _, dberr := e.store.db.Exec(`UPDATE outbox SET state = ?, last_error = ?, result = ? WHERE id = ?`, state, last, string(out), x.ID); dberr != nil {
		log.Printf("orch: finish %d: %v", x.ID, dberr)
	}
	e.mu.Lock()
	ch := e.waits[x.ID]
	delete(e.waits, x.ID)
	e.mu.Unlock()
	if ch != nil {
		ch <- err
	}
	next := x.Then
	if err != nil {
		next = x.Else
	}
	if next == "" {
		return
	}
	payload := map[string]any{}
	json.Unmarshal(x.Payload, &payload)
	for k, v := range res {
		payload[k] = v
	}
	if err != nil {
		payload["error"] = err.Error()
	}
	session := x.Lane
	if p, ok := payload["parent"].(string); ok && p != "" {
		session = p
	}
	if _, err := e.Dispatch(NewCommand("", next, session, payload)); err != nil {
		log.Printf("orch: %s after %s: %v", next, x.Kind, err)
	}
}

// Recover makes the outbox safe to run after a crash or an upgrade: a row
// left running is done if its echo reached the store, else pending again,
// except a launch, whose half-made session can't be judged, which is retired
// and fails its task. Then every lane with work starts.
func (e *Engine) Recover() error {
	rows, err := e.store.db.Query(`SELECT id, session_id, kind, payload, created_at, else_cmd FROM outbox WHERE state = 'running'`)
	if err != nil {
		return err
	}
	type left struct {
		id                  int64
		lane, kind, payload string
		created             int64
		elseCmd             string
	}
	var all []left
	for rows.Next() {
		var l left
		if err := rows.Scan(&l.id, &l.lane, &l.kind, &l.payload, &l.created, &l.elseCmd); err != nil {
			rows.Close()
			return err
		}
		all = append(all, l)
	}
	rows.Close()
	for _, l := range all {
		state := "pending"
		switch l.kind {
		case EffTurnStart:
			var p TurnPayload
			json.Unmarshal([]byte(l.payload), &p)
			var n int
			e.store.db.QueryRow(`SELECT COUNT(*) FROM items WHERE session_id = ? AND kind = 'user' AND text = ? AND ts >= ?`, l.lane, p.Text, l.created).Scan(&n)
			if n > 0 {
				state = "done"
			}
		case EffLaunch, EffForkLaunch:
			state = "retired"
		}
		if _, err := e.store.db.Exec(`UPDATE outbox SET state = ?, last_error = CASE WHEN ? = 'retired' THEN 'pocketd restarted while it ran' ELSE last_error END WHERE id = ?`, state, state, l.id); err != nil {
			return err
		}
		if state == "retired" && l.elseCmd != "" {
			payload := map[string]any{}
			json.Unmarshal([]byte(l.payload), &payload)
			payload["error"] = "pocketd restarted while the session was starting"
			session, _ := payload["parent"].(string)
			if session == "" {
				session = l.lane
			}
			if _, err := e.Dispatch(NewCommand("", l.elseCmd, session, payload)); err != nil {
				log.Printf("orch: recover %d: %v", l.id, err)
			}
		}
	}
	return e.KickAll()
}

func (e *Engine) KickAll() error {
	rows, err := e.store.db.Query(`SELECT DISTINCT session_id FROM outbox WHERE state = 'pending'`)
	if err != nil {
		return err
	}
	var lanes []string
	for rows.Next() {
		var l string
		rows.Scan(&l)
		lanes = append(lanes, l)
	}
	rows.Close()
	for _, l := range lanes {
		e.kick(l)
	}
	return nil
}

// Drain stops claiming and waits up to wait for running effects, before an
// in-place upgrade hands over.
func (e *Engine) Drain(wait time.Duration) {
	e.mu.Lock()
	e.stopped = true
	e.mu.Unlock()
	done := make(chan struct{})
	go func() { e.running.Wait(); close(done) }()
	select {
	case <-done:
	case <-time.After(wait):
		log.Printf("orch: handing over with effects still running")
	}
}

func (e *Engine) Close() {
	e.Drain(5 * time.Second)
	e.cancel()
}

// Result is a finished effect's result and state: pending, running, done, failed or retired.
func (e *Engine) Result(id int64) (map[string]any, string, string) {
	var state, last, raw string
	if err := e.store.db.QueryRow(`SELECT state, last_error, result FROM outbox WHERE id = ?`, id).Scan(&state, &last, &raw); err != nil {
		return nil, "", ""
	}
	var res map[string]any
	json.Unmarshal([]byte(raw), &res)
	return res, state, last
}

// Resume claims again after a Drain whose handover didn't happen.
func (e *Engine) Resume() {
	e.mu.Lock()
	e.stopped = false
	e.mu.Unlock()
	e.KickAll()
}
