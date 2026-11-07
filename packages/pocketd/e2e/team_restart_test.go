//go:build unix

package e2e

import (
	"encoding/json"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

// askingPlan is a lead's plan whose t1 runs `make build`, which its allowlist
// doesn't hold, so t1's worker waits on the user; t2 starts from t1's work.
func askingPlan(t *testing.T) string {
	t.Helper()
	plan, err := json.Marshal(map[string]any{"tasks": []map[string]any{
		{"id": "t1", "title": "Build a", "agent": "claude", "files": []string{"a.txt"}, "after": []string{}, "allow": []string{"make test"},
			"instructions": "write a.txt one\nrun make build"},
		{"id": "t2", "title": "Write b", "agent": "claude", "files": []string{"b.txt"}, "after": []string{"t1"}, "allow": []string{},
			"instructions": "write b.txt two"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	return "<anywhere_team_plan>\n" + string(plan) + "\n</anywhere_team_plan>"
}

// leadsTeam is lead's team in a team.update or a team.list reply.
func leadsTeam(lead string, m Message) (proto.Team, bool) {
	if m.Type == "team.update" && m.Team.Lead == lead {
		return m.Team, true
	}
	if m.Type == "team.list" {
		for _, tm := range m.Teams {
			if tm.Lead == lead {
				return tm, true
			}
		}
	}
	return proto.Team{}, false
}

func TestARestartFailsATaskWhoseWorkerDidNotComeBackAndTellsTheLead(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity)
	o := h.Owner()
	create(o, "lead", inRepo(h, map[string]any{"chat": true}))
	created := reply(o, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	lead := created.AgentID
	h.WaitScreen(created.TerminalID, "fake claude ready")
	o.Send(map[string]any{"type": "agent.prompt", "id": "plan", "agentId": lead, "text": askingPlan(t), "team": true})
	o.WaitFor("the proposed plan", teamOf(lead, "proposed"))
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": lead, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	o.WaitFor("t1 asking to run make build", taskIn(lead, "t1", "needsYou"))

	h.Restart()
	again := h.Owner()
	again.Send(map[string]any{"type": "team.list", "id": "tl"})
	m := again.WaitFor("t1 failed by the restart", func(m Message) bool {
		tm, ok := leadsTeam(lead, m)
		return ok && len(tm.Tasks) > 0 && tm.Tasks[0].State == "failed"
	})
	team, _ := leadsTeam(lead, m)
	if team.State != "running" || team.Tasks[0].Summary != "the worker stopped when pocketd restarted" || team.Tasks[1].State != "waiting" {
		t.Fatalf("team %+v", team)
	}
	stop(t, h)
	want := []string{"<anywhere_team_handback task=\"t1\" status=\"failed\">\nthe worker stopped when pocketd restarted\n</anywhere_team_handback>"}
	if got := unacked(t, h, lead); !reflect.DeepEqual(got, want) {
		t.Fatalf("the lead's mail %q", got)
	}
}
