package e2e

import "testing"

func TestPhoneApprovesTool(t *testing.T) {
	for _, decision := range []string{"allow", "deny"} {
		t.Run(decision, func(t *testing.T) {
			h, phone, term, id := StartClaude(t)

			h.Prompt(term, "run ls")
			req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })
			if req.Request.AgentID != id || req.Request.ToolName != "Bash" || req.Request.Detail.Command != "ls" {
				t.Fatalf("request: %s", req.Raw)
			}
			phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": decision})
			phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "r1" })
			h.WaitScreen(term, "hook: "+decision)
			status := map[string]string{"allow": "ok", "deny": "error"}[decision]
			phone.WaitFor("tool "+status, func(m Message) bool {
				return m.Type == "agent.stream" && m.Item.Kind == "tool" && m.Item.Call.Status == status
			})
		})
	}
}

func TestDesktopAnswerClearsPhoneCard(t *testing.T) {
	h, phone, term, _ := StartClaude(t)

	h.Prompt(term, "desk ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })
	res := phone.WaitFor("permission.resolved", func(m Message) bool { return m.Type == "permission.resolved" })
	if res.RequestID != req.Request.RequestID || res.Decision != "allow" {
		t.Fatalf("resolved: %s", res.Raw)
	}
	h.WaitScreen(term, `hook released: ""`)
}

func TestReconnectingPhoneSeesOpenRequest(t *testing.T) {
	h, first, term, id := StartClaude(t)
	h.Prompt(term, "run ls")
	first.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	second := h.Phone()
	second.WaitFor("open request", func(m Message) bool { return m.Type == "permission.request" && m.Request.AgentID == id })
}
