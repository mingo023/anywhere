package hosts

import (
	"context"
	"encoding/json"
	"fmt"
	"slices"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func TestAHostsSessionIsKeptUnderItsTagWithNoTerminalHere(t *testing.T) {
	got, ok := mirrored("devbox", proto.AgentSummary{ID: "a2", Parent: "a1", TerminalID: "t9", Cwd: "/srv/app", Status: "working"})
	if !ok || got.ID != "devbox/a2" || got.Parent != "devbox/a1" || got.TerminalID != "" || got.Cwd != "/srv/app" {
		t.Fatalf("mirrored = %+v, %v", got, ok)
	}
	if got, _ := mirrored("devbox", proto.AgentSummary{ID: "a3"}); got.Parent != "" {
		t.Errorf("a session without a parent got %q", got.Parent)
	}
}

func TestAnIDFromAHostThatCouldClimbOutOfAFolderIsRefused(t *testing.T) {
	for _, s := range []proto.AgentSummary{{ID: "../../etc"}, {ID: "a2", Parent: "a/../b"}, {ID: ""}} {
		if got, ok := mirrored("devbox", s); ok {
			t.Errorf("%+v was kept as %+v", s, got)
		}
	}
}

type asks struct {
	Mirror
	asked, answered []string
}

func (a *asks) Summary(proto.AgentSummary) {}
func (a *asks) Asked(session, request string, _ json.RawMessage) {
	a.asked = append(a.asked, session+" "+request)
}
func (a *asks) Answered(session, request string) {
	a.answered = append(a.answered, session+" "+request)
}

func TestAQuestionAskedBeforeTheLaunchReturnedReachesTheMirrorOnceTracked(t *testing.T) {
	m := &asks{}
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox")
	ask := func(agent, request string) {
		detail := proto.ToolDetail{Kind: "other", Name: "AskUserQuestion", Input: json.RawMessage(`{"questions":[{"question":"Which?"}]}`)}
		raw, err := json.Marshal(proto.NewPermissionRequest(proto.PermissionRequest{RequestID: request, AgentID: agent, ToolName: "AskUserQuestion", Detail: detail}))
		if err != nil {
			t.Fatal(err)
		}
		r.frame("permission.request", raw)
	}
	ask("a2", "q1")
	ask("a2", "q2")
	raw, _ := json.Marshal(proto.NewPermissionResolved("q2", "allow"))
	r.frame("permission.resolved", raw)
	ask("a3", "q3")
	if len(m.asked) != 0 {
		t.Fatalf("asked before tracking: %v", m.asked)
	}
	r.track("a2", proto.AgentSummary{ID: "a2", Provider: "claude"})
	if !slices.Equal(m.asked, []string{"devbox/a2 devbox/q1"}) {
		t.Fatalf("asked = %v, want only a2's open question", m.asked)
	}
	raw, _ = json.Marshal(proto.NewPermissionResolved("q1", "allow"))
	r.frame("permission.resolved", raw)
	if !slices.Equal(m.answered, []string{"devbox/a2 devbox/q1"}) {
		t.Errorf("answered = %v", m.answered)
	}
}

type seqs struct {
	Mirror
	got []int64
}

func (m *seqs) Item(_ string, _ int64, it proto.Item) { m.got = append(m.got, it.Seq) }

func TestASessionMoreThanAPageAheadIsMirroredFromItsFirstUnseenItem(t *testing.T) {
	var all []proto.Item
	for seq := int64(1); seq <= 1234; seq++ {
		all = append(all, proto.Item{ID: fmt.Sprint("i", seq), Seq: seq, Kind: "assistant", Text: "x"})
	}
	// Answers as a host's Timeline does: the newest limit items past sinceSeq, or before beforeSeq.
	socket := fakePocketd(t, func(read func() map[string]any, send func(any)) {
		for m := read(); m != nil; m = read() {
			since, _ := m["sinceSeq"].(float64)
			before, backwards := m["beforeSeq"].(float64)
			var match []proto.Item
			for _, it := range all {
				if backwards && float64(it.Seq) < before || !backwards && float64(it.Seq) > since {
					match = append(match, it)
				}
			}
			page := match[len(match)-min(len(match), int(m["limit"].(float64))):]
			send(proto.NewAgentTimeline(m["id"].(string), "a1", 1, page, len(page) < len(match) || !backwards && since > 0, 1234))
		}
	})
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	c, err := Dial(ctx, socket, "", func(string, []byte) {})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	m := &seqs{}
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox")
	r.client = c
	r.seq["a1"] = 100
	r.catchUp("a1", false)
	if len(m.got) != 1134 || m.got[0] != 101 || !slices.IsSorted(m.got) || m.got[len(m.got)-1] != 1234 {
		t.Fatalf("mirrored %d items, from %v", len(m.got), m.got[:min(len(m.got), 3)])
	}
}
