package hosts

import (
	"encoding/json"
	"slices"
	"testing"

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
