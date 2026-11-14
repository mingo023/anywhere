package factory

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/board"
	"pocketd/internal/hub"
	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/team"
	"pocketd/internal/timeline"
)

const leadID = "3f9a0c1b-0000-4000-8000-000000000001"

const teamPlan = `<anywhere_team_plan>
{"tasks":[{"id":"t1","title":"Write a","agent":"claude","files":["a.txt"],"after":[],"allow":[],"instructions":"write a.txt"},{"id":"t2","title":"Write b","agent":"claude","files":["b.txt"],"after":["t1"],"allow":[],"instructions":"write b.txt"}]}
</anywhere_team_plan>`

// chat is the fake chat driver of team/teams_test.go.
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

type world struct {
	t     *testing.T
	home  string
	e     *Engine
	teams *team.Teams
	reg   *agent.Registry
	chat  *chat
	key   string
	spec  proto.LaunchSpec
}

func newWorld(t *testing.T) *world {
	t.Helper()
	home := t.TempDir()
	h := hub.New()
	store, err := team.OpenStore(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	w := &world{t: t, home: home, reg: agent.NewRegistry(h), chat: &chat{}}
	w.teams = &team.Teams{Store: store, Agents: w.reg, Hub: h, Poll: time.Millisecond}
	w.e = &Engine{Home: home, Board: board.Open(home), Configs: OpenConfigs(home), Teams: w.teams, Agents: w.reg, Now: time.Now, Poll: time.Millisecond}
	w.e.Launch = func(key, requestID string, s proto.LaunchSpec, created func(string)) launch.Result {
		w.key, w.spec = key, s
		created("/w/" + requestID)
		w.reg.Add(leadID, "/w/"+requestID, s.Provider, w.chat)
		return launch.Result{AgentID: leadID, TerminalID: "term-1", Cwd: "/w/" + requestID}
	}
	w.teams.OnChange = w.e.TeamChanged
	return w
}

// ticket is a claimed Ticket of a project whose Factory is on.
func (w *world) ticket(spec bool) proto.Ticket {
	w.t.Helper()
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled, cfg.Spec = true, spec
	if err := w.e.Configs.Save("/repo", cfg); err != nil {
		w.t.Fatal(err)
	}
	x, err := w.e.Board.Save(proto.Ticket{Title: "Queue sends", Prompt: "Keep sends in a queue", Provider: "claude", Folder: "/repo"}, true, time.Now())
	if err != nil {
		w.t.Fatal(err)
	}
	w.e.Board.Claim(time.Now(), nil)
	return w.got(x.ID)
}

func (w *world) got(id string) proto.Ticket {
	w.t.Helper()
	x, ok := w.e.ticket(id)
	if !ok {
		w.t.Fatalf("no ticket %s", id)
	}
	return x
}

func (w *world) settle() { w.e.drain(context.Background()) }

func (w *world) lead() *agent.Agent {
	w.t.Helper()
	a, err := w.reg.Get(leadID)
	if err != nil {
		w.t.Fatal(err)
	}
	return a
}

// turn is the lead's whole turn on the last prompt it was sent.
func (w *world) turn(reply string, ok bool) {
	sent := w.chat.sent()
	lead := w.lead()
	lead.Record(timeline.Event{Kind: "user", Text: sent[len(sent)-1]})
	lead.Record(timeline.Event{Kind: "assistant_text", Text: reply})
	lead.Record(timeline.Event{Kind: "result", OK: ok})
}

// started is a Ticket whose lead was launched and asked for its first prompt.
func (w *world) started(spec bool) proto.Ticket {
	w.t.Helper()
	x := w.ticket(spec)
	if res := w.e.Start("req12345678", x, func(string) {}); res.Err != nil {
		w.t.Fatal(res.Err.Message)
	}
	w.settle()
	return w.got(x.ID)
}

// gated is a Ticket whose lead's plan is proposed.
func (w *world) gated() proto.Ticket {
	w.t.Helper()
	x := w.started(true)
	w.turn(goodSpec, true)
	w.e.TurnEnded(leadID, true)
	w.settle()
	w.turn(teamPlan, true)
	w.teams.Ended(leadID)
	w.settle()
	return w.got(x.ID)
}

func TestATicketGoesFromSpecToThePlanGateAndOnToImplementWhenItsPlanStarts(t *testing.T) {
	w := newWorld(t)
	x := w.started(true)
	if s := w.spec; w.key != "factory:"+x.ID || !s.Chat || s.Access != "edits" || s.Provider != "claude" || s.Prompt != "" || s.Checkout.New == nil || s.Checkout.New.Name != "ticket-req12345" {
		t.Fatalf("launched %q with %+v", w.key, s)
	}
	if x.Station != proto.StationSpec || x.Status != proto.TicketWorking || x.AgentID != leadID || x.TerminalID != "term-1" || x.Worktree != "/w/req12345678" {
		t.Fatalf("after the start: %+v", x)
	}
	if sent := w.chat.sent(); len(sent) != 1 || !strings.HasPrefix(sent[0], Rules+"\n\n<anywhere_factory_spec_request>") || !strings.HasSuffix(sent[0], "Ticket: Queue sends\n\nKeep sends in a queue") {
		t.Fatalf("the lead was sent %q", sent)
	}

	w.turn(goodSpec, true)
	w.e.TurnEnded(leadID, true)
	w.settle()
	saved, _ := os.ReadFile(filepath.Join(w.home, "state", "factory", x.ID, "spec.md"))
	if g := w.got(x.ID); g.Station != proto.StationPlan || g.Status != proto.TicketWorking || !strings.Contains(string(saved), "- [ ] A send survives a restart") {
		t.Fatalf("after the spec: %+v, saved %q", g, saved)
	}
	if sent := w.chat.sent(); len(sent) != 2 || !strings.Contains(sent[1], "Plan this ticket.\n\nTicket: Queue sends") || !strings.Contains(sent[1], "Spec:\n## Brief") || strings.Contains(sent[1], "anywhere_factory_rules") {
		t.Fatalf("the plan request: %q", sent)
	}

	w.turn(teamPlan, true)
	w.teams.Ended(leadID)
	w.settle()
	if g := w.got(x.ID); g.Station != proto.StationPlan || g.Gate != proto.GatePlan || g.Status != proto.TicketNeedsYou {
		t.Fatalf("at the gate: %+v", g)
	}

	tm, _, _ := w.teams.Store.Get(leadID)
	tm.State = "running"
	if err := w.teams.Store.Put(tm); err != nil {
		t.Fatal(err)
	}
	w.e.TeamChanged(tm)
	w.settle()
	if g := w.got(x.ID); g.Station != proto.StationImplement || g.Gate != "" || g.Status != proto.TicketWorking {
		t.Fatalf("after the approval: %+v", g)
	}
	acts, _ := ReadActivity(w.home, x.ID)
	want := []string{"anywhere:moved@spec", "anywhere:moved@plan", "you:approved@plan", "anywhere:moved@implement"}
	if !slices.Equal(kindsOf(acts), want) || acts[2].Text != "2 workers" {
		t.Fatalf("activity %v", kindsOf(acts))
	}
}

func TestASecondUnusableSpecFailsTheTicketAtSpec(t *testing.T) {
	w := newWorld(t)
	x := w.started(true)
	w.turn("I read the code.", true)
	w.e.TurnEnded(leadID, true)
	w.settle()
	if sent := w.chat.sent(); len(sent) != 2 || !strings.HasPrefix(sent[1], "Your spec block can't be used: there is no") {
		t.Fatalf("the repair: %q", sent)
	}
	w.turn("Sorry.", true)
	w.e.TurnEnded(leadID, true)
	w.settle()
	g := w.got(x.ID)
	if g.Station != proto.StationSpec || g.Status != proto.TicketFailed || !strings.HasPrefix(g.Why, "The lead sent no usable spec: ") || len(w.chat.sent()) != 2 {
		t.Fatalf("got %+v, sent %d", g, len(w.chat.sent()))
	}
	acts, _ := ReadActivity(w.home, x.ID)
	if last := acts[len(acts)-1]; last.Kind != "failed" || last.Station != proto.StationSpec || last.Text != g.Why {
		t.Fatalf("last event %+v", last)
	}
}

func TestAProjectWithoutTheSpecStationSendsTheRulesWithThePlanRequest(t *testing.T) {
	w := newWorld(t)
	x := w.started(false)
	sent := w.chat.sent()
	if x.Station != proto.StationPlan || len(sent) != 1 || !strings.Contains(sent[0], Rules+"\n\nPlan this ticket.") || strings.Contains(sent[0], "Spec:") {
		t.Fatalf("got %+v, sent %q", x, sent)
	}
}

func TestAskingForChangesAtTheGateSendsTheTicketBackToPlanning(t *testing.T) {
	w := newWorld(t)
	x := w.gated()
	if err := w.teams.Prompt(w.lead(), "Make the tasks smaller"); err != nil {
		t.Fatal(err)
	}
	w.settle()
	if g := w.got(x.ID); g.Station != proto.StationPlan || g.Gate != "" || g.Status != proto.TicketWorking {
		t.Fatalf("got %+v", g)
	}
	acts, _ := ReadActivity(w.home, x.ID)
	if last := acts[len(acts)-1]; last.Actor != "you" || last.Kind != "changes" {
		t.Fatalf("last event %+v", last)
	}
}

func TestALeadThatClosesFailsItsTicketAndLetsItStartAgain(t *testing.T) {
	w := newWorld(t)
	x := w.started(true)
	w.e.Recover()
	w.settle()
	w.reg.Remove(leadID)
	w.e.Status(leadID, "working", "closed")
	w.settle()
	if g := w.got(x.ID); g.Status != proto.TicketFailed || g.Why != "The lead's session closed" || g.AgentID != "" || g.Station != proto.StationSpec {
		t.Fatalf("got %+v", g)
	}
}

func TestALeadThatCannotBePromptedFailsTheTicketWithTheReason(t *testing.T) {
	w := newWorld(t)
	w.chat.err = errors.New("the chat is gone")
	x := w.started(true)
	if x.Status != proto.TicketFailed || x.Why != "The lead couldn't be asked: the chat is gone" {
		t.Fatalf("got %+v", x)
	}
}

func TestTheEngineOwnsAnUnstartedTicketOfAFactoryProjectAndEveryTicketWithAStation(t *testing.T) {
	w := newWorld(t)
	x := w.ticket(true)
	other := proto.Ticket{ID: "o", Folder: "/elsewhere"}
	running := proto.Ticket{ID: "r", Folder: "/repo", AgentID: "a"}
	stationed := proto.Ticket{ID: "s", Folder: "/elsewhere", AgentID: "a", Station: proto.StationImplement}
	if !w.e.Owns(x) || w.e.Owns(other) || w.e.Owns(running) || !w.e.Owns(stationed) {
		t.Fatalf("owns: new %t, other project %t, started before the Factory %t, with a station %t", w.e.Owns(x), w.e.Owns(other), w.e.Owns(running), w.e.Owns(stationed))
	}
}

func TestDetailHoldsTheSpecAndTheActivityAndRefusesATicketTheBoardDoesNotHold(t *testing.T) {
	w := newWorld(t)
	x := w.gated()
	d, err := w.e.Detail(x.ID)
	if err != nil || d.TicketID != x.ID || !strings.HasPrefix(d.Spec, "## Brief") || len(d.Activity) != 2 {
		t.Fatalf("got %+v, %v", d, err)
	}
	if _, err := w.e.Detail("../../etc"); !errors.Is(err, board.ErrUnknown) {
		t.Fatalf("an unknown id: %v", err)
	}
	select {
	case id := <-w.e.Changed():
		if id != x.ID {
			t.Fatalf("changed %q", id)
		}
	default:
		t.Fatal("no change was told")
	}
}

func TestANilEngineIgnoresEveryHookAndRefusesDetail(t *testing.T) {
	var e *Engine
	e.Status("a", "working", "idle")
	e.TurnEnded("a", true)
	e.TeamChanged(proto.Team{Lead: "a"})
	e.Recover()
	e.Run(context.Background())
	if e.Owns(proto.Ticket{Station: proto.StationSpec}) {
		t.Fatal("a nil engine owns a ticket")
	}
	if d, err := e.Detail("t1"); !errors.Is(err, ErrOff) || d.Activity == nil {
		t.Fatalf("got %+v, %v", d, err)
	}
}
