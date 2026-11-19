//go:build unix

package daemon

import (
	"os"
	"testing"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

func TestCarryDescribesEachTerminalAndItsAgent(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	background(t, path, "t1")
	hookFrom(d, pr, sessionStart("c1", path))
	eventually(t, "the server", func() bool { return callOf(pr, "t1").Status == "running" })
	claim, _ := pr.a.ClaimNaming()
	pr.a.SetNamed(claim, "Fix login")

	terms := d.Carry()
	defer d.Release()
	s := pr.a.Summary()
	if len(terms) != 1 {
		t.Fatalf("handed %d terminals", len(terms))
	}
	got := terms[0]
	if got.Saved.TerminalID != term.Info().ID || got.Saved.AgentID != s.ID || got.Saved.ConversationID != "c1" || got.FD != 0 ||
		got.Agent == nil || got.Agent.Named != "Fix login" || got.Agent.Epoch != s.Epoch || got.Agent.Seq != s.MaxSeq {
		t.Fatalf("handed = %+v", got)
	}
	if bg := got.Agent.Background; bg.Tasks["bt1"] != "t1" || len(bg.Lasting) != 1 || bg.Lasting[0] != "t1" {
		t.Fatalf("handed background = %+v", bg)
	}
}

func TestAHandedOverTerminalKeepsItsLocal(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Local: "l1"})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	terms := d.Carry()
	defer d.Release()
	if len(terms) != 1 || terms[0].Saved.Local != "l1" {
		t.Fatalf("handed = %+v", terms)
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

func TestAHandedOverAgentKeepsItsBackgroundCallsRunning(t *testing.T) {
	path := projectTranscript(t)
	background(t, path, "t1")
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin", "CLAUDE_CONFIG_DIR=" + os.Getenv("CLAUDE_CONFIG_DIR")}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
		TranscriptPath: path, Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}

	d.expect(saved, term, "", &handoff.Agent{Phase: "working"})
	pr := waitPresent(t, d, term)

	eventually(t, "the server", func() bool { return callOf(pr, "t1").Status == "running" })
	time.Sleep(100 * time.Millisecond)
	if s := callOf(pr, "t1").Status; s != "running" {
		t.Fatalf("the server is %s", s)
	}
}

func TestAHandedOverAgentTakesTheBackgroundAgentItCarriedOverItsStoredTimeline(t *testing.T) {
	d := newDaemon(t)
	d.Agents.History = func(string) ([]proto.Item, bool) {
		return []proto.Item{{ID: "i1", Seq: 1, Kind: "tool", Call: &proto.ToolCall{ToolUseID: "t1", Name: "Agent", Detail: proto.ToolDetail{Kind: "task"}, Status: "running"}}}, true
	}
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
		TranscriptPath: transcript(t, "hi"), Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}

	d.expect(saved, term, "", &handoff.Agent{Phase: "working", Seq: 1, Background: timeline.Background{Tasks: map[string]string{"a1": "t1"}, Lasting: []string{"t1"}}})

	a, err := d.Agents.Get("a-1")
	if err != nil || a.Timeline.BackgroundAgents() != 1 || !a.Timeline.InBackground("a1") {
		t.Fatalf("the carried sub-agent isn't running in the background: %v", err)
	}
}

func TestAHandedOverAgentTheUserStartedStaysTheirs(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
		TranscriptPath: transcript(t, "hi"), Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working", Typed: true}

	d.expect(saved, term, "", &handoff.Agent{Phase: "working"})
	pr := waitPresent(t, d, term)

	if !pr.typed {
		t.Fatal("the handoff forgot the user started the agent")
	}
}
