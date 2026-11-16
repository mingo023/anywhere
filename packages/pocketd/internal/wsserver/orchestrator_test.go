package wsserver

import (
	"testing"

	"pocketd/internal/orch"
)

func TestAgentListGivesTheStoresOpenSessionsAsTheRegistryHasThemLive(t *testing.T) {
	stored := []orch.Session{
		{ID: "a0", Agent: "codex", Title: "Kept", Status: "done", CreatedAt: 1},
		{ID: "a1", Agent: "claude", Title: "Stale", Status: "working", CreatedAt: 2},
	}
	reg, _, p := setup(t, func(s *Server) {
		s.OpenSessions = func() ([]orch.Session, error) { return stored, nil }
	})
	a1, _ := reg.Get("a1")
	a1.SetTitle("Live")
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	p.recv()
	m := p.recv()
	if m["type"] != "agent.list" {
		t.Fatalf("%v", m)
	}
	var got []string
	for _, a := range m["agents"].([]any) {
		a := a.(map[string]any)
		got = append(got, a["id"].(string)+" "+a["title"].(string)+" "+a["status"].(string))
	}
	if want := []string{"a0 Kept done", "a1 Live idle"}; len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("listed %q, want %q", got, want)
	}
}
