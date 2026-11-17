package wsserver

import (
	"cmp"
	"encoding/json"
	"errors"
	"log"
	"slices"
	"time"

	"pocketd/internal/events"
	"pocketd/internal/launch"
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
		Summary: t.Summary, Error: t.Error, EnvironmentID: t.Environment, CreatedAt: t.CreatedAt, SettledAt: t.SettledAt}
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

// createWait outlasts the launcher's own bound on a create.
const createWait = 11 * time.Minute

// started runs agent.create as a session.start command named by the
// requester and its request id, so a retry gets the first create's outcome,
// and calls reply with the outcome at least once. The asker hears it from the
// launcher, before the effect is recorded, as a chat's first turn may end
// within milliseconds of it.
func (s *Server) started(w launch.Who, requestID string, spec proto.LaunchSpec, progress func(step, note string), creating func(launch.Creating), reply func(launch.Result)) {
	id := "create:" + w.Key + ":" + requestID
	defer s.Launch.Watch(id, progress, creating, reply)()
	r, err := s.Orch.Dispatch(orch.NewCommand(id, orch.CmdSessionStart, "", orch.StartPayload{Owner: w.Owner, Key: w.Key, Spec: spec}))
	if err != nil {
		if f := (*orch.Failure)(nil); errors.As(err, &f) && f.Code == orch.CodeInvalid {
			reply(launch.Result{Err: &launch.Failure{Code: "duplicate", Message: "This request was already sent with a different spec"}})
			return
		}
		reply(launch.Result{Err: &launch.Failure{Code: "spawn_failed", Message: err.Error()}})
		return
	}
	reply(s.outcome(r))
}

// outcome is the create r's effect recorded, once it ends.
func (s *Server) outcome(r orch.Receipt) launch.Result {
	waited := s.Orch.Wait(r.Effects, createWait)
	res, _, last := s.Orch.Result(r.Effects[0])
	var out launch.Result
	b, _ := json.Marshal(res)
	json.Unmarshal(b, &out)
	if waited != nil && out.Err == nil {
		out.Err = &launch.Failure{Code: "spawn_failed", Message: cmp.Or(last, waited.Error())}
	}
	return out
}

func failureReply(err error) error {
	var f *orch.Failure
	if errors.As(err, &f) {
		return errors.New(f.Message)
	}
	return err
}

func (c *conn) resolve(e *orch.Engine, m proto.ClientMessage) error {
	agentID := c.asker(m.RequestID)
	gone := errors.New("Permission request is no longer open")
	if agentID == "" {
		return gone
	}
	p := orch.RequestPayload{RequestID: m.RequestID, Decision: m.Decision, Option: m.Option, Message: m.Message, Answers: m.Answers, User: true}
	if _, err := e.DispatchWait(orch.NewCommand("", orch.CmdRequestAnswer, agentID, p), promptWait); err != nil {
		if orch.AsFailure(err).Code == orch.CodeNotFound {
			return gone
		}
		return failureReply(err)
	}
	c.s.Events.Emit(events.Event{Kind: "answer", Agent: agentID, Principal: c.principal(), Decision: m.Decision})
	return nil
}

// orchestrated answers the verbs the orchestrator owns; false leaves m to dispatch.
func (c *conn) orchestrated(m proto.ClientMessage) (bool, error) {
	e := c.s.Orch
	if e == nil {
		return false, nil
	}
	if m.Type == "permission.resolve" {
		return true, c.resolve(e, m)
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
	case "agent.send_now":
		_, err := e.DispatchWait(orch.Command{Kind: orch.CmdTurnSendNow, Session: m.AgentID}, promptWait)
		return true, failureReply(err)
	case "agent.settings":
		p := orch.SettingsPayload{Model: m.Model, Effort: m.Effort, Options: m.Options, Access: m.Access}
		_, err := e.DispatchWait(orch.NewCommand("", orch.CmdSessionSettings, m.AgentID, p), promptWait)
		return true, failureReply(err)
	case "agent.close":
		_, err := e.DispatchWait(orch.Command{Kind: orch.CmdSessionClose, Session: m.AgentID}, promptWait)
		return true, failureReply(err)
	case "agent.pin":
		action := "unpin"
		if m.Pinned {
			action = "pin"
		}
		_, err := e.DispatchWait(orch.NewCommand("", orch.CmdSessionOrganize, m.AgentID, orch.OrganizePayload{Action: action}), promptWait)
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
