//go:build unix

package e2e

import (
	"encoding/json"
	"testing"

	"pocketd/internal/proto"
)

func TestCancellingATaskStopsTheWholeTreeBelowIt(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	// The middle session waits on its own child, which asks to run a command and holds there.
	leaf, _ := json.Marshal(map[string]string{"task": "run ls", "title": "Leaf", "mode": "wait"})
	brief, _ := json.Marshal(map[string]string{"task": "mcp delegate_task " + string(leaf), "title": "Middle"})
	phone.Prompt(parent, "mcp delegate_task "+string(brief))

	middle := childOf(phone, parent).Agent
	grandchild := phone.WaitFor("the grandchild", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == middle.ID
	}).Agent
	phone.WaitFor("the grandchild's ask", func(m Message) bool { return asked(m) && m.Request.AgentID == grandchild.ID })
	var middleTask proto.Delegation
	h.eventually("the middle task running", func() bool {
		for _, d := range delegationsOf(phone, parent) {
			middleTask = d
		}
		return middleTask.Status == "working"
	})

	phone.Prompt(parent, `mcp task_cancel {"taskId":"`+middleTask.TaskID+`"}`)

	h.eventually("both tasks cancelled", func() bool {
		below := delegationsOf(phone, middle.ID)
		return len(below) == 1 && below[0].Status == "cancelled" && delegationsOf(phone, parent)[0].Status == "cancelled"
	})
	h.WaitScreen(grandchild.TerminalID, "interrupt")
	h.WaitScreen(middle.TerminalID, "interrupt")
}
