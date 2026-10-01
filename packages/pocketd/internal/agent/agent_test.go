package agent

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type fakeDriver struct{}

func (fakeDriver) Prompt(string) error { return nil }
func (fakeDriver) Interrupt() error    { return nil }
func (fakeDriver) Compact() error      { return nil }
func (fakeDriver) Close()              {}

type msg struct {
	Type  string `json:"type"`
	Epoch int64  `json:"epoch"`
	Agent struct {
		Status string `json:"status"`
		Title  string `json:"title"`
	} `json:"agent"`
	Item struct {
		Kind string `json:"kind"`
	} `json:"item"`
}

func drain(ch <-chan []byte) []msg {
	var out []msg
	for {
		select {
		case raw := <-ch:
			var m msg
			json.Unmarshal(raw, &m)
			out = append(out, m)
		default:
			return out
		}
	}
}

func TestTurnLifecycle(t *testing.T) {
	h := hub.New()
	ch, _ := h.Subscribe()
	r := NewRegistry(h)
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	a.Record(timeline.Event{Kind: "user", Text: "fix the tests\nplease"})
	a.Working()
	a.Record(timeline.Event{Kind: "assistant_text", Text: "ok"})
	a.Record(timeline.Event{Kind: "result", OK: true})
	a.TurnEnded(false)

	var got []string
	for _, m := range drain(ch) {
		if m.Type == "agent.update" {
			got = append(got, "update:"+m.Agent.Status)
		} else {
			got = append(got, "stream:"+m.Item.Kind)
		}
	}
	want := "update:idle update:idle stream:user update:working stream:assistant stream:result update:done"
	if strings.Join(got, " ") != want {
		t.Fatalf("got  %s\nwant %s", strings.Join(got, " "), want)
	}
	s := a.Summary()
	if s.Title != "fix the tests" || s.Epoch != 1 || s.MaxSeq != 3 || s.Provider != "claude" {
		t.Fatalf("%+v", s)
	}
}

func TestAITitleWins(t *testing.T) {
	r := NewRegistry(hub.New())
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	a.SetTitle("Fix tests")
	a.Record(timeline.Event{Kind: "user", Text: "hello"})
	if a.Summary().Title != "Fix tests" {
		t.Fatal(a.Summary().Title)
	}
}

func TestRemovePublishesClosed(t *testing.T) {
	h := hub.New()
	r := NewRegistry(h)
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	ch, _ := h.Subscribe()
	r.Remove("a1")
	a.Working()
	a.SetCompacting()
	a.SetTitle("late")
	a.Record(timeline.Event{Kind: "user", Text: "late"})
	if m := drain(ch); len(m) != 1 || m[0].Agent.Status != "closed" {
		t.Fatalf("%+v", m)
	}
	if _, err := r.Get("a1"); err == nil || err.Error() != "Unknown agent: a1" {
		t.Fatal(err)
	}
	if len(r.List()) != 0 {
		t.Fatal("still listed")
	}
}

func TestDriverKnowsAgentBeforeAnyoneSeesIt(t *testing.T) {
	h := hub.New()
	ch, _ := h.Subscribe()
	r := NewRegistry(h)
	var built *Agent
	a := r.AddFunc("a1", "/w", "claude", func(a *Agent) Driver {
		if _, err := r.Get("a1"); err == nil || len(drain(ch)) != 0 {
			t.Error("agent visible before its driver exists")
		}
		built = a
		return fakeDriver{}
	})
	if built != a || a.Driver() != (fakeDriver{}) {
		t.Fatal("driver not built for this agent")
	}
}

func state(s proto.AgentSummary) string {
	out := s.Status
	if s.Failed {
		out += " failed"
	}
	if s.Compacting {
		out += " compacting"
	}
	return out
}

func TestStatusMachine(t *testing.T) {
	for _, tc := range []struct {
		name  string
		steps func(a *Agent)
		want  string
	}{
		{"a new agent is idle", func(a *Agent) {}, "idle"},
		{"working", func(a *Agent) { a.Working() }, "working"},
		{"needs you", func(a *Agent) { a.Working(); a.NeedsYou() }, "needsYou"},
		{"an unseen turn end is done", func(a *Agent) { a.Working(); a.TurnEnded(false) }, "done"},
		{"a failed turn is done failed", func(a *Agent) { a.Working(); a.TurnEnded(true) }, "done failed"},
		{"a turn can end from needs you", func(a *Agent) { a.NeedsYou(); a.TurnEnded(false) }, "done"},
		{"turn end while idle is ignored", func(a *Agent) { a.TurnEnded(true) }, "idle"},
		{"working clears done failed", func(a *Agent) { a.Working(); a.TurnEnded(true); a.Working() }, "working"},
		{"failed is only sent with done", func(a *Agent) { a.Working(); a.TurnEnded(true); a.NeedsYou() }, "needsYou"},
		{"an interrupt is not done", func(a *Agent) { a.Working(); a.Clear() }, "idle"},
		{"an interrupt from needs you is not done", func(a *Agent) { a.NeedsYou(); a.Clear() }, "idle"},
		{"clear while idle keeps done", func(a *Agent) { a.Working(); a.TurnEnded(false); a.Clear() }, "done"},
		{"compacting from idle works", func(a *Agent) { a.SetCompacting() }, "working compacting"},
		{"compaction from idle ends like a turn", func(a *Agent) { a.SetCompacting(); a.Compacted() }, "done"},
		{"compaction mid-turn keeps the turn", func(a *Agent) { a.Working(); a.SetCompacting(); a.Compacted() }, "working"},
		{"a turn end during compaction keeps compacting", func(a *Agent) { a.Working(); a.SetCompacting(); a.TurnEnded(false) }, "done compacting"},
		{"an interrupt ends a compaction from idle", func(a *Agent) { a.SetCompacting(); a.Clear() }, "idle"},
		{"a failed compaction from idle ends it", func(a *Agent) { a.SetCompacting(); a.TurnEnded(true) }, "done failed"},
		{"an ended compaction from idle leaves the next turn alone", func(a *Agent) { a.SetCompacting(); a.Clear(); a.Working(); a.Compacted() }, "working"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
			tc.steps(a)
			if got := state(a.Summary()); got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
		})
	}
}

func TestEachChangePublishesOnce(t *testing.T) {
	h := hub.New()
	a := NewRegistry(h).Add("a1", "/w", "claude", fakeDriver{})
	ch, _ := h.Subscribe()
	a.Working()
	a.Working()
	a.SetTerminal("t1")
	a.SetTerminal("t1")
	a.TurnEnded(false)
	a.TurnEnded(false)
	a.SetAttached(true)
	var got []string
	for _, m := range drain(ch) {
		got = append(got, m.Agent.Status)
	}
	if strings.Join(got, " ") != "working working done" {
		t.Fatalf("got %v", got)
	}
}

func TestSettersShowInTheSummary(t *testing.T) {
	a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
	if s := a.Summary(); s.TerminalID != "" || s.ProviderSessionID != "" || !s.Attached {
		t.Fatalf("new agent: %+v", s)
	}
	a.SetTerminal("t1")
	a.SetAttached(false)
	a.SetCwd("/x")
	a.SetConversation("c1")
	s := a.Summary()
	if s.TerminalID != "t1" || s.Attached || s.Cwd != "/x" || s.ProviderSessionID != "c1" || a.Provider() != "claude" {
		t.Fatalf("%+v", s)
	}
}

func TestConversationSwitchStartsOver(t *testing.T) {
	a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
	a.SetConversation("c1")
	a.Record(timeline.Event{Kind: "user", Text: "one"})
	a.SetConversation("c1")
	if s := a.Summary(); s.Title != "one" || s.Epoch != 1 {
		t.Fatalf("same conversation reset: %+v", s)
	}
	a.SetConversation("c2")
	items, _ := a.Timeline.Page(0, 200)
	if s := a.Summary(); s.Title != "" || s.Epoch != 2 || s.MaxSeq != 1 || len(items) != 0 || s.ProviderSessionID != "c2" {
		t.Fatalf("after switch: %+v %v", s, items)
	}
}

func TestRecordLeavesStatusAlone(t *testing.T) {
	a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
	a.Record(timeline.Event{Kind: "user", Text: "hi"})
	a.Record(timeline.Event{Kind: "result", OK: false})
	if s := a.Summary(); s.Status != "idle" || s.Title != "hi" || s.MaxSeq != 2 {
		t.Fatalf("%+v", s)
	}
}

func TestAnAgentOnScreenIsNeverDone(t *testing.T) {
	r := NewRegistry(hub.New())
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	b := r.Add("a2", "/w", "claude", fakeDriver{})
	r.SetView("phone", []string{"a1", "zz"})
	r.SetView("desk", []string{"a1"})
	for _, x := range []*Agent{a, b} {
		x.Working()
		x.TurnEnded(true)
	}
	if got := state(a.Summary()) + ", " + state(b.Summary()); got != "idle, done failed" {
		t.Fatal(got)
	}
	r.DropView("phone")
	a.Working()
	a.TurnEnded(false)
	if got := state(a.Summary()); got != "idle" {
		t.Fatalf("still shown on desk: %s", got)
	}
	r.SetView("desk", []string{})
	a.Working()
	a.TurnEnded(false)
	if got := state(a.Summary()); got != "done" {
		t.Fatalf("shown nowhere: %s", got)
	}
}

func TestShowingADoneAgentClearsDone(t *testing.T) {
	h := hub.New()
	r := NewRegistry(h)
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	a.Working()
	a.TurnEnded(true)
	at := a.Summary().UpdatedAt
	time.Sleep(2 * time.Millisecond)
	ch, _ := h.Subscribe()
	r.SetView("phone", []string{"a1"})
	r.SetView("phone", []string{"a1"})
	if m := drain(ch); len(m) != 1 || m[0].Agent.Status != "idle" || a.Summary().UpdatedAt != at {
		t.Fatalf("%+v", m)
	}
}

func TestMarkSeenClearsDoneOnce(t *testing.T) {
	r := NewRegistry(hub.New())
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	a.Working()
	a.TurnEnded(false)
	at := a.Summary().UpdatedAt
	time.Sleep(2 * time.Millisecond)
	r.MarkSeen([]string{"a1", "zz"})
	if s := a.Summary(); state(s) != "idle" || s.UpdatedAt != at {
		t.Fatalf("%+v", s)
	}
	a.Working()
	a.TurnEnded(false)
	if got := state(a.Summary()); got != "done" {
		t.Fatalf("mark seen lasted: %s", got)
	}
}

func TestOnStatusFiresOnEveryStatusChange(t *testing.T) {
	reg := NewRegistry(hub.New())
	var got []string
	reg.OnStatus = func(id, provider, from, to string) { got = append(got, id+" "+provider+" "+from+">"+to) }
	a := reg.Add("a1", "/w", "claude", fakeDriver{})
	a.Working()
	a.SetTitle("x")
	a.NeedsYou()
	a.Working()
	a.TurnEnded(false)
	reg.Remove("a1")
	want := []string{"a1 claude idle>working", "a1 claude working>needsYou", "a1 claude needsYou>working", "a1 claude working>done", "a1 claude done>closed"}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("%q", got)
	}
}

func TestBusyCountsWorkingAndNeedsYou(t *testing.T) {
	with := func(statuses ...string) []proto.AgentSummary {
		var out []proto.AgentSummary
		for _, s := range statuses {
			out = append(out, proto.AgentSummary{Status: s})
		}
		return out
	}
	for _, c := range []struct {
		agents []proto.AgentSummary
		busy   bool
	}{
		{nil, false},
		{with("idle", "done", "closed"), false},
		{with("idle", "working"), true},
		{with("needsYou"), true},
	} {
		if Busy(c.agents) != c.busy {
			t.Errorf("%v: want %v", c.agents, c.busy)
		}
	}
}
