//go:build unix

package e2e

import (
	"strings"
	"testing"
)

func TestAChatDelegatesOverMCPAndIsWokenWhenTheChildSettles(t *testing.T) {
	_, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","title":"Tests"}`)

	var child string
	phone.WaitFor("the child, in a worktree of its own", func(m Message) bool {
		if m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.Relationship == "subagent" && m.Agent.Branch != "main" {
			child = m.Agent.ID
		}
		return child != ""
	})
	phone.WaitAll("the child's turn and the parent's wake",
		streamOf(child, "assistant", "echo: write the tests"),
		func(m Message) bool {
			return m.Type == "delegation.update" && m.AgentID == parent && m.Delegation.ChildID == child &&
				m.Delegation.Status == "completed" && m.Delegation.Summary == "echo: write the tests"
		},
		func(m Message) bool {
			return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "user" &&
				strings.Contains(m.Item.Text, "reached a terminal state")
		},
	)
}

func TestAToolRefusesAnUnknownTaskWithItsCode(t *testing.T) {
	_, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp task_status {"taskId":"nope"}`)
	phone.WaitFor("the refusal", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "assistant" && strings.Contains(m.Item.Text, "not_found")
	})
}
