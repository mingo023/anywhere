//go:build unix

package e2e

import (
	"testing"

	"pocketd/internal/ops"
)

func TestClosingAParentsTerminalClosesTheSessionsItDelegatedTo(t *testing.T) {
	h, phone, term, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Finished"}`)
	finished := phone.WaitFor("the finished child", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.Status == "done"
	}).Agent
	phone.Prompt(parent, `mcp delegate_task {"task":"await never","title":"Held"}`)
	held := phone.WaitFor("the held child working", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.ID != finished.ID && m.Agent.Status == "working"
	}).Agent

	h.Ops().Send(ops.Msg{Op: "close", ID: term})

	phone.WaitStatus(parent, "closed")
	phone.WaitStatus(finished.ID, "closed")
	phone.WaitStatus(held.ID, "closed")
}
