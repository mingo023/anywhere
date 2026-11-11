package e2e

import (
	"strings"
	"testing"
)

func TestAPhoneSwitchesClaudesModelAndEffort(t *testing.T) {
	h, phone, term, agent := StartClaude(t)

	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": agent, "model": "opus", "effort": "high"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	h.WaitScreen(term, "/model opus")
	h.WaitScreen(term, "/effort high")
	phone.WaitFor("the switched summary", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == agent && m.Agent.Model == "opus" && m.Agent.Effort == "high"
	})
}

func TestClaudeTakesItsModelsSettingsOnlyAsItStarts(t *testing.T) {
	_, phone, _, agent := StartClaude(t)

	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": agent, "options": map[string]string{"fast": "true"}})
	refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s1" })
	if !strings.Contains(refused.Raw, "This agent takes its model's settings only as it starts") {
		t.Fatalf("got %s", refused.Raw)
	}
}
