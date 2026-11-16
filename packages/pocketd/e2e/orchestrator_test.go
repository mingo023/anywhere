//go:build unix

package e2e

import (
	"bytes"
	"encoding/json"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"pocketd/internal/ops"
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

// storedAsLive waits until the store holds id's live timeline as the wire
// shows it, sub-agents' items included, each row at the timeline's epoch, then
// returns the store, open.
func storedAsLive(h *Harness, id string) *orch.Store {
	h.t.Helper()
	p := h.Owner(proto.CapSubagents)
	store, err := orch.Open(h.Home)
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { store.Close() })
	var live, stored []byte
	var stale int
	for deadline := time.Now().Add(10 * time.Second); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		p.Send(map[string]any{"type": "agent.timeline", "id": "s-" + id, "agentId": id})
		var page struct {
			Epoch int64        `json:"epoch"`
			Items []proto.Item `json:"items"`
		}
		json.Unmarshal([]byte(p.WaitFor("the timeline", func(m Message) bool { return m.Type == "agent.timeline" && m.ID == "s-"+id }).Raw), &page)
		items, err := store.Items(id)
		if err != nil {
			h.t.Fatal(err)
		}
		live, _ = json.Marshal(page.Items)
		stored, _ = json.Marshal(items)
		store.DB().QueryRow(`SELECT COUNT(*) FROM items WHERE session_id = ? AND epoch != ?`, id, page.Epoch).Scan(&stale)
		if len(page.Items) > 0 && bytes.Equal(live, stored) && stale == 0 {
			return store
		}
	}
	h.t.Fatalf("the store holds\n%s\n(%d rows off the epoch), the live timeline\n%s", stored, stale, live)
	return nil
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

func TestAnAgentThatIsntAttachedRecordsOnlyItsCreationAndItsClose(t *testing.T) {
	h := Start(t)
	phone := h.Phone()
	term := h.SpawnReady("grok")
	id := phone.WaitFor("the grok's agent", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.TerminalID == term && !m.Agent.Attached
	}).Agent.ID
	h.Ops().Send(ops.Msg{Op: "close", ID: term})
	phone.WaitStatus(id, "closed")
	store, err := orch.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	var kinds string
	var rows int
	h.eventually("its close", func() bool {
		store.DB().QueryRow(`SELECT group_concat(kind) FROM (SELECT kind FROM events WHERE session_id = ? ORDER BY seq)`, id).Scan(&kinds)
		return strings.HasSuffix(kinds, "session.closed")
	})
	store.DB().QueryRow(`SELECT (SELECT COUNT(*) FROM turns WHERE session_id = ?) + (SELECT COUNT(*) FROM items WHERE session_id = ?)`, id, id).Scan(&rows)
	if kinds != "session.created,session.closed" || rows != 0 {
		t.Fatalf("events %s, %d turn and item rows", kinds, rows)
	}
}
