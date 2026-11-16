package hosts

import (
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
