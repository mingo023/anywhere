package automation

import (
	"testing"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type idleDriver struct{}

func (idleDriver) Prompt(string) error                { return nil }
func (idleDriver) Interrupt() error                   { return nil }
func (idleDriver) Compact() error                     { return nil }
func (idleDriver) Settings(string, string) error      { return nil }
func (idleDriver) Commands() ([]proto.Command, error) { return nil, nil }
func (idleDriver) Access(string) error                { return nil }
func (idleDriver) Close()                             {}

func answer(a *agent.Agent, prompt, reply string) {
	a.Record(timeline.Event{Kind: "user", Text: prompt})
	a.Working()
	a.Record(timeline.Event{Kind: "assistant_text", Text: reply})
	a.Record(timeline.Event{Kind: "result", OK: true})
	a.TurnEnded(false)
}

func TestATurnWatcherIgnoresTheSessionsEarlierTurnsAndSeesItsOwn(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	answer(a, "earlier", "Earlier reply.")
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "the run's prompt", "", nil)

	if seen, ok := watch("a1"); !ok || !seen.Before || seen.Status != "done" || seen.Summary != "" || seen.Answered {
		t.Fatalf("before the prompt went in: %+v %v", seen, ok)
	}

	a.Record(timeline.Event{Kind: "user", Text: "the run's prompt"})
	a.Working()
	if seen, _ := watch("a1"); !seen.Before || seen.Status != "working" {
		t.Fatalf("while the turn starts: %+v", seen)
	}

	a.Record(timeline.Event{Kind: "assistant_text", Text: "Own reply."})
	a.Record(timeline.Event{Kind: "result", OK: true})
	a.TurnEnded(false)
	seen, _ := watch("a1")
	if seen.Before || seen.Status != "done" || seen.Summary != "Own reply." || !seen.Answered {
		t.Fatalf("after its own turn: %+v", seen)
	}
}

func TestATurnWatcherForAQueuedPromptIsNotEndedByAMessageQueuedAheadOfIt(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "the run's prompt", "q1", entryAs("the run's prompt", "pending"))
	answer(a, "a message queued first", "Its reply.")
	if seen, _ := watch("a1"); !seen.Before || seen.Summary != "" {
		t.Fatalf("after the message ahead: %+v", seen)
	}
	answer(a, " the run's\nprompt ", "Own reply.")
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("after its own turn: %+v", seen)
	}
}

// entryAs is an Entry that finds every entry with text, in state.
func entryAs(text, state string) Entry {
	return func(string, string) (string, string) { return text, state }
}

func TestATurnWatcherFollowsTheEditedTextOfItsQueuedPrompt(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "the run's prompt", "q1", entryAs("the edited prompt", "pending"))
	answer(a, "the edited prompt", "Own reply.")
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("%+v", seen)
	}
}

func TestATurnWatcherWhoseQueuedPromptWasCancelledEndsTheRun(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	a.Working()
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "the run's prompt", "q1", entryAs("the run's prompt", "cancelled"))
	if seen, _ := watch("a1"); seen.Cancel == "" {
		t.Fatalf("%+v", seen)
	}
}

// crashed is a session restored from a store that lost every item after seq 3, as a crash can.
func crashed() *agent.Registry {
	reg := agent.NewRegistry(hub.New())
	reg.History = func(string) ([]proto.Item, bool) {
		return []proto.Item{{ID: "i3", Seq: 3, Kind: "user", Text: "earlier"}}, true
	}
	reg.Restore(agent.Restored{ID: "a1", Cwd: "/w", Provider: "claude"}, idleDriver{})
	return reg
}

func TestATurnWatcherAfterACrashLostItsTurnEndsTheRunInsteadOfFollowingALaterTurn(t *testing.T) {
	reg := crashed()
	watch := TurnWatcher(reg, 7, "the run's prompt", "", nil)
	if seen, _ := watch("a1"); !seen.Before || seen.Fail != "" {
		t.Fatalf("before any turn: %+v", seen)
	}
	a, _ := reg.Get("a1")
	answer(a, "a later prompt", "Later reply.")
	if seen, _ := watch("a1"); seen.Fail == "" || seen.Summary != "" {
		t.Fatalf("after a later turn: %+v", seen)
	}
}

func TestATurnWatcherAfterACrashFindsItsTurnWhenItComesAfterAll(t *testing.T) {
	for _, entry := range []string{"", "q1"} {
		reg := crashed()
		watch := TurnWatcher(reg, 7, "the run's prompt", entry, entryAs("the run's prompt", "pending"))
		watch("a1")
		a, _ := reg.Get("a1")
		if entry != "" {
			answer(a, "a message queued first", "Its reply.")
		}
		answer(a, "the run's prompt", "Own reply.")
		if seen, _ := watch("a1"); seen.Before || seen.Fail != "" || seen.Summary != "Own reply." {
			t.Fatalf("entry %q: %+v", entry, seen)
		}
	}
}

func TestATurnWatcherWhoseQueuedPromptWasSentOrGoneBeforeACrashLostItsTurnFailsTheRun(t *testing.T) {
	for _, state := range []string{"sent", ""} {
		watch := TurnWatcher(crashed(), 7, "the run's prompt", "q1", entryAs("the run's prompt", state))
		seen, _ := watch("a1")
		if run, _ := Step(proto.Run{Status: "running"}, seen, true, true); run.Status != "failed" || run.Why != "Its turn was lost in a crash" {
			t.Fatalf("entry %q: %+v from %+v", state, run, seen)
		}
	}
}

func TestATurnWatcherFailsTheRunWhoseTurnARestartCutButNotOnePostedSince(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	answer(a, "earlier", "Earlier reply.")
	_, after := a.Timeline.State()
	a.Record(timeline.Event{Kind: "user", Text: "the run's prompt"})
	a.SetRestore(proto.RestoreInterrupted)
	seen, _ := TurnWatcher(reg, after, "the run's prompt", "", nil)("a1")

	if run, _ := Step(proto.Run{Status: "running"}, seen, true, true); run.Status != "failed" {
		t.Fatalf("left going by the restart: %+v from %+v", run, seen)
	}
	if run, _ := Step(proto.Run{Status: "running"}, seen, true, false); run.Status != "running" {
		t.Fatalf("posted since: %+v from %+v", run, seen)
	}
}

func TestATurnWatcherWhoseQueuedPromptWentOutAsItBeganFollowsTheTurnItStarted(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	_, after := a.Timeline.State()
	a.Working()
	watch := TurnWatcher(reg, after, "the run's prompt", "q1", entryAs("the run's prompt", "sent"))
	if seen, _ := watch("a1"); !seen.Before || seen.Fail != "" {
		t.Fatalf("as its turn starts: %+v", seen)
	}
	a.Record(timeline.Event{Kind: "user", Text: "the run's prompt"})
	a.Record(timeline.Event{Kind: "assistant_text", Text: "Own reply."})
	a.TurnEnded(false)
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("after its turn: %+v", seen)
	}
}

func TestATurnWatcherStillFindsItsTurnAfterTheTimelineWasCleared(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	answer(a, "earlier", "Earlier reply.")
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "the run's prompt", "", nil)
	a.ClearTimeline()
	answer(a, "the run's prompt", "Own reply.")
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("%+v", seen)
	}
}

func TestATurnWatcherReportsAMissingAgentAsAbsent(t *testing.T) {
	if _, ok := TurnWatcher(agent.NewRegistry(hub.New()), 0, "p", "", nil)("nope"); ok {
		t.Fatal("found an agent that is not there")
	}
}
