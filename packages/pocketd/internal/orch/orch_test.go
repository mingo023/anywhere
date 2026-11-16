package orch

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

func openTest(t *testing.T) *Store {
	t.Helper()
	s, err := Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

type fakeEffector struct {
	mu      sync.Mutex
	ran     []Effect
	running map[string]int
	peak    int
	hold    chan struct{}
	result  func(Effect) (map[string]any, error)
}

func (f *fakeEffector) Run(ctx context.Context, e Effect) (map[string]any, error) {
	f.mu.Lock()
	if f.running == nil {
		f.running = map[string]int{}
	}
	f.running[e.Lane]++
	f.peak = max(f.peak, f.running[e.Lane])
	f.ran = append(f.ran, e)
	hold := f.hold
	f.mu.Unlock()
	if hold != nil {
		<-hold
	}
	f.mu.Lock()
	f.running[e.Lane]--
	f.mu.Unlock()
	if f.result != nil {
		return f.result(e)
	}
	return nil, nil
}

func (f *fakeEffector) kinds() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out []string
	for _, e := range f.ran {
		out = append(out, e.Kind+"@"+e.Lane)
	}
	return out
}

func putSession(t *testing.T, s *Store, x Session) {
	t.Helper()
	k := NewSink(s)
	k.Summary(proto.AgentSummary{ID: x.ID, Provider: x.Agent, Access: x.Access, Status: x.Status, Project: x.Project, Cwd: x.Cwd, Title: x.Title})
	k.Close()
	if x.Depth > 0 || x.ParentID != "" {
		if err := s.Tx(func(tx *Tx) error {
			_, err := tx.Exec(`UPDATE sessions SET depth = ?, parent_id = ? WHERE id = ?`, x.Depth, x.ParentID, x.ID)
			return err
		}); err != nil {
			t.Fatal(err)
		}
	}
}

func agents(kind string) AgentInfo {
	switch kind {
	case "claude":
		return AgentInfo{Known: true, Access: []string{"settings", "ask", "edits", "auto", "full"}, CanSteer: true}
	case "codex":
		return AgentInfo{Known: true, Access: []string{"ask", "edits", "full"}, CanSteer: true}
	case "amp":
		return AgentInfo{Known: true, NeverAsks: true}
	case "pi":
		return AgentInfo{Known: true, Access: []string{"full"}}
	}
	return AgentInfo{}
}

func TestMigrationsRunFromEmptyAndAgainWithoutChange(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	s.Close()
	s, err = Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	var n int
	s.db.QueryRow(`SELECT COUNT(*) FROM schema_version`).Scan(&n)
	if n != 1 {
		t.Fatalf("schema_version rows = %d, want 1", n)
	}
	fi, err := os.Stat(filepath.Join(dir, "state", "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %v, want 0600", fi.Mode().Perm())
	}
}

func TestAFailedTransactionLeavesNothing(t *testing.T) {
	s := openTest(t)
	err := s.Tx(func(tx *Tx) error {
		tx.Exec(`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES ('c', 'h', '{}', 1)`)
		return errors.New("crash")
	})
	if err == nil {
		t.Fatal("want the error")
	}
	var n int
	s.db.QueryRow(`SELECT COUNT(*) FROM receipts`).Scan(&n)
	if n != 0 {
		t.Fatalf("receipts = %d after a failed transaction", n)
	}
}

func TestPruneKeepsWhatRetentionKeeps(t *testing.T) {
	s := openTest(t)
	now := time.Now()
	day := int64(24 * time.Hour / time.Millisecond)
	ms := now.UnixMilli()
	err := s.Tx(func(tx *Tx) error {
		for _, q := range []string{
			`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', ` + itoa(ms-2*day) + `)`,
			`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', ` + itoa(ms-2*day+1) + `)`,
			`INSERT INTO events (session_id, kind, payload, at) VALUES ('a', 'x', '{}', ` + itoa(ms) + `)`,
			`INSERT INTO events (session_id, kind, payload, at) VALUES ('b', 'x', '{}', ` + itoa(ms-3*day) + `)`,
			`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES ('old', 'h', '{}', ` + itoa(ms-8*day) + `)`,
			`INSERT INTO receipts (command_id, spec_hash, result, at) VALUES ('new', 'h', '{}', ` + itoa(ms-6*day) + `)`,
			`INSERT INTO sessions (id, closed_at) VALUES ('gone', ` + itoa(ms-16*day) + `)`,
			`INSERT INTO sessions (id, closed_at) VALUES ('kept', ` + itoa(ms-14*day) + `)`,
			`INSERT INTO sessions (id, closed_at) VALUES ('open', 0)`,
			`INSERT INTO items (session_id, item_id, seq, epoch, kind, body, ts) VALUES ('gone', 'i1', 1, 1, 'user', x'00', 1)`,
			`INSERT INTO items (session_id, item_id, seq, epoch, kind, body, ts) VALUES ('kept', 'i1', 1, 1, 'user', x'00', 1)`,
		} {
			if _, err := tx.Exec(q); err != nil {
				return err
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Prune(now); err != nil {
		t.Fatal(err)
	}
	count := func(q string) int {
		var n int
		s.db.QueryRow(q).Scan(&n)
		return n
	}
	if n := count(`SELECT COUNT(*) FROM events WHERE session_id = 'a'`); n != 1 {
		t.Errorf("events of a = %d, want the fresh one", n)
	}
	if n := count(`SELECT COUNT(*) FROM events WHERE session_id = 'b'`); n != 1 {
		t.Errorf("events of b = %d, want its newest kept", n)
	}
	if n := count(`SELECT COUNT(*) FROM receipts`); n != 1 {
		t.Errorf("receipts = %d, want the 6-day-old one", n)
	}
	if n := count(`SELECT COUNT(*) FROM sessions`); n != 2 {
		t.Errorf("sessions = %d, want kept and open", n)
	}
	if n := count(`SELECT COUNT(*) FROM items WHERE session_id = 'gone'`); n != 0 {
		t.Errorf("items of a session closed 16 days ago = %d", n)
	}
	if n := count(`SELECT COUNT(*) FROM items WHERE session_id = 'kept'`); n != 1 {
		t.Errorf("items of a session closed 14 days ago = %d", n)
	}
}

func itoa(n int64) string { return strconv.FormatInt(n, 10) }

func TestASinkWritesSessionsItemsAndTurns(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	var turns []TurnChange
	var mu sync.Mutex
	k.OnTurn = func(c TurnChange) { mu.Lock(); turns = append(turns, c); mu.Unlock() }
	k.Summary(proto.AgentSummary{ID: "a", Provider: "claude", Status: "idle"})
	k.Summary(proto.AgentSummary{ID: "a", Provider: "claude", Status: "working"})
	k.Item("a", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "fix the login redirect"})
	k.Item("a", 1, proto.Item{ID: "i2", Seq: 2, Kind: "assistant", Text: "Fixed it."})
	k.Summary(proto.AgentSummary{ID: "a", Provider: "claude", Status: "done"})
	k.Flush()
	items, err := s.Items("a")
	if err != nil || len(items) != 2 || items[1].Text != "Fixed it." {
		t.Fatalf("items = %+v, %v", items, err)
	}
	if got := s.LatestReply("a"); got != "Fixed it." {
		t.Fatalf("latest reply = %q", got)
	}
	var state string
	s.db.QueryRow(`SELECT state FROM turns WHERE session_id = 'a'`).Scan(&state)
	if state != "completed" {
		t.Fatalf("turn state = %q", state)
	}
	mu.Lock()
	defer mu.Unlock()
	if len(turns) != 3 || turns[2].From != "working" || turns[2].To != "done" {
		t.Fatalf("turn changes = %+v", turns)
	}
	var hits int
	s.db.QueryRow(`SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'redirect'`).Scan(&hits)
	if hits != 1 {
		t.Fatalf("fts hits = %d", hits)
	}
}

func TestAClearedConversationDropsItsItems(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle"})
	k.Item("a", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "old"})
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle", Resets: 1})
	k.Item("a", 2, proto.Item{ID: "i2", Seq: 2, Kind: "user", Text: "new"})
	k.Close()
	items, _ := s.Items("a")
	if len(items) != 1 || items[0].Text != "new" {
		t.Fatalf("items after a clear = %+v", items)
	}
}

func TestADuplicateCommandIDCommitsOnce(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	f := &fakeEffector{}
	e := NewEngine(s, f)
	e.Agent = agents
	cmd := NewCommand("req-1", CmdMessageSend, "a", SendPayload{Text: "hello", Mode: "queue"})
	if _, err := e.Dispatch(cmd); err != nil {
		t.Fatal(err)
	}
	r, err := e.Dispatch(cmd)
	if err != nil || !r.Replay {
		t.Fatalf("replay = %+v, %v", r, err)
	}
	q, _ := s.Queue("a", false)
	if len(q) != 1 {
		t.Fatalf("queue = %d entries, want 1", len(q))
	}
	_, err = e.Dispatch(NewCommand("req-1", CmdMessageSend, "a", SendPayload{Text: "other", Mode: "queue"}))
	if f := AsFailure(err); err == nil || f.Code != CodeInvalid {
		t.Fatalf("same id, other spec: %v", err)
	}
}

func TestAnEffectCommittedBeforeACrashRunsAfterReopen(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	e := NewEngine(s, nil) // no effector: commits, runs nothing, as if killed after commit
	if _, err := e.Dispatch(NewCommand("c1", CmdMessageSend, "a", SendPayload{Text: "hi", Mode: "steer"})); err != nil {
		t.Fatal(err)
	}
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	f := &fakeEffector{}
	e = NewEngine(s, f)
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool { return len(f.kinds()) == 1 })
	if got := f.kinds()[0]; got != "turn.start@a" {
		t.Fatalf("ran %s", got)
	}
}

func TestAPromptThatRanBeforeACrashIsNotSentTwice(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	e := NewEngine(s, nil)
	r, _ := e.Dispatch(NewCommand("c1", CmdMessageSend, "a", SendPayload{Text: "hi", Mode: "steer"}))
	s.db.Exec(`UPDATE outbox SET state = 'running' WHERE id = ?`, r.Effects[0])
	k := NewSink(s)
	k.Item("a", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "hi", Ts: time.Now().UnixMilli() + 1})
	k.Close()
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	f := &fakeEffector{}
	e = NewEngine(s, f)
	e.Recover()
	time.Sleep(50 * time.Millisecond)
	if len(f.kinds()) != 0 {
		t.Fatalf("the prompt ran again: %v", f.kinds())
	}
	var state string
	s.db.QueryRow(`SELECT state FROM outbox WHERE id = ?`, r.Effects[0]).Scan(&state)
	if state != "done" {
		t.Fatalf("state = %s", state)
	}
}

func TestALaneRunsOneEffectAtATime(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	f := &fakeEffector{hold: make(chan struct{})}
	e := NewEngine(s, f)
	for i := range 3 {
		e.Dispatch(NewCommand("c"+itoa(int64(i)), CmdMessageSend, "a", SendPayload{Text: "hi", Mode: "steer"}))
	}
	time.Sleep(30 * time.Millisecond)
	for range 3 {
		f.hold <- struct{}{}
	}
	waitFor(t, func() bool { return len(f.kinds()) == 3 })
	if f.peak != 1 {
		t.Fatalf("peak running in one lane = %d", f.peak)
	}
}

func waitFor(t *testing.T, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(3 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatal("timed out")
}

func TestAccessStepsDownOrRefuses(t *testing.T) {
	for _, c := range []struct {
		parent, want, stepped, code string
		target                      AgentInfo
	}{
		{"auto", "auto", "", "", agents("claude")},
		{"auto", "edits", "edits", "", agents("codex")},
		{"settings", "settings", "", "", agents("codex")},
		{"full", "full", "", "", agents("amp")},
		{"auto", "", "", CodeForbidden, agents("amp")},
		{"edits", "", "", CodeForbidden, agents("pi")},
	} {
		got, stepped, err := Access(c.parent, c.target)
		if c.code != "" {
			if AsFailure(err).Code != c.code || err == nil {
				t.Errorf("%s → %+v: err %v, want %s", c.parent, c.target, err, c.code)
			}
			continue
		}
		if err != nil || got != c.want || stepped != c.stepped {
			t.Errorf("%s → %+v = %q (%q) %v, want %q (%q)", c.parent, c.target, got, stepped, err, c.want, c.stepped)
		}
	}
}

func parentState(status string, depth int) State {
	return State{Now: 1, Limits: DefaultLimits, Session: &Session{ID: "p", Agent: "claude", Access: "auto", Status: status, Depth: depth, Project: "proj", Cwd: "/r"},
		Self: agents("claude"), Target: agents("codex")}
}

func TestDelegationRules(t *testing.T) {
	brief := NewCommand("c", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix it", Title: "Fix"})
	cases := []struct {
		name   string
		st     State
		code   string
		status string
	}{
		{"needs an active turn", parentState("idle", 0), CodeInvalid, ""},
		{"starts when under the limits", parentState("working", 0), "", TaskStarting},
		{"depth 3 is the last level", parentState("working", 2), "", TaskStarting},
		{"depth over 3 is refused", parentState("working", 3), CodeDepthExceeded, ""},
	}
	sixRunning := parentState("working", 0)
	for range 6 {
		sixRunning.Children = append(sixRunning.Children, Delegation{Status: TaskWorking})
	}
	cases = append(cases, struct {
		name   string
		st     State
		code   string
		status string
	}{"the 7th child of a session queues", sixRunning, "", TaskQueued})
	macFull := parentState("working", 0)
	macFull.Running = 12
	cases = append(cases, struct {
		name   string
		st     State
		code   string
		status string
	}{"the 13th on the Mac queues", macFull, "", TaskQueued})
	for _, c := range cases {
		d, err := decide(c.st, brief)
		if c.code != "" {
			if err == nil || AsFailure(err).Code != c.code {
				t.Errorf("%s: err %v, want %s", c.name, err, c.code)
			}
			continue
		}
		if err != nil {
			t.Errorf("%s: %v", c.name, err)
			continue
		}
		got := d.Delegations[0]
		if got.Status != c.status {
			t.Errorf("%s: status %s, want %s", c.name, got.Status, c.status)
		}
		launches := len(d.Effects)
		if (c.status == TaskStarting) != (launches == 1) {
			t.Errorf("%s: %d launches", c.name, launches)
		}
		if got.Access != "edits" || got.SteppedDown != "edits" {
			t.Errorf("%s: access %s stepped %s, want codex's edits", c.name, got.Access, got.SteppedDown)
		}
		if got.Isolation != "worktree" {
			t.Errorf("%s: isolation %s", c.name, got.Isolation)
		}
	}
}

func TestAPublishedResultNeverChanges(t *testing.T) {
	st := parentState("working", 0)
	st.Task = &Delegation{TaskID: "t", ParentID: "p", Status: TaskCompleted, Summary: "first"}
	d, err := decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t", Status: TaskFailed, Summary: "second"}))
	if err != nil || len(d.Delegations) != 0 {
		t.Fatalf("a settled task changed: %+v %v", d, err)
	}
}

func TestSettleWakesAnAsyncParentAndAWaitingOneReadsItThroughTheWait(t *testing.T) {
	st := parentState("working", 0)
	st.Parent = st.Session
	st.Task = &Delegation{TaskID: "t", ParentID: "p", Status: TaskWorking, Mode: "async", CompletionWake: "settled_only", Delivery: DeliverNone}
	d, _ := decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t", Status: TaskCompleted, Summary: "ok"}))
	if d.Delegations[0].Delivery != DeliverPending || d.Follow[0].Kind != CmdWakeDeliver {
		t.Fatalf("async: %+v", d)
	}
	st.Task.Mode = "wait"
	d, _ = decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t", Status: TaskCompleted}))
	if d.Delegations[0].Delivery != DeliverWaiting {
		t.Fatalf("wait: %+v", d.Delegations[0])
	}
	st.Task.Mode, st.Task.CompletionWake = "async", "settled_only"
	d, _ = decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t", Status: TaskCancelled}))
	if d.Delegations[0].Delivery != DeliverNone {
		t.Fatalf("a cancelled task woke a settled_only parent: %+v", d.Delegations[0])
	}
}

func TestSiblingsThatSettleBeforeTheWakeShareIt(t *testing.T) {
	st := parentState("working", 0)
	st.Children = []Delegation{
		{TaskID: "t1", ParentID: "p", Status: TaskCompleted, Delivery: DeliverPending},
		{TaskID: "t2", ParentID: "p", Status: TaskCompleted, Delivery: DeliverPending},
	}
	st.Self = AgentInfo{Known: true} // can't steer: the wake queues
	d, err := decide(st, Command{ID: "w", Kind: CmdWakeDeliver, Session: "p"})
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Queue) != 1 || len(d.Queue[0].Refs) != 2 || !strings.Contains(d.Queue[0].Text, "t1, t2") {
		t.Fatalf("one wake for both: %+v", d.Queue)
	}
	st.Queue = d.Queue
	st.Children = append(st.Children, Delegation{TaskID: "t3", ParentID: "p", Status: TaskFailed, Delivery: DeliverPending})
	st.Children[0].Delivery, st.Children[1].Delivery = DeliverQueued, DeliverQueued
	d, _ = decide(st, Command{ID: "w2", Kind: CmdWakeDeliver, Session: "p"})
	if len(d.Queue) != 1 || len(d.Queue[0].Refs) != 3 || d.Queue[0].ID != st.Queue[0].ID {
		t.Fatalf("a later sibling joins the queued wake: %+v", d.Queue)
	}
}

func TestAWakeSteersIntoAClaudeTurnAndStartsAnIdleParent(t *testing.T) {
	st := parentState("working", 0)
	st.Children = []Delegation{{TaskID: "t1", ParentID: "p", Status: TaskCompleted, Delivery: DeliverPending}}
	d, _ := decide(st, Command{ID: "w", Kind: CmdWakeDeliver, Session: "p"})
	if len(d.Queue) != 0 || !hasEffect(d, EffTurnStart) {
		t.Fatalf("steer: %+v", d)
	}
	st.Session.Status = "idle"
	st.Self = AgentInfo{Known: true}
	d, _ = decide(st, Command{ID: "w", Kind: CmdWakeDeliver, Session: "p"})
	if len(d.Queue) != 0 || !hasEffect(d, EffTurnStart) {
		t.Fatalf("idle parent: %+v", d)
	}
}

func hasEffect(d Decision, kind string) bool {
	for _, e := range d.Effects {
		if e.Kind == kind {
			return true
		}
	}
	return false
}

func TestStopDisposesWakesAndHoldsTheUsersQueue(t *testing.T) {
	st := parentState("working", 0)
	st.Queue = []QueueEntry{
		{ID: "u", Origin: OriginUser, State: EntryPending},
		{ID: "w", Origin: OriginWake, State: EntryPending},
		{ID: "q", Origin: OriginQuestion, State: EntryPending},
	}
	st.Children = []Delegation{{TaskID: "t", Delivery: DeliverQueued}, {TaskID: "t2", Delivery: DeliverDelivered}}
	d, err := decide(st, Command{Kind: CmdSessionStop, Session: "p"})
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]string{}
	for _, e := range d.Queue {
		got[e.ID] = e.State
	}
	if got["u"] != EntryHeld || got["w"] != EntryCancelled || got["q"] != EntryCancelled {
		t.Fatalf("queue after Stop: %v", got)
	}
	if len(d.Delegations) != 1 || d.Delegations[0].Delivery != DeliverDisposed {
		t.Fatalf("delegations after Stop: %+v", d.Delegations)
	}
	st.Queue = d.Queue
	st.Session.Status = "idle"
	d, _ = decide(st, NewCommand("s", CmdMessageSend, "p", SendPayload{Text: "go on"}))
	if len(d.Queue) == 0 || d.Queue[0].ID != "u" || d.Queue[0].State != EntryPending {
		t.Fatalf("the next send releases the held entry: %+v", d.Queue)
	}
}

func TestQueueReorderAndEdit(t *testing.T) {
	st := parentState("working", 0)
	st.Queue = []QueueEntry{{ID: "a", Ord: 1, State: EntryPending}, {ID: "b", Ord: 2, State: EntryPending}, {ID: "c", Ord: 3, State: EntryPending}}
	d, _ := decide(st, NewCommand("", CmdQueueReorder, "p", EntryPayload{EntryID: "c", Before: "a"}))
	if d.Queue[0].Ord >= 1 {
		t.Fatalf("c before a: ord %v", d.Queue[0].Ord)
	}
	d, _ = decide(st, NewCommand("", CmdQueueReorder, "p", EntryPayload{EntryID: "a", Before: "c"}))
	if o := d.Queue[0].Ord; o <= 2 || o >= 3 {
		t.Fatalf("a between b and c: ord %v", o)
	}
	if _, err := decide(st, NewCommand("", CmdQueueEdit, "p", EntryPayload{EntryID: "a", Text: " "})); err == nil {
		t.Fatal("an empty edit was taken")
	}
	st.Queue[0].State = EntrySent
	if _, err := decide(st, NewCommand("", CmdQueueCancel, "p", EntryPayload{EntryID: "a"})); err == nil {
		t.Fatal("a sent entry was cancelled")
	}
}

func TestDrainSendsTheFirstPendingEntryInOrder(t *testing.T) {
	st := parentState("idle", 0)
	st.Queue = []QueueEntry{{ID: "h", Ord: 0, State: EntryHeld}, {ID: "a", Ord: 1, State: EntryPending, Text: "first"}, {ID: "b", Ord: 2, State: EntryPending}}
	d, _ := decide(st, Command{Kind: CmdQueueDrain, Session: "p"})
	if len(d.Queue) != 1 || d.Queue[0].ID != "a" || d.Queue[0].State != EntrySending {
		t.Fatalf("drain: %+v", d.Queue)
	}
	st.Session.Status = "working"
	if d, _ := decide(st, Command{Kind: CmdQueueDrain, Session: "p"}); len(d.Effects) != 0 {
		t.Fatal("drained into a busy session")
	}
}

func TestCancelTakesTheSubtreeDeepestFirst(t *testing.T) {
	st := parentState("working", 0)
	st.Task = &Delegation{TaskID: "t", ParentID: "p", ChildID: "c", Status: TaskWorking}
	st.Subtree = []Delegation{
		{TaskID: "g", ParentID: "c", ChildID: "gc", Status: TaskWorking},
		{TaskID: "done", ParentID: "c", ChildID: "x", Status: TaskCompleted},
		*st.Task,
	}
	d, err := decide(st, NewCommand("", CmdDelegateCancel, "p", TaskPayload{TaskID: "t"}))
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Delegations) != 2 || d.Delegations[0].TaskID != "g" || d.Delegations[1].TaskID != "t" {
		t.Fatalf("cancelled %+v", d.Delegations)
	}
	if len(d.Effects) != 2 || d.Effects[0].Lane != "gc" || d.Effects[1].Lane != "c" {
		t.Fatalf("interrupts %+v", d.Effects)
	}
}

func TestAChildsQuestionQueuesANoticeAndAnAnswerCancelsIt(t *testing.T) {
	child := Session{ID: "c", Agent: "codex", Status: "needsYou"}
	st := State{Now: 1, Session: &child, Task: &Delegation{TaskID: "t", ParentID: "p", ChildID: "c", Status: TaskWorking, Mode: "async", Title: "Fix"}}
	payload := json.RawMessage(`{"questions":[{"question":"Which branch?"}]}`)
	d, err := decide(st, NewCommand("", CmdRequestOpen, "c", RequestPayload{RequestID: "r", Kind: "question", Payload: payload}))
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Follow) != 1 || d.Follow[0].Session != "p" || !strings.Contains(string(d.Follow[0].Payload), `"origin":"question"`) {
		t.Fatalf("notice: %+v", d.Follow)
	}
	if d.Delegations[0].PendingQuestion != "r" {
		t.Fatal("the task doesn't show its question")
	}
	perm, _ := decide(st, NewCommand("", CmdRequestOpen, "c", RequestPayload{RequestID: "r2", Kind: "permission"}))
	if len(perm.Follow) != 0 {
		t.Fatal("a permission ask was routed to the parent")
	}
	st.Task.PendingQuestion = "r"
	st.Request = &Request{ID: "r", Session: "c", Kind: "question", State: "open"}
	d, _ = decide(st, NewCommand("", CmdRequestClose, "c", RequestPayload{RequestID: "r"}))
	if len(d.Follow) != 1 || d.Follow[0].Kind != CmdQueueCancelRefs || d.Follow[0].Session != "p" {
		t.Fatalf("close: %+v", d.Follow)
	}
}

func TestEngineDelegatesSettlesAndWakesTheParent(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	var launched atomic.Int32
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind == EffLaunch {
			launched.Add(1)
			return map[string]any{"childId": "child-1"}, nil
		}
		return nil, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	r, err := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix the redirect", Title: "Fix redirect"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return len(ts) == 1 && ts[0].ChildID == "child-1" && ts[0].Status == TaskWorking
	})
	child, _ := s.Session("child-1")
	if child == nil || child.ParentID != "p" || child.Depth != 1 || child.CreatedBy != "agent" {
		t.Fatalf("lineage: %+v", child)
	}
	k := NewSink(s)
	k.OnTurn = e.OnTurn
	k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "working"})
	k.Item("child-1", 1, proto.Item{ID: "i1", Seq: 1, Kind: "assistant", Text: "Redirect fixed in auth.ts"})
	k.Item("child-1", 1, proto.Item{ID: "i2", Seq: 2, Kind: "result", OK: true})
	k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "done"})
	k.Flush()
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskCompleted && ts[0].Delivery == DeliverDelivered
	})
	ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
	if ts[0].Summary != "Redirect fixed in auth.ts" {
		t.Fatalf("summary %q", ts[0].Summary)
	}
	waitFor(t, func() bool {
		for _, k := range f.kinds() {
			if k == "turn.start@p" {
				return true
			}
		}
		return false
	})
	k.Close()
}

func TestAChildWhoseReplyLandsAfterItsTurnEndsPublishesThatReply(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind == EffLaunch {
			return map[string]any{"childId": "child-1"}, nil
		}
		return nil, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	r, err := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix the redirect", Title: "Fix redirect"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return len(ts) == 1 && ts[0].Status == TaskWorking
	})
	k := NewSink(s)
	defer k.Close()
	k.OnTurn = e.OnTurn
	k.Item("child-1", 1, proto.Item{ID: "i1", Seq: 1, Kind: "assistant", Text: "an earlier turn's reply"})
	k.Item("child-1", 1, proto.Item{ID: "i2", Seq: 2, Kind: "result", OK: true})
	k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "working"})
	k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "done"})
	k.Flush()
	time.Sleep(100 * time.Millisecond) // the transcript lags the stream that ended the turn
	k.Item("child-1", 1, proto.Item{ID: "i3", Seq: 3, Kind: "assistant", Text: "Redirect fixed in auth.ts"})
	k.Item("child-1", 1, proto.Item{ID: "i4", Seq: 4, Kind: "result", OK: true})
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskCompleted
	})
	ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
	if ts[0].Summary != "Redirect fixed in auth.ts" {
		t.Fatalf("summary %q", ts[0].Summary)
	}
}

func TestATaskCancelledWhileItsChildLaunchesClosesTheChild(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	gate := make(chan struct{})
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind == EffLaunch {
			<-gate
			return map[string]any{"childId": "child-1"}, nil
		}
		return nil, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	defer e.Close()
	r, _ := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x"}))
	var task Delegation
	json.Unmarshal(r.Result, &task)
	waitFor(t, func() bool { return slices.Contains(f.kinds(), EffLaunch+"@launch:"+task.TaskID) })
	if _, err := e.Dispatch(NewCommand("c1", CmdDelegateCancel, "p", TaskPayload{TaskID: task.TaskID})); err != nil {
		t.Fatal(err)
	}
	close(gate)
	waitFor(t, func() bool { return slices.Contains(f.kinds(), EffClose+"@child-1") })
	ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
	if ts[0].Status != TaskCancelled {
		t.Fatalf("status %s", ts[0].Status)
	}
}

func TestASessionAnAgentStartsTakesItsAccessAndCountsAgainstTheMacLimit(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "edits", Status: "working", Project: "proj", Cwd: "/r"})
	hold := make(chan struct{})
	f := &fakeEffector{hold: hold}
	e := NewEngine(s, f)
	e.Agent = agents
	e.Limits = func() Limits { return Limits{Depth: 3, PerSession: 6, PerMac: 2} }
	defer e.Close()
	defer close(hold)
	create := func(id, agent string) (Receipt, error) {
		return e.Dispatch(NewCommand(id, CmdSessionCreate, "p", LaunchPayload{Agent: agent, Prompt: "x", Access: "edits"}))
	}
	if _, err := create("c0", "amp"); AsFailure(err).Code != CodeForbidden {
		t.Fatalf("an always-Full agent from an Edits session: %v", err)
	}
	r, err := create("c1", "pi")
	if err == nil {
		t.Fatalf("pi offers only Full, narrower than nothing: %s", r.Result)
	}
	r, err = create("c2", "codex")
	if err != nil || !strings.Contains(string(r.Result), `"access":"edits"`) {
		t.Fatalf("codex: %s, %v", r.Result, err)
	}
	if _, err := create("c3", "claude"); err != nil {
		t.Fatal(err)
	}
	if _, err := create("c4", "claude"); AsFailure(err).Code != CodeConflict {
		t.Fatalf("third launch over a limit of 2: %v", err)
	}
}

func TestADelegationTakesTheCompletionWakeItAsksFor(t *testing.T) {
	st := parentState("working", 0)
	st.Target = agents("codex")
	for wake, want := range map[string]string{"": "settled_only", "always": "always", "sometimes": ""} {
		d, err := decide(st, NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x", CompletionWake: wake}))
		if want == "" {
			if AsFailure(err).Code != CodeInvalid {
				t.Errorf("%q: %v", wake, err)
			}
			continue
		}
		if err != nil || d.Delegations[0].CompletionWake != want {
			t.Errorf("%q: %+v, %v", wake, d.Delegations, err)
		}
	}
}

func TestAForkRecordsItsLineageAndMergeBackGoesOnlyToARelatedSession(t *testing.T) {
	st := parentState("working", 0)
	st.Session.Conversation = "c1"
	st.Target = agents("claude")
	d, err := decide(st, NewCommand("", CmdTransferFork, "p", TransferPayload{Native: true, Prompt: "try the other fix"}))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffForkLaunch {
		t.Fatalf("fork: %+v, %v", d, err)
	}
	var lp LaunchPayload
	json.Unmarshal(d.Effects[0].Payload, &lp)
	if lp.Fork != "p" || lp.Rel != "fork" || lp.Access != "auto" {
		t.Fatalf("fork launch: %+v", lp)
	}
	d, _ = decide(st, NewCommand("", CmdTransferStarted, "p", TransferPayload{TransferID: lp.TransferID, TargetID: "f1"}))
	if d.Lineage[0].ParentID != "p" || d.Lineage[0].Relationship != "fork" {
		t.Fatalf("lineage: %+v", d.Lineage)
	}

	merge := NewCommand("", CmdTransferMerge, "p", TransferPayload{TargetID: "o", Context: "what I found"})
	st.Other = &Session{ID: "o", Project: "proj", Status: "idle"}
	if _, err := decide(st, merge); AsFailure(err).Code != CodeForbidden {
		t.Fatalf("merge into an unrelated session: %v", err)
	}
	st.Related = true
	st.Other.Project = "elsewhere"
	if _, err := decide(st, merge); AsFailure(err).Code != CodeForbidden {
		t.Fatalf("merge into another project: %v", err)
	}
	st.Other.Project = "proj"
	d, err = decide(st, merge)
	if err != nil || d.Follow[0].Kind != CmdMessageSend || d.Follow[0].Session != "o" {
		t.Fatalf("merge: %+v, %v", d, err)
	}
}

func TestOrganizeActions(t *testing.T) {
	st := parentState("idle", 0)
	for action, want := range map[string]string{"pin": EffPin, "unpin": EffPin, "mark_unread": EffMarkUnread, "archive": EffClose} {
		d, err := decide(st, NewCommand("", CmdSessionOrganize, "p", OrganizePayload{Action: action}))
		if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != want {
			t.Errorf("%s: %+v, %v", action, d, err)
		}
	}
	d, _ := decide(st, NewCommand("", CmdSessionOrganize, "p", OrganizePayload{Action: "archive"}))
	if len(d.Archive) != 1 {
		t.Fatalf("archive keeps no history mark: %+v", d)
	}
	for _, action := range []string{"snooze", "settle"} {
		if _, err := decide(st, NewCommand("", CmdSessionOrganize, "p", OrganizePayload{Action: action})); AsFailure(err).Code != CodeUnsupported {
			t.Errorf("%s: %v", action, err)
		}
	}
}

func TestASessionAnAgentLaunchedRecordsWhoStartedIt(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "edits", Status: "working", Project: "proj", Cwd: "/r"})
	f := &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"childId": "k", "targetId": "k"}, nil }}
	e := NewEngine(s, f)
	e.Agent = agents
	defer e.Close()
	if _, err := e.Dispatch(NewCommand("c1", CmdSessionCreate, "p", LaunchPayload{Agent: "claude", Prompt: "x", Access: "edits"})); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool {
		k, _ := s.Session("k")
		return k != nil && k.CreatedBy == "agent"
	})
}

func TestAChildThatEndsBeforeItsTaskKnowsItStillSettlesTheTask(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	k := NewSink(s)
	defer k.Close()
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind != EffLaunch {
			return nil, nil
		}
		k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "working"})
		k.Item("child-1", 1, proto.Item{ID: "i1", Seq: 1, Kind: "assistant", Text: "Done fast"})
		k.Item("child-1", 1, proto.Item{ID: "i2", Seq: 2, Kind: "result", OK: true})
		k.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "done"})
		k.Flush()
		return map[string]any{"childId": "child-1"}, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	k.OnTurn = e.OnTurn
	r, err := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x", Title: "Fast"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return len(ts) == 1 && ts[0].Status == TaskCompleted && ts[0].Summary == "Done fast"
	})
}

func TestATaskQueuedOnTheMacLimitStartsWhenAnotherChatsChildSettles(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "a", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	putSession(t, s, Session{ID: "b", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	n := 0
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind != EffLaunch {
			return nil, nil
		}
		n++
		return map[string]any{"childId": fmt.Sprint("child-", n)}, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	e.Limits = func() Limits { return Limits{Depth: 3, PerSession: 6, PerMac: 1} }
	defer e.Close()
	delegate := func(parent string) Delegation {
		r, err := e.Dispatch(NewCommand("", CmdDelegate, parent, DelegatePayload{Agent: "codex", Brief: "x"}))
		if err != nil {
			t.Fatal(err)
		}
		var d Delegation
		json.Unmarshal(r.Result, &d)
		return d
	}
	status := func(id string) string {
		ts, _ := s.Delegations(`task_id = ?`, id)
		return ts[0].Status
	}
	first := delegate("a")
	waitFor(t, func() bool { return status(first.TaskID) == TaskWorking })
	second := delegate("b")
	if got := status(second.TaskID); got != TaskQueued {
		t.Fatalf("over the Mac limit: %s", got)
	}
	if _, err := e.Dispatch(NewCommand("", CmdDelegateSettle, "a", TaskPayload{TaskID: first.TaskID, Status: TaskCompleted})); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool { return status(second.TaskID) == TaskWorking })
}

func TestAQueuedMessageTheAgentRefusesStaysQueued(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind == EffTurnStart {
			return nil, errors.New("not in the foreground")
		}
		return nil, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	defer e.Close()
	if _, err := e.Dispatch(NewCommand("", CmdMessageSend, "a", SendPayload{Text: "hello", Mode: "queue"})); err != nil {
		t.Fatal(err)
	}
	if _, err := e.Dispatch(Command{Kind: CmdQueueDrain, Session: "a"}); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool {
		f.mu.Lock()
		defer f.mu.Unlock()
		return len(f.ran) > 0
	})
	waitFor(t, func() bool {
		q, _ := s.Queue("a", true)
		return len(q) == 1 && q[0].State == EntryPending
	})
}

func TestAQueuedTaskKeepsTheModelAndEffortItAskedFor(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{})
	e.Agent = agents
	e.Limits = func() Limits { return Limits{Depth: 3, PerSession: 6, PerMac: 0} }
	defer e.Close()
	r, err := e.Dispatch(NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x", Model: "gpt-5.5", Effort: "high"}))
	if err != nil {
		t.Fatal(err)
	}
	var d Delegation
	json.Unmarshal(r.Result, &d)
	ts, _ := s.Delegations(`task_id = ?`, d.TaskID)
	if ts[0].Status != TaskQueued || ts[0].Model != "gpt-5.5" || ts[0].Effort != "high" {
		t.Fatalf("stored: %+v", ts[0])
	}
}

func TestARetriedLaunchFindsTheSessionTheFirstTryStarted(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "edits", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"childId": "k"}, nil }})
	e.Agent = agents
	defer e.Close()
	cmd := NewCommand("req-1", CmdSessionCreate, "p", LaunchPayload{Agent: "claude", Prompt: "x", Access: "edits"})
	first, err := e.DispatchWait(cmd, 3*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	again, err := e.Dispatch(cmd)
	if err != nil || !again.Replay || !slices.Equal(again.Effects, first.Effects) {
		t.Fatalf("replay = %+v, first = %+v, %v", again, first, err)
	}
	if res, state, _ := e.Result(again.Effects[0]); state != "done" || res["childId"] != "k" {
		t.Fatalf("result %v %s", res, state)
	}
}

func TestAForkCountsAgainstTheMacLimit(t *testing.T) {
	st := parentState("working", 0)
	st.Session.Conversation = "c1"
	st.Target = agents("claude")
	st.Running = st.Limits.PerMac
	if _, err := decide(st, NewCommand("", CmdTransferFork, "p", TransferPayload{Native: true, Prompt: "x"})); AsFailure(err).Code != CodeConflict {
		t.Fatalf("a fork over the Mac limit: %v", err)
	}
}

func TestARestoredChildStillShowsTheChatThatDelegatedIt(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"childId": "child-1"}, nil }})
	e.Agent = agents
	defer e.Close()
	r, err := e.Dispatch(NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskWorking
	})
	reg := agent.NewRegistry(hub.New())
	reg.Lineage = s.Lineage
	got := reg.Restore(agent.Restored{ID: "child-1", Provider: "codex"}, nil).Summary()
	if got.Parent != "p" || got.Relationship != "subagent" || got.Delegation == nil || got.Delegation.TaskID != task.TaskID {
		t.Fatalf("restored: parent %q, relationship %q, delegation %+v", got.Parent, got.Relationship, got.Delegation)
	}
}

func TestAnAnswerReachesOnlyTheSessionThatAsked(t *testing.T) {
	st := parentState("working", 0)
	st.Request = &Request{ID: "q1", Session: "other", Kind: "question", State: "open"}
	if _, err := decide(st, NewCommand("", CmdRequestAnswer, "p", RequestPayload{RequestID: "q1", Decision: "allow"})); err == nil || AsFailure(err).Code != CodeNotFound {
		t.Fatalf("an answer to another session's question: %v", err)
	}
	st.Request.Session = "p"
	if d, err := decide(st, NewCommand("", CmdRequestAnswer, "p", RequestPayload{RequestID: "q1", Decision: "allow"})); err != nil || len(d.Effects) != 1 {
		t.Fatalf("its own question: %+v, %v", d, err)
	}
}

func TestEveryWaiterOnOneEffectHearsItFinish(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "edits", Status: "working", Project: "proj", Cwd: "/r"})
	hold := make(chan struct{})
	e := NewEngine(s, &fakeEffector{hold: hold, result: func(Effect) (map[string]any, error) { return nil, errors.New("no room") }})
	e.Agent = agents
	defer e.Close()
	r, err := e.Dispatch(NewCommand("req-1", CmdSessionCreate, "p", LaunchPayload{Agent: "claude", Prompt: "x", Access: "edits"}))
	if err != nil {
		t.Fatal(err)
	}
	got := make(chan error, 2)
	for range 2 {
		go func() { got <- e.Wait(r.Effects, 2*time.Second) }()
	}
	time.Sleep(50 * time.Millisecond)
	close(hold)
	for range 2 {
		if err := <-got; err == nil || err.Error() != "no room" {
			t.Fatalf("a waiter heard %v", err)
		}
	}
}

func TestATaskQueuedOnTheMacLimitStartsWhenAnotherChatsTaskIsCancelled(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "a", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	putSession(t, s, Session{ID: "b", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	n := 0
	e := NewEngine(s, &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind != EffLaunch {
			return nil, nil
		}
		n++
		return map[string]any{"childId": fmt.Sprint("child-", n)}, nil
	}})
	e.Agent = agents
	e.Limits = func() Limits { return Limits{Depth: 3, PerSession: 6, PerMac: 1} }
	defer e.Close()
	delegate := func(parent string) string {
		r, err := e.Dispatch(NewCommand("", CmdDelegate, parent, DelegatePayload{Agent: "codex", Brief: "x"}))
		if err != nil {
			t.Fatal(err)
		}
		var d Delegation
		json.Unmarshal(r.Result, &d)
		return d.TaskID
	}
	status := func(id string) string {
		ts, _ := s.Delegations(`task_id = ?`, id)
		return ts[0].Status
	}
	first := delegate("a")
	waitFor(t, func() bool { return status(first) == TaskWorking })
	second := delegate("b")
	if _, err := e.Dispatch(NewCommand("", CmdDelegateCancel, "a", TaskPayload{TaskID: first})); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool { return status(second) == TaskWorking })
}

func TestCancellingATaskDropsItsQuestionNoticeFromTheParentsQueue(t *testing.T) {
	st := parentState("working", 0)
	task := Delegation{TaskID: "t1", ParentID: "p", ChildID: "c", Status: TaskWorking, PendingQuestion: "q1"}
	st.Task, st.Subtree = &task, []Delegation{task}
	d, err := decide(st, NewCommand("", CmdDelegateCancel, "p", TaskPayload{TaskID: "t1"}))
	if err != nil {
		t.Fatal(err)
	}
	dropped := slices.ContainsFunc(d.Follow, func(c Command) bool {
		return c.Kind == CmdQueueCancelRefs && c.Session == "p" && strings.Contains(string(c.Payload), "q1")
	})
	if !dropped {
		t.Fatalf("follow-ups: %+v", d.Follow)
	}
}

func TestALaunchThatFinishedBeforeACrashStillStartsItsTask(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, nil)
	e.Agent = agents
	r, err := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	// The launch ran and was recorded, and pocketd died before the task heard of it.
	s.db.Exec(`UPDATE outbox SET state = 'done', result = '{"childId":"child-1"}' WHERE id = ?`, r.Effects[0])
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	e = NewEngine(s, &fakeEffector{})
	e.Agent = agents
	defer e.Close()
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
	if ts[0].Status != TaskWorking || ts[0].ChildID != "child-1" {
		t.Fatalf("after recovery: %+v", ts[0])
	}
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	var n int
	s.db.QueryRow(`SELECT COUNT(*) FROM events WHERE kind = 'delegation.started'`).Scan(&n)
	if n != 1 {
		t.Fatalf("started %d times", n)
	}
}

func TestAQueuedMessageSentBeforeACrashCountsAsSentAfterIt(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	putSession(t, s, Session{ID: "a", Agent: "claude", Status: "idle"})
	e := NewEngine(s, nil)
	e.Dispatch(NewCommand("", CmdMessageSend, "a", SendPayload{Text: "hi", Mode: "queue"}))
	if _, err := e.Dispatch(Command{Kind: CmdQueueDrain, Session: "a"}); err != nil {
		t.Fatal(err)
	}
	s.db.Exec(`UPDATE outbox SET state = 'running'`)
	k := NewSink(s)
	k.Item("a", 1, proto.Item{ID: "i1", Seq: 1, Kind: "user", Text: "hi", Ts: time.Now().UnixMilli() + 1})
	k.Close()
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	e = NewEngine(s, &fakeEffector{})
	defer e.Close()
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	q, _ := s.Queue("a", false)
	if len(q) != 1 || q[0].State != EntrySent {
		t.Fatalf("queue after recovery: %+v", q)
	}
}

func TestAChildCutOffByAColdRestartFailsItsTask(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"childId": "child-1"}, nil }})
	e.Agent = agents
	defer e.Close()
	r, err := e.Dispatch(NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	before := NewSink(s)
	before.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "working"})
	before.Close()
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskWorking
	})
	after := NewSink(s)
	defer after.Close()
	after.OnTurn = e.OnTurn
	after.Summary(proto.AgentSummary{ID: "child-1", Provider: "codex", Status: "idle"})
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskFailed && strings.Contains(ts[0].Error, "restarted")
	})
}

func TestPruneDropsTheLaunchOfASessionRetentionDropped(t *testing.T) {
	s := openTest(t)
	now := time.Now()
	old := itoa(now.Add(-16 * 24 * time.Hour).UnixMilli())
	for _, q := range []string{
		`INSERT INTO sessions (id, closed_at) VALUES ('gone', ` + old + `)`,
		`INSERT INTO sessions (id, closed_at) VALUES ('open', 0)`,
		`INSERT INTO outbox (session_id, kind, payload, state, result, created_at) VALUES ('launch:t1', '` + EffLaunch + `', '{"parent":"open","prompt":"the brief"}', 'done', '{"childId":"gone"}', 1)`,
		`INSERT INTO outbox (session_id, kind, payload, state, result, created_at) VALUES ('launch:t2', '` + EffLaunch + `', '{"parent":"open","prompt":"kept"}', 'done', '{"childId":"open"}', 1)`,
		`INSERT INTO outbox (session_id, kind, payload, state, created_at) VALUES ('launch:t3', '` + EffLaunch + `', '{"parent":"open","prompt":"queued"}', 'pending', 1)`,
	} {
		if _, err := s.db.Exec(q); err != nil {
			t.Fatal(err)
		}
	}
	if err := s.Prune(now); err != nil {
		t.Fatal(err)
	}
	var left string
	s.db.QueryRow(`SELECT group_concat(session_id) FROM outbox`).Scan(&left)
	if left != "launch:t2,launch:t3" {
		t.Fatalf("launches left: %s", left)
	}
}
