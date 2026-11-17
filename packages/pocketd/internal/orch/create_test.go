package orch

import (
	"testing"
	"time"

	"pocketd/internal/proto"
)

func start(id, model string) Command {
	return NewCommand(id, CmdSessionStart, "", StartPayload{Owner: true, Key: "owner", Spec: proto.LaunchSpec{Provider: "claude", Model: model}})
}

func TestAClientCreateRunsOnceOnItsOwnLaneAndARetryGetsTheFirstResult(t *testing.T) {
	s := openTest(t)
	f := &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"AgentID": "a1"}, nil }}
	e := NewEngine(s, f)
	defer e.Close()
	first, err := e.Dispatch(start("create:owner:r1", "opus"))
	if err != nil || len(first.Effects) != 1 {
		t.Fatalf("create = %+v, %v", first, err)
	}
	if err := e.Wait(first.Effects, 2*time.Second); err != nil {
		t.Fatal(err)
	}
	again, err := e.Dispatch(start("create:owner:r1", "opus"))
	if err != nil || !again.Replay || len(again.Effects) != 1 || again.Effects[0] != first.Effects[0] {
		t.Fatalf("retry = %+v, %v, want the first create's effect", again, err)
	}
	if res, state, _ := e.Result(again.Effects[0]); state != "done" || res["AgentID"] != "a1" {
		t.Fatalf("retry's result = %v %s", res, state)
	}
	if got := f.kinds(); len(got) != 1 || got[0] != EffCreate+"@create:owner:r1" {
		t.Fatalf("ran %v, want one create on its own lane", got)
	}
}

func TestARequestIdUsedWithAnotherSpecIsRefusedButAnotherRequestersIsItsOwn(t *testing.T) {
	s := openTest(t)
	e := NewEngine(s, nil)
	if _, err := e.Dispatch(start("create:owner:r1", "opus")); err != nil {
		t.Fatal(err)
	}
	if _, err := e.Dispatch(start("create:owner:r1", "sonnet")); err == nil || AsFailure(err).Code != CodeInvalid {
		t.Fatalf("another spec: %v, want invalid_request", err)
	}
	if r, err := e.Dispatch(start("create:device:d1:r1", "sonnet")); err != nil || r.Replay {
		t.Fatalf("another requester's r1 = %+v, %v, want its own create", r, err)
	}
}

func TestAClientCreateDoesNotCountAgainstTheAgentsMacLimit(t *testing.T) {
	s := openTest(t)
	e := NewEngine(s, nil)
	e.Limits = func() Limits { return Limits{Depth: 1, PerSession: 1, PerMac: 1} }
	e.Agent = agents
	if _, err := e.Dispatch(start("create:owner:r1", "opus")); err != nil {
		t.Fatal(err)
	}
	if _, err := e.Dispatch(NewCommand("c1", CmdSessionCreate, "", LaunchPayload{Agent: "codex"})); err != nil {
		t.Fatalf("an agent's session after a client create: %v", err)
	}
}

func TestAClientCreatesReceiptAndResultLastAWeek(t *testing.T) {
	s := openTest(t)
	now := time.Now()
	e := NewEngine(s, &fakeEffector{})
	e.Now = func() time.Time { return now }
	defer e.Close()
	r, _ := e.Dispatch(start("create:owner:r1", "opus"))
	e.Wait(r.Effects, 2*time.Second)
	held, _ := NewEngine(s, nil).Dispatch(NewCommand("create:owner:r2", CmdSessionStart, "", StartPayload{Key: "owner"}))
	s.db.Exec(`UPDATE outbox SET state = 'running' WHERE id = ?`, held.Effects[0])
	s.Prune(now.Add(ReceiptsKept - time.Hour))
	if again, _ := e.Dispatch(start("create:owner:r1", "opus")); !again.Replay {
		t.Fatal("a create's receipt went before a week")
	}
	s.Prune(now.Add(ReceiptsKept + time.Hour))
	if _, state, _ := e.Result(r.Effects[0]); state != "" {
		t.Fatalf("a week-old create's result is still %s", state)
	}
	if _, state, _ := e.Result(held.Effects[0]); state != "running" {
		t.Fatalf("a running create's row was pruned: %q", state)
	}
	if again, _ := e.Dispatch(start("create:owner:r1", "opus")); again.Replay {
		t.Fatal("a create's receipt outlived a week")
	}
}

func TestACreateCutByACrashIsRetiredAndARetryHearsSo(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	e := NewEngine(s, nil)
	r, _ := e.Dispatch(start("create:owner:r1", "opus"))
	s.db.Exec(`UPDATE outbox SET state = 'running' WHERE id = ?`, r.Effects[0])
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	f := &fakeEffector{}
	e = NewEngine(s, f)
	defer e.Close()
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	again, _ := e.Dispatch(start("create:owner:r1", "opus"))
	if err := e.Wait(again.Effects, time.Second); err == nil {
		t.Fatal("a retry of a create a crash cut waited for nothing and heard no failure")
	}
	if _, state, last := e.Result(again.Effects[0]); state != "retired" || last == "" {
		t.Fatalf("state %s %q, want retired", state, last)
	}
	if len(f.kinds()) != 0 {
		t.Fatalf("ran %v after the crash", f.kinds())
	}
}

func TestARetryWaitsForTheCreateARestartLeftPending(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(dir)
	e := NewEngine(s, nil)
	e.Dispatch(start("create:owner:r1", "opus"))
	s.Close()
	s, _ = Open(dir)
	defer s.Close()
	f := &fakeEffector{hold: make(chan struct{})}
	e = NewEngine(s, f)
	defer e.Close()
	if err := e.Recover(); err != nil {
		t.Fatal(err)
	}
	again, _ := e.Dispatch(start("create:owner:r1", "opus"))
	done := make(chan error, 1)
	go func() { done <- e.Wait(again.Effects, 5*time.Second) }()
	select {
	case err := <-done:
		t.Fatalf("the retry ended before the create ran: %v", err)
	case <-time.After(200 * time.Millisecond):
	}
	close(f.hold)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	if _, state, _ := e.Result(again.Effects[0]); state != "done" {
		t.Fatalf("state %s", state)
	}
}
