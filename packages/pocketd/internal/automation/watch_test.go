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
	watch := TurnWatcher(reg, after, "")

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
	watch := TurnWatcher(reg, after, "the run's prompt")
	answer(a, "a message queued first", "Its reply.")
	if seen, _ := watch("a1"); !seen.Before || seen.Summary != "" {
		t.Fatalf("after the message ahead: %+v", seen)
	}
	answer(a, " the run's\nprompt ", "Own reply.")
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("after its own turn: %+v", seen)
	}
}

func TestATurnWatcherStillFindsItsTurnAfterTheTimelineWasCleared(t *testing.T) {
	reg := agent.NewRegistry(hub.New())
	a := reg.Add("a1", "/w", "claude", idleDriver{})
	answer(a, "earlier", "Earlier reply.")
	_, after := a.Timeline.State()
	watch := TurnWatcher(reg, after, "")
	a.ClearTimeline()
	answer(a, "the run's prompt", "Own reply.")
	if seen, _ := watch("a1"); seen.Before || seen.Summary != "Own reply." {
		t.Fatalf("%+v", seen)
	}
}

func TestATurnWatcherReportsAMissingAgentAsAbsent(t *testing.T) {
	if _, ok := TurnWatcher(agent.NewRegistry(hub.New()), 0, "")("nope"); ok {
		t.Fatal("found an agent that is not there")
	}
}
