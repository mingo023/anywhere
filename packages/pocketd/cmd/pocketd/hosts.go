package main

import (
	"context"
	"errors"
	"log"

	"pocketd/internal/agent"
	"pocketd/internal/config"
	"pocketd/internal/hosts"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// openHosts links the hosts saved in home's state/hosts.json, first taking the desktop's on first start, until ctx
// ends, keeping what runs on them in mirror; nil serves none.
func openHosts(ctx context.Context, home string, env func() []string, name string, mirror hosts.Mirror) *hosts.Manager {
	if err := hosts.ImportDesktop(home); err != nil {
		log.Printf("hosts: %v; not imported", err)
	}
	m, err := hosts.Open(hosts.Paths{Home: home, RemoteDir: config.HomeName(), LegacyDir: config.LegacyHomeName()}, env, versionString(), name)
	if err != nil {
		log.Printf("hosts: %v; serving none", err)
		return nil
	}
	m.Mirror = mirror
	m.Start(ctx)
	return m
}

// mirror keeps the sessions this Mac started on its hosts in the orchestrator's store, as its sink keeps local ones.
type mirror struct{ o *orchestrator }

func (x mirror) Summary(s proto.AgentSummary)                    { x.o.sink.Summary(s) }
func (x mirror) Item(session string, epoch int64, it proto.Item) { x.o.sink.Item(session, epoch, it) }

func (x mirror) Asked(req proto.PermissionRequest) { x.o.ask(req) }

func (x mirror) Answered(session, request string) { x.o.engine.AskClosed(session, request) }

func (x mirror) Removed(host, name string) { x.o.engine.FailHost(host, name) }

func (x mirror) Linked(host string) { x.o.engine.Relink(host) }

// Gone settles the task of a session its host lost before its closing does, so the parent hears why.
func (x mirror) Gone(session, name string) {
	ts, err := x.o.store.Delegations(`child_id = ? AND status IN ('starting', 'working')`, session)
	if err != nil {
		log.Printf("hosts: tasks of %s: %v", session, err)
	}
	for _, t := range ts {
		// Stopped is what wakes the parent for a cancelled task.
		p := orch.TaskPayload{TaskID: t.TaskID, Status: orch.TaskCancelled, Error: "Anywhere on " + name + " stopped during the task, and the session ended with it", Stopped: true}
		if _, err := x.o.engine.Dispatch(orch.NewCommand("", orch.CmdDelegateSettle, t.ParentID, p)); err != nil {
			log.Printf("hosts: settle %s: %v", t.TaskID, err)
		}
	}
}

func (x mirror) Live(host string) []proto.AgentSummary {
	rows, err := x.o.store.Sessions(`id LIKE ? AND closed_at = 0`, host+"/%")
	if err != nil {
		log.Printf("hosts: sessions on %s: %v", host, err)
	}
	var out []proto.AgentSummary
	for _, s := range rows {
		out = append(out, proto.AgentSummary{ID: s.ID, Title: s.Title, Cwd: s.Cwd, Provider: s.Agent, Model: s.Model, Effort: s.Effort, Access: s.Access, Status: s.Status,
			Pinned: s.Pinned, ProviderSessionID: s.Conversation, CreatedAt: s.CreatedAt, UpdatedAt: s.UpdatedAt, Project: s.Project, Worktree: s.Worktree, Branch: s.Branch,
			Origin: s.Origin, ChatOnly: s.Chat})
	}
	return out
}

func (x mirror) Open(session string) []string {
	rs, err := x.o.store.Requests(session, "open")
	if err != nil {
		log.Printf("hosts: asks of %s: %v", session, err)
	}
	var ids []string
	for _, r := range rs {
		ids = append(ids, r.ID)
	}
	return ids
}

// environments is the linked hosts as the MCP tools see them.
type environments struct {
	m   *hosts.Manager
	reg *agent.Registry
}

func (e environments) List() []mcp.Environment {
	var out []mcp.Environment
	for _, h := range e.m.List() {
		env := mcp.Environment{ID: h.ID, Label: h.Name, State: string(h.Link.State)}
		if h.Link.Failure != "" {
			env.Error = h.Link.Failure.Message(h.Name)
		}
		out = append(out, env)
	}
	return out
}

func (e environments) Projects(ctx context.Context, id string) ([]proto.Project, error) {
	return e.m.Projects(ctx, id)
}

func (e environments) Remove(id string) error {
	err := e.m.Remove(id, false)
	if errors.Is(err, hosts.ErrUnknown) {
		return &orch.Failure{Code: orch.CodeNotFound, Message: "No linked host " + id}
	}
	return err
}

func (e environments) Card(session string, l proto.HostLink) error {
	a, err := e.reg.Get(session)
	if err != nil {
		return err
	}
	a.Record(timeline.Event{Kind: "host_link", HostLink: &l})
	return nil
}
