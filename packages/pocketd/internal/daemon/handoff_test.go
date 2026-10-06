package daemon

import (
	"testing"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestHandoffDescribesEachTerminalAndItsAgent(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	eventually(t, "the transcript", func() bool { return pr.a.Summary().MaxSeq > 0 })
	pr.a.SetNamed("Fix login")

	terms, err := d.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	defer d.CancelHandoff()
	s := pr.a.Summary()
	if len(terms) != 1 {
		t.Fatalf("handed %d terminals", len(terms))
	}
	got := terms[0]
	if got.Saved.TerminalID != term.Info().ID || got.Saved.AgentID != s.ID || got.Saved.ConversationID != "c1" || got.Pid != term.Pid() ||
		got.Agent == nil || got.Agent.Named != "Fix login" || got.Agent.Epoch != s.Epoch || got.Agent.Seq != s.MaxSeq {
		t.Fatalf("handed = %+v", got)
	}
	f := handoff.File{Format: handoff.Format, Terminals: terms}
	if err := handoff.Check(f); err != nil {
		t.Fatalf("before cancel: %v", err)
	}
	d.CancelHandoff()
	if handoff.Check(f) == nil {
		t.Fatal("the pty still survives exec after CancelHandoff")
	}
}

func TestAHandedOverAgentKeepsItsIDAndShowsNoRestoreNotice(t *testing.T) {
	defer func(w time.Duration) { ClaudeAttachWait = w }(ClaudeAttachWait)
	ClaudeAttachWait = 100 * time.Millisecond
	d := newDaemon(t)
	reported := make(chan string, 1)
	d.OnRestore = func(_ string, _ bool, _ int64, outcome, reason string) { reported <- outcome + " " + reason }
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
		TranscriptPath: transcript(t, "hi"), Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}

	d.expect(saved, term, "", &handoff.Agent{Named: "Fix login", Phase: "working", Epoch: 3, Seq: 12})
	pr := waitPresent(t, d, term)

	select {
	case r := <-reported:
		t.Fatalf("a handoff reported a restore: %q", r)
	case <-time.After(300 * time.Millisecond):
	}
	s := pr.a.Summary()
	if s.ID != "a-1" || s.Restore != "" || !s.Attached || s.Title != "Fix login" || s.Status != "working" || s.Epoch != 4 || s.MaxSeq < 12 {
		t.Fatalf("summary = %+v", s)
	}
	if d.hintOf(pr) != nil {
		t.Fatal("the hint is still waiting")
	}
}
