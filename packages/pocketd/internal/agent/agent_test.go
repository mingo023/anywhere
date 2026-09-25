package agent

import (
	"encoding/json"
	"strings"
	"testing"

	"pocketd/internal/hub"
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
	a.Apply(timeline.Event{Kind: "user", Text: "fix the tests\nplease"})
	a.Apply(timeline.Event{Kind: "assistant_text", Text: "ok"})
	a.Apply(timeline.Event{Kind: "result", OK: true})

	var got []string
	for _, m := range drain(ch) {
		if m.Type == "agent.update" {
			got = append(got, "update:"+m.Agent.Status)
		} else {
			got = append(got, "stream:"+m.Item.Kind)
		}
	}
	want := "update:idle stream:user update:running stream:assistant stream:result update:idle"
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
	a.Apply(timeline.Event{Kind: "user", Text: "hello"})
	if a.Summary().Title != "Fix tests" {
		t.Fatal(a.Summary().Title)
	}
}

func TestRemovePublishesClosed(t *testing.T) {
	h := hub.New()
	r := NewRegistry(h)
	r.Add("a1", "/w", "claude", fakeDriver{})
	ch, _ := h.Subscribe()
	r.Remove("a1")
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
