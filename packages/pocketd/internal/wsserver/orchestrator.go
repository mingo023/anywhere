package wsserver

import (
	"errors"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

// promptWait bounds how long agent.prompt waits for the agent to take a prompt.
const promptWait = 30 * time.Second

// QueueEntries is a session's live queue as clients see it.
func QueueEntries(e *orch.Engine, session string) []proto.QueueEntry {
	entries, _ := e.Store().Queue(session, true)
	out := make([]proto.QueueEntry, 0, len(entries))
	for _, q := range entries {
		out = append(out, proto.QueueEntry{ID: q.ID, Text: q.Text, Origin: q.Origin, State: q.State, Order: q.Ord})
	}
	return out
}

// Delegations is what a session delegated, with each open question's text.
func Delegations(e *orch.Engine, parent string) []proto.Delegation {
	ts, _ := e.Store().Delegations(`parent_id = ? ORDER BY created_at`, parent)
	out := make([]proto.Delegation, 0, len(ts))
	for _, t := range ts {
		out = append(out, WireDelegation(e, t))
	}
	return out
}

func WireDelegation(e *orch.Engine, t orch.Delegation) proto.Delegation {
	d := proto.Delegation{TaskID: t.TaskID, ParentID: t.ParentID, ChildID: t.ChildID, Title: t.Title, Agent: t.Agent, Model: t.Model, Status: t.Status,
		Summary: t.Summary, Error: t.Error, CreatedAt: t.CreatedAt, SettledAt: t.SettledAt}
	if t.PendingQuestion != "" && t.ChildID != "" {
		open, _ := e.Store().Requests(t.ChildID, "open")
		for _, r := range open {
			if r.ID == t.PendingQuestion {
				d.Question = orch.QuestionText(r.Payload)
			}
		}
	}
	return d
}

func failureReply(err error) error {
	var f *orch.Failure
	if errors.As(err, &f) {
		return errors.New(f.Message)
	}
	return err
}

// orchestrated answers the verbs the orchestrator owns; false leaves m to dispatch.
func (c *conn) orchestrated(m proto.ClientMessage) (bool, error) {
	e := c.s.Orch
	if e == nil {
		return false, nil
	}
	if _, err := c.s.Agents.Get(m.AgentID); err != nil && m.Type != "agent.delegations" && m.Type != "queue.list" {
		return false, nil
	}
	switch m.Type {
	case "agent.prompt":
		mode := "steer"
		if m.Queue {
			mode = "auto"
		}
		_, err := e.DispatchWait(orch.NewCommand("", orch.CmdMessageSend, m.AgentID, orch.SendPayload{Text: m.Text, Mode: mode, Origin: orch.OriginUser}), promptWait)
		return true, failureReply(err)
	case "agent.interrupt":
		_, err := e.DispatchWait(orch.Command{Kind: orch.CmdSessionStop, Session: m.AgentID}, promptWait)
		return true, failureReply(err)
	case "agent.delegations":
		c.send(proto.NewAgentDelegations(m.ID, m.AgentID, Delegations(e, m.AgentID)))
		return true, nil
	case "queue.list":
		c.send(proto.NewQueueList(m.ID, m.AgentID, QueueEntries(e, m.AgentID)))
		return true, nil
	case "queue.edit", "queue.cancel", "queue.reorder", "queue.send_now":
		kind := map[string]string{"queue.edit": orch.CmdQueueEdit, "queue.cancel": orch.CmdQueueCancel, "queue.reorder": orch.CmdQueueReorder, "queue.send_now": orch.CmdQueueSendNow}[m.Type]
		_, err := e.DispatchWait(orch.NewCommand("", kind, m.AgentID, orch.EntryPayload{EntryID: m.EntryID, Text: m.Text, Before: m.Before}), promptWait)
		return true, failureReply(err)
	}
	return false, nil
}
