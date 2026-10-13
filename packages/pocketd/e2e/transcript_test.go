//go:build unix

package e2e

import "testing"

func TestPhonePromptStreamsTranscript(t *testing.T) {
	h, phone, term, id := StartClaude(t)
	if id == term {
		t.Fatalf("agent id is the terminal id %s", id)
	}

	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": id, "text": "hello"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" })
	phone.WaitStatus(id, "working")
	// Stop is a hook and the stream is a tailed file, so done can come first.
	phone.WaitAll("the turn",
		streamOf(id, "user", "hello"),
		streamOf(id, "assistant", "echo: hello"),
		func(m Message) bool { return m.Type == "agent.stream" && m.Item.Kind == "result" && m.Item.OK },
		statusOf(id, "done"),
	)
	h.WaitScreen(term, "echo: hello")
}

func TestAClaudeTypedInATerminalIsNamedFromItsFirstPrompt(t *testing.T) {
	_, phone, _, id := StartClaude(t)
	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": id, "text": "hello"})
	phone.WaitFor("the generated name", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Title == "Say hello"
	})
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
