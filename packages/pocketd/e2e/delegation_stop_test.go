//go:build unix

package e2e

import (
	"testing"
)

func TestAChildTheUserStopsSettlesItsTaskAsCancelledAndWakesTheParent(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"await never","title":"Held"}`)
	child := phone.WaitFor("the child working", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.Status == "working"
	}).Agent

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "stop-child", "agentId": child.ID})

	task := settledAndWoken(h, phone, parent, "cancelled")
	if task.Summary != "Stopped by the user" || task.Error != "" {
		t.Fatalf("task = %+v, want cancelled with the summary %q", task, "Stopped by the user")
	}
}
