//go:build unix

package e2e

import (
	"encoding/json"
	"slices"
	"strings"
	"syscall"
	"testing"

	"pocketd/internal/orch"
	"pocketd/internal/orchhost"
	"pocketd/internal/proto"
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

// delegationsOf is parent's delegations as agent.delegations gives them.
func delegationsOf(p *Phone, parent string) []proto.Delegation {
	p.t.Helper()
	p.Send(map[string]any{"type": "agent.delegations", "id": "d-" + parent, "agentId": parent})
	var page struct {
		Items []proto.Delegation `json:"items"`
	}
	json.Unmarshal([]byte(p.WaitFor("the delegations", func(m Message) bool { return m.Type == "agent.delegations" && m.ID == "d-"+parent }).Raw), &page)
	return page.Items
}

// settledAndWoken waits until parent's one task has status and parent's timeline holds its wake.
func settledAndWoken(h *Harness, p *Phone, parent, status string) proto.Delegation {
	h.t.Helper()
	var task proto.Delegation
	h.eventually("the task "+status, func() bool {
		ts := delegationsOf(p, parent)
		if len(ts) == 1 {
			task = ts[0]
		}
		return task.Status == status
	})
	h.eventually("the parent's wake", func() bool {
		return slices.ContainsFunc(timelineOf(p, parent), func(s string) bool {
			return strings.HasPrefix(s, "user Delegated task "+task.TaskID+" reached a terminal state")
		})
	})
	return task
}

func TestAnUpgradeMidDelegationKeepsTheChildWorkingAndWakesTheParentWithItsResult(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"run ls","title":"List"}`)
	first := phone.WaitFor("the child's ask", func(m Message) bool { return asked(m) && m.Request.AgentID != parent })

	h.Upgrade()

	again := h.Owner()
	req := again.WaitFor("the child's ask again", func(m Message) bool {
		return asked(m) && m.Request.AgentID == first.Request.AgentID && m.Request.RequestID != first.Request.RequestID
	})
	resolve(again, req, map[string]any{"decision": "allow"})
	task := settledAndWoken(h, again, parent, "completed")
	if task.ChildID != first.Request.AgentID || task.Summary != "echo: ran ls" {
		t.Fatalf("task = %+v", task)
	}
}

func TestAColdRestartMidDelegationFailsTheTaskAndWakesTheResumedParent(t *testing.T) {
	h, phone, _, parent := chatClaude(t, func(h *Harness) { h.resume = true })
	phone.Prompt(parent, `mcp delegate_task {"task":"run ls","title":"List"}`)
	phone.WaitFor("the child's ask", func(m Message) bool { return asked(m) && m.Request.AgentID != parent })

	h.Restart()

	task := settledAndWoken(h, h.Owner(), parent, "failed")
	if !strings.Contains(task.Error, "restarted") {
		t.Fatalf("task = %+v", task)
	}
}

func TestAColdRestartWithoutResumeFailsTheTaskOfTheChildItEnded(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"run ls","title":"List"}`)
	phone.WaitFor("the child's ask", func(m Message) bool { return asked(m) && m.Request.AgentID != parent })

	h.Restart()

	again := h.Owner()
	h.eventually("the task failed", func() bool {
		ts := delegationsOf(again, parent)
		return len(ts) == 1 && ts[0].Status == "failed" && strings.Contains(ts[0].Error, "restarted")
	})
}

func TestALaunchCommittedBeforeARestartStartsAfterIt(t *testing.T) {
	h, phone, _, parent := chatClaude(t, func(h *Harness) { h.resume = true })
	phone.Prompt(parent, "first")
	phone.WaitStatus(parent, "done")
	h.Stop(syscall.SIGTERM)
	store, err := orch.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	// The parent is mid-turn when it delegates; this pocketd stops before the launch runs.
	store.DB().Exec(`UPDATE sessions SET status = 'working' WHERE id = ?`, parent)
	e := orch.NewEngine(store, nil)
	e.Agent = orchhost.AgentInfo
	if _, err := e.Dispatch(orch.NewCommand("", orch.CmdDelegate, parent, orch.DelegatePayload{Agent: "claude", Brief: "after the restart", Title: "Later"})); err != nil {
		t.Fatal(err)
	}
	store.DB().Exec(`UPDATE sessions SET status = 'done' WHERE id = ?`, parent)
	store.Close()

	h.up()

	task := settledAndWoken(h, h.Owner(), parent, "completed")
	if task.ChildID == "" || task.Summary != "echo: after the restart" {
		t.Fatalf("task = %+v", task)
	}
}
