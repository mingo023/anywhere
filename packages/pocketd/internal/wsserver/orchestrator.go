package wsserver

import (
	"cmp"
	"errors"
	"log"
	"slices"
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

// agents is agent.list: each open session the store keeps, as the registry
// has it live where it runs, and those running the store hasn't written yet.
func (s *Server) agents() []proto.AgentSummary {
	live := s.Agents.List()
	if s.OpenSessions == nil {
		return live
	}
	stored, err := s.OpenSessions()
	if err != nil {
		log.Printf("agent.list: %v", err)
		return live
	}
	return listed(stored, live)
}

func listed(stored []orch.Session, live []proto.AgentSummary) []proto.AgentSummary {
	byID := map[string]proto.AgentSummary{}
	for _, a := range live {
		byID[a.ID] = a
	}
	out := make([]proto.AgentSummary, 0, max(len(stored), len(live)))
	for _, x := range stored {
		if a, ok := byID[x.ID]; ok {
			out = append(out, a)
			delete(byID, x.ID)
			continue
		}
		out = append(out, proto.AgentSummary{ID: x.ID, TerminalID: x.Terminal, Title: x.Title, Cwd: x.Cwd, Provider: x.Agent, Model: x.Model, Effort: x.Effort, Access: x.Access,
			Status: x.Status, Failed: x.Failed, Pinned: x.Pinned, ProviderSessionID: x.Conversation, CreatedAt: x.CreatedAt, UpdatedAt: x.UpdatedAt, Project: x.Project,
			Worktree: x.Worktree, Branch: x.Branch, Origin: x.Origin, ChatOnly: x.Chat, Parent: x.ParentID, Relationship: x.Relationship, CreatedBy: x.CreatedBy})
	}
	for _, a := range live {
		if _, ok := byID[a.ID]; ok {
			out = append(out, a)
		}
	}
	slices.SortStableFunc(out, func(a, b proto.AgentSummary) int { return cmp.Compare(a.CreatedAt, b.CreatedAt) })
	return out
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
