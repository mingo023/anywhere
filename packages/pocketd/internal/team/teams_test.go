package team

import (
	"context"
	"encoding/json"
	"errors"
	"slices"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// chat is a chat agent's driver that keeps the prompts pocketd sends it.
type chat struct {
	mu      sync.Mutex
	prompts []string
	err     error
}

func (c *chat) Prompt(text string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.prompts = append(c.prompts, text)
	return c.err
}
func (c *chat) Interrupt() error                   { return nil }
func (c *chat) Compact() error                     { return nil }
func (c *chat) Settings(string, string) error      { return nil }
func (c *chat) Access(string) error                { return nil }
func (c *chat) Commands() ([]proto.Command, error) { return nil, nil }
func (c *chat) Close()                             {}

func (c *chat) sent() []string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return slices.Clone(c.prompts)
}

type fixture struct {
	teams *Teams
	lead  *agent.Agent
	chat  *chat
	msgs  <-chan []byte
}

func newFixture(t *testing.T) fixture {
	h := hub.New()
	msgs, stop := h.Subscribe()
	t.Cleanup(stop)
	reg := agent.NewRegistry(h)
	c := &chat{}
	lead := reg.Add("3f9a0c1b-2d4e-4f60-8a7b-9c0d1e2f3a4b", "/w", "claude", c)
	return fixture{teams: &Teams{Store: open(t, t.TempDir()), Agents: reg, Hub: h, Poll: time.Millisecond}, lead: lead, chat: c, msgs: msgs}
}

// turn records one turn of the lead in its timeline.
func (f fixture) turn(prompt, reply string, ok bool) {
	f.lead.Record(timeline.Event{Kind: "user", Text: prompt})
	f.lead.Record(timeline.Event{Kind: "assistant_text", Text: reply})
	f.lead.Record(timeline.Event{Kind: "result", OK: ok})
}

// answer is the lead's turn on the last prompt pocketd sent it.
func (f fixture) answer(reply string, ok bool) {
	sent := f.chat.sent()
	f.turn(sent[len(sent)-1], reply, ok)
}

func (f fixture) team(t *testing.T) proto.Team {
	t.Helper()
	team, ok, err := f.teams.Store.Get(f.lead.ID())
	if err != nil || !ok {
		t.Fatalf("no team: %v", err)
	}
	return team
}

// published is the state of each team.update sent so far.
func (f fixture) published() []string {
	var states []string
	for {
		select {
		case raw := <-f.msgs:
			var m struct {
				Type string
				Team proto.Team
			}
			json.Unmarshal(raw, &m)
			if m.Type == "team.update" {
				states = append(states, m.Team.State)
			}
		default:
			return states
		}
	}
}

// await waits for the next team.update in state.
func (f fixture) await(t *testing.T, state string) proto.Team {
	t.Helper()
	timeout := time.After(5 * time.Second)
	for {
		select {
		case raw := <-f.msgs:
			var m struct {
				Type string
				Team proto.Team
			}
			json.Unmarshal(raw, &m)
			if m.Type == "team.update" && m.Team.State == state {
				return m.Team
			}
		case <-timeout:
			t.Fatalf("no team.update %s", state)
		}
	}
}

func TestATeamPromptSendsTheRulesAndProposesThePlanTheLeadAnswers(t *testing.T) {
	f := newFixture(t)
	if err := f.teams.Prompt(f.lead, "Add truncate"); err != nil {
		t.Fatal(err)
	}
	if got := f.chat.sent(); !slices.Equal(got, []string{Wrap("Add truncate")}) {
		t.Fatalf("sent %q", got)
	}
	f.answer("Two tasks, the export after the function.\n"+twoTasks, true)

	f.teams.Ended(f.lead.ID())

	got := f.team(t)
	if got.State != "proposed" || got.Repaired || !slices.Equal(ids(got.Tasks), []string{"t1", "t2"}) || got.UpdatedAt == 0 {
		t.Fatalf("got %+v", got)
	}
	if states := f.published(); !slices.Equal(states, []string{"planning", "proposed"}) {
		t.Fatalf("published %v", states)
	}
	if list, err := f.teams.List(); err != nil || len(list) != 1 || list[0].Lead != f.lead.ID() {
		t.Fatalf("list %+v %v", list, err)
	}
}

func TestAnUnusablePlanGetsOneRepairPromptThenTheTeamFails(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer("I'd split it in two.", true)

	f.teams.Ended(f.lead.ID())

	repair := "Your plan block can't be used: no <anywhere_team_plan> block. Send the corrected <anywhere_team_plan> block only."
	if sent := f.chat.sent(); len(sent) != 2 || sent[1] != repair {
		t.Fatalf("sent %q", sent)
	}
	if got := f.team(t); got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v", got)
	}
	f.answer("Still thinking.", true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 2 {
		t.Fatalf("a second repair: %q", sent)
	}
	if states := f.published(); !slices.Equal(states, []string{"planning", "planning", "failed"}) {
		t.Fatalf("published %v", states)
	}
}

func TestAPlanSentAfterTheRepairIsProposed(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer("I'd split it in two.", true)
	f.teams.Ended(f.lead.ID())
	f.answer(twoTasks, true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "proposed" || !got.Repaired || len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedPlanningTurnFailsTheTeam(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, false)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "The lead's turn failed" {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 1 {
		t.Fatalf("sent %q", sent)
	}
}

func TestATurnEndThatReachesTheTimelineLateIsWaitedFor(t *testing.T) {
	f := newFixture(t)
	f.teams.Poll = 50 * time.Millisecond
	f.teams.Prompt(f.lead, "Add truncate")
	f.lead.Record(timeline.Event{Kind: "user", Text: f.chat.sent()[0]})
	f.lead.Record(timeline.Event{Kind: "assistant_text", Text: twoTasks})
	go func() {
		time.Sleep(10 * time.Millisecond)
		f.lead.Record(timeline.Event{Kind: "result", OK: false})
	}()

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "The lead's turn failed" {
		t.Fatalf("got %+v", got)
	}
}

func TestATurnBeforeTheTeamPromptIsNotReadAsItsAnswer(t *testing.T) {
	f := newFixture(t)
	f.turn("an earlier prompt", twoTasks, true)
	f.teams.Prompt(f.lead, "Add truncate")

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v", got)
	}
}

func TestALeadsLaterTurnsLeaveItsProposedTeamAsItIs(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())
	f.turn("thanks", "You're welcome.", true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "proposed" || len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 1 {
		t.Fatalf("sent %q", sent)
	}
}

func TestAnAgentWithoutATeamIsLeftAlone(t *testing.T) {
	f := newFixture(t)
	f.turn("hi", twoTasks, true)

	f.teams.Ended(f.lead.ID())

	if list, err := f.teams.List(); err != nil || len(list) != 0 {
		t.Fatalf("list %+v %v", list, err)
	}
	if sent := f.chat.sent(); len(sent) != 0 {
		t.Fatalf("sent %q", sent)
	}
}

func TestATeamPromptThatCantReachTheLeadFailsTheTeam(t *testing.T) {
	f := newFixture(t)
	f.chat.err = errors.New("This session has ended")

	err := f.teams.Prompt(f.lead, "Add truncate")

	if err == nil || err.Error() != "This session has ended" {
		t.Fatalf("got %v", err)
	}
	if got := f.team(t); got.State != "failed" || got.Error != "This session has ended" {
		t.Fatalf("got %+v", got)
	}
}

func TestATeamPromptIsRefusedWhileTheLeadsTeamRunsAndReplacesAnyOtherTeam(t *testing.T) {
	for _, state := range []string{"running", "review", "merging"} {
		f := newFixture(t)
		f.teams.Store.Put(proto.Team{Lead: f.lead.ID(), State: state, Tasks: []proto.TeamTask{}})

		err := f.teams.Prompt(f.lead, "Add truncate")

		if !errors.Is(err, ErrBusy) {
			t.Errorf("%s: got %v", state, err)
		}
		if got := f.team(t); got.State != state {
			t.Errorf("%s: the team became %s", state, got.State)
		}
		if sent := f.chat.sent(); len(sent) != 0 {
			t.Errorf("%s: sent %q", state, sent)
		}
	}
	for _, state := range []string{"planning", "proposed", "merged", "conflict", "failed"} {
		f := newFixture(t)
		f.teams.Store.Put(proto.Team{Lead: f.lead.ID(), State: state, Error: "an old one", Tasks: []proto.TeamTask{}})

		if err := f.teams.Prompt(f.lead, "Add truncate"); err != nil {
			t.Errorf("%s: %v", state, err)
		}
		if got := f.team(t); got.State != "planning" || got.Error != "" {
			t.Errorf("%s: got %+v", state, got)
		}
	}
}

func TestALeadThatClosesWhilePlanningFailsItsTeam(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")

	f.teams.handle(event{kind: "status", agent: f.lead.ID(), from: "working", to: "closed"})

	if got := f.team(t); got.State != "failed" || got.Error != "the lead closed while planning" {
		t.Fatalf("got %+v", got)
	}
}

func TestALeadThatClosesLeavesATeamPastPlanningAlone(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())

	f.teams.handle(event{kind: "status", agent: f.lead.ID(), from: "idle", to: "closed"})

	if got := f.team(t); got.State != "proposed" {
		t.Fatalf("got %+v", got)
	}
}

func TestRunHandsATurnEndHeardFromTheStatusToItsTeam(t *testing.T) {
	f := newFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go f.teams.Run(ctx)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)

	f.teams.Status(f.lead.ID(), "working", "done")

	if got := f.await(t, "proposed"); len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
}

func TestWithoutTeamsTheVerbsAreRefused(t *testing.T) {
	var teams *Teams
	if err := teams.Prompt(nil, "Add truncate"); err == nil || err.Error() != "Teams are off" {
		t.Fatalf("prompt: %v", err)
	}
	if _, err := teams.List(); err == nil || err.Error() != "Teams are off" {
		t.Fatalf("list: %v", err)
	}
	teams.Status("a1", "working", "done")
	teams.Run(context.Background())
}

// proposedFixture is a fixture whose lead's plan waits for Start.
func proposedFixture(t *testing.T) fixture {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())
	return f
}

func TestALeadNeedsYouWhileItsPlanWaitsUntilItsNextTurn(t *testing.T) {
	f := proposedFixture(t)
	if s := f.lead.Summary().Status; s != "needsYou" {
		t.Fatalf("proposed: %s", s)
	}
	f.teams.Prompt(f.lead, "Split it differently")
	if s := f.lead.Summary().Status; s != "needsYou" {
		t.Fatalf("a re-plan before its turn starts: %s", s)
	}
	f.lead.Working()
	if s := f.lead.Summary().Status; s != "working" {
		t.Fatalf("the re-plan's turn: %s", s)
	}
}

func TestALeadStopsNeedingYouWhenItsTeamLeavesProposed(t *testing.T) {
	f := proposedFixture(t)
	f.teams.change(f.lead.ID(), func(tm proto.Team) (proto.Team, bool) {
		tm.State = "running"
		return tm, true
	})
	if s := f.lead.Summary().Status; s != "idle" {
		t.Fatalf("started: %s", s)
	}

	g := proposedFixture(t)
	g.chat.err = errors.New("This session has ended")
	g.teams.Prompt(g.lead, "Split it differently")
	if s := g.lead.Summary().Status; s != "idle" {
		t.Fatalf("a re-plan that couldn't be sent: %s", s)
	}
}
