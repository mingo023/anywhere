package e2e

import "testing"

func TestPhonePromptStreamsTranscript(t *testing.T) {
	h, phone, id := StartClaude(t)
	phone.WaitStatus(id, "idle")

	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": id, "text": "hello"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" })
	phone.WaitStream(id, "user", "hello")
	phone.WaitStatus(id, "running")
	phone.WaitStream(id, "assistant", "echo: hello")
	phone.WaitFor("result", func(m Message) bool { return m.Type == "agent.stream" && m.Item.Kind == "result" && m.Item.OK })
	phone.WaitStatus(id, "idle")
	h.WaitScreen(id, "echo: hello")
}

func TestPlainCommandIsNotAnAgent(t *testing.T) {
	h := Start(t)
	phone := h.Phone()
	h.Spawn("sh")
	phone.Send(map[string]any{"type": "agent.list", "id": "l"})
	m := phone.WaitFor("agent.list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "l" })
	if len(m.Agents) != 0 {
		t.Fatalf("sh became an agent: %s", m.Raw)
	}
}
