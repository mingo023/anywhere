//go:build unix

package e2e

import (
	"slices"
	"testing"
)

func TestClaudesTaskListReachesThePhoneAsTasks(t *testing.T) {
	h, phone, term, _ := StartClaude(t)

	h.Prompt(term, "plan")
	want := []Task{{Text: "Read the code", Status: "completed"}, {Text: "Fix it", Status: "pending"}}
	phone.WaitFor("tasks with the first done", func(m Message) bool {
		return m.Type == "agent.stream" && m.Item.Kind == "tasks" && slices.Equal(m.Item.Items, want)
	})
}
