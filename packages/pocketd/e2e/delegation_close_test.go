//go:build unix

package e2e

import (
	"slices"
	"testing"

	"pocketd/internal/ops"
	"pocketd/internal/terminal"
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
	h.eventually("the children's terminals to end", func() bool {
		return !h.lists(finished.TerminalID) && !h.lists(held.TerminalID)
	})
}

func (h *Harness) lists(id string) bool {
	c := h.Ops()
	defer c.Close()
	c.Send(ops.Msg{Op: "list"})
	m, err := c.Recv()
	if err != nil {
		h.t.Fatalf("list: %v", err)
	}
	return slices.ContainsFunc(m.Items, func(i terminal.Info) bool { return i.ID == id })
}
