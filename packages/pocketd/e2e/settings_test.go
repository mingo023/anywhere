package e2e

import "testing"

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
