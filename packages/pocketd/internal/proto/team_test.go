package proto

import (
	"encoding/json"
	"slices"
	"testing"
)

func TestTheTeamGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(golden(t, "client", "agent_prompt_team.json"))
	if err != nil || !m.Team || m.AgentID != "a1" || m.Text != "Add truncate and titleCase, each with its own test" {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "agent_prompt.json")); err != nil || m.Team {
		t.Fatalf("a prompt without team: %+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "agent_create_team.json")); err != nil || !m.Spec.Team || !m.Spec.Chat {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "team_list.json")); err != nil || m.Type != "team.list" || m.ID != "tl1" {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestATeamFieldOffTheContractIsMalformed(t *testing.T) {
	for _, raw := range []string{
		`{"type":"agent.prompt","id":"1","agentId":"a","text":"x","team":"yes"}`,
		`{"type":"agent.prompt","id":"1","agentId":"a","text":"x","team":null}`,
		`{"type":"team.list"}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"team":"yes"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"Team":true}}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestATeamGoesOutWithEveryListItsSchemaRequires(t *testing.T) {
	got, _ := json.Marshal(NewTeamUpdate(Team{Lead: "a1", State: "planning", Tasks: []TeamTask{{ID: "t1", State: "planned"}}}))
	want := `{"type":"team.update","team":{"lead":"a1","state":"planning","tasks":[{"id":"t1","title":"","agent":"","files":[],"after":[],"allow":[],"instructions":"","state":"planned"}],"updatedAt":0}}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	got, _ = json.Marshal(NewTeamList("tl1", []Team{{Lead: "a1", State: "planning"}}))
	want = `{"type":"team.list","id":"tl1","teams":[{"lead":"a1","state":"planning","tasks":[],"updatedAt":0}]}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
}

func TestTheServerOffersTheTeamCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapTeam}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapTeam}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}
