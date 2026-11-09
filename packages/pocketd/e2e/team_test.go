//go:build unix

package e2e

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

// plan is a usable plan block. fakeclaude echoes its prompt, the rules then the user's text, so a plan block in the
// text comes after the rules' example and is the one pocketd reads.
const plan = `<anywhere_team_plan>{"tasks":[` +
	`{"id":"t1","title":"Add truncate","agent":"claude","files":["src/truncate.js"],"after":[],"allow":["npm test"],"instructions":"Write truncate and its test"},` +
	`{"id":"t2","title":"Export truncate","agent":"codex","files":["src/index.js"],"after":["t1"],"allow":[],"instructions":"Export it from src/index.js"}` +
	`]}</anywhere_team_plan>`

var planned = []proto.TeamTask{
	{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate and its test", State: "planned"},
	{ID: "t2", Title: "Export truncate", Agent: "codex", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, Instructions: "Export it from src/index.js", State: "planned"},
}

func teamOf(lead, state string) func(Message) bool {
	return func(m Message) bool { return m.Type == "team.update" && m.Team.Lead == lead && m.Team.State == state }
}

func teamPrompt(p *Phone, lead, text string) {
	p.Send(map[string]any{"type": "agent.prompt", "id": "team-prompt", "agentId": lead, "text": text, "team": true})
}

func teamList(p *Phone) []proto.Team {
	p.Send(map[string]any{"type": "team.list", "id": "tl"})
	return p.WaitFor("team.list", func(m Message) bool { return m.Type == "team.list" && m.ID == "tl" }).Teams
}

func TestATeamPromptMakesTheLeadPlanAndPocketdProposesItsPlan(t *testing.T) {
	h, phone, term, lead := chatClaude(t)

	teamPrompt(phone, lead, plan)

	var got proto.Team
	phone.WaitAll("planning, the user's text and the proposed plan",
		teamOf(lead, "planning"),
		// The chat shows what the user typed, not the rules before it.
		streamOf(lead, "user", plan),
		func(m Message) bool {
			if teamOf(lead, "proposed")(m) {
				got = m.Team
				return true
			}
			return false
		},
	)
	if !reflect.DeepEqual(got.Tasks, planned) || got.Repaired || got.Error != "" {
		t.Fatalf("got %+v", got)
	}
	h.WaitScreen(term, "prompt: <anywhere_team>")
	if teams := teamList(phone); len(teams) != 1 || !reflect.DeepEqual(teams[0], got) {
		t.Fatalf("team.list: %+v", teams)
	}
}

func TestAPlanThatCantBeUsedGetsOneRepairTurnThenTheTeamFails(t *testing.T) {
	_, phone, _, lead := chatClaude(t)

	// The echo holds only the rules' example, which has one task.
	teamPrompt(phone, lead, "add truncate")

	phone.WaitStream(lead, "user", "Your plan block can't be used: a plan needs 2 to 4 tasks, not 1. Send the corrected <anywhere_team_plan> block only.")
	got := phone.WaitFor("the failed team", teamOf(lead, "failed")).Team
	if !got.Repaired || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v", got)
	}
}

func TestANewSessionStartedAsATeamPlansItsFirstPrompt(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()

	create(phone, "r1", inRepo(h, map[string]any{"prompt": plan, "chat": true, "team": true}))

	var lead string
	var proposed proto.Team
	phone.WaitAll("the session and its plan",
		func(m Message) bool {
			if m.Type == "agent.created" {
				lead = m.AgentID
				return true
			}
			return false
		},
		func(m Message) bool {
			if m.Type == "team.update" && m.Team.State == "proposed" {
				proposed = m.Team
				return true
			}
			return false
		},
	)
	if proposed.Lead != lead || !reflect.DeepEqual(proposed.Tasks, planned) {
		t.Fatalf("lead %s, got %+v", lead, proposed)
	}
}

func TestATeamIsStillListedAfterARestart(t *testing.T) {
	h, phone, _, lead := chatClaude(t)
	teamPrompt(phone, lead, plan)
	phone.WaitFor("the proposed plan", teamOf(lead, "proposed"))

	h.Restart()

	teams := teamList(h.Owner())
	if len(teams) != 1 || teams[0].Lead != lead || teams[0].State != "proposed" || !reflect.DeepEqual(teams[0].Tasks, planned) {
		t.Fatalf("got %+v", teams)
	}
}
