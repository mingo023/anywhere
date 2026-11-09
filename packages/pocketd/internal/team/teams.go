package team

import (
	"cmp"
	"context"
	"errors"
	"log"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/names"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

const (
	// reads is how many times a turn's end is looked for: the transcript can reach the timeline after the status.
	reads = 5
	// turnItems bounds the items a turn is read from.
	turnItems = 500
)

var (
	errOff = errors.New("Teams are off")
	// ErrBusy refuses a team prompt while the lead's team has workers at work or work to merge.
	ErrBusy = errors.New("This session's team is still running or waiting to merge")
)

// Teams is the one engine behind every lead's team. change is the only writer: it keeps each team in Store and
// tells every client of each change. Hooks only queue what agents did; Run handles it in order. A nil Teams
// refuses every verb.
type Teams struct {
	Store  *Store
	Agents *agent.Registry
	Hub    *hub.Hub
	// Poll is the wait between reads of a turn whose end isn't in the timeline yet; zero is a second.
	Poll time.Duration
	// Mail queues text for an agent's chat (daemon.Mail): handbacks and notes to a lead.
	Mail func(agentID, text string) error
	// Launch starts a worker as the owner and returns its agent id and folder.
	Launch func(lead, task string, s proto.LaunchSpec) (agentID, cwd string, err error)
	// Projects finds the project a lead's folder is in.
	Projects *registry.Registry
	// Names takes the display name each worker's Worktree gets from its task; nil names none.
	Names *names.Names

	// mu is held by change, so one team change runs at a time; it also guards since.
	mu sync.Mutex
	// since is where each lead's timeline stood when pocketd last prompted it, so only the answer to that prompt is read.
	since map[string]int64

	qmu   sync.Mutex
	queue []event
	wake  chan struct{}
}

// event is something an agent did, queued by a hook that runs under the agent's lock.
type event struct {
	kind     string // "status", "turn" or "told"
	agent    string
	from, to string // status: the change
	ok       bool   // turn: it ended normally
	text     string // told: what the user sent
}

// Prompt makes lead plan text as a team, in place of any team it had that isn't running, in review or merging.
func (t *Teams) Prompt(lead *agent.Agent, text string) error {
	if t == nil {
		return errOff
	}
	var err error
	t.change(lead.ID(), func(team proto.Team) (proto.Team, bool) {
		if busy(team.State) {
			err = ErrBusy
			return team, false
		}
		if err = t.send(lead, Wrap(text)); err != nil {
			return proto.Team{Lead: lead.ID(), State: "failed", Error: err.Error()}, true
		}
		return proto.Team{Lead: lead.ID(), State: "planning"}, true
	})
	return err
}

// busy is whether a team in state has workers at work or work to merge.
func busy(state string) bool { return state == "running" || state == "review" || state == "merging" }

// List is every lead's team.
func (t *Teams) List() ([]proto.Team, error) {
	if t == nil {
		return nil, errOff
	}
	return t.Store.List()
}

// Status is Registry.OnStatus's. It runs under the agent's lock, so it only queues the change for Run.
func (t *Teams) Status(id, from, to string) {
	if t != nil {
		t.post(event{kind: "status", agent: id, from: from, to: to})
	}
}

func (t *Teams) post(e event) {
	t.qmu.Lock()
	t.queue = append(t.queue, e)
	t.qmu.Unlock()
	select {
	case t.wakeup() <- struct{}{}:
	default:
	}
}

// wakeup is the channel post wakes Run with.
func (t *Teams) wakeup() chan struct{} {
	t.qmu.Lock()
	defer t.qmu.Unlock()
	if t.wake == nil {
		t.wake = make(chan struct{}, 1)
	}
	return t.wake
}

// Run handles what agents did, in the order they did it, until ctx ends.
func (t *Teams) Run(ctx context.Context) {
	if t == nil {
		return
	}
	wake := t.wakeup()
	for {
		select {
		case <-ctx.Done():
			return
		case <-wake:
		}
		t.qmu.Lock()
		queue := t.queue
		t.queue = nil
		t.qmu.Unlock()
		for _, e := range queue {
			// Shutting down closes every session; those closes aren't workers failing.
			if ctx.Err() != nil {
				return
			}
			t.handle(e)
		}
	}
}

func (t *Teams) handle(e event) {
	switch {
	case e.kind == "status" && e.to == "closed":
		t.closed(e.agent)
	case e.kind == "status" && turnEnd(e.from, e.to):
		t.Ended(e.agent)
	case e.kind == "turn" && e.ok && t.review(e.agent):
		return
	}
	t.work(e)
}

// turnEnd is whether a status change ends a turn: working or asking, then done (unseen) or idle.
func turnEnd(from, to string) bool {
	return (from == "working" || from == "needsYou") && (to == "done" || to == "idle")
}

// closed fails lead's team when the lead closed while it planned.
func (t *Teams) closed(lead string) {
	t.change(lead, func(team proto.Team) (proto.Team, bool) {
		if team.State != "planning" {
			return team, false
		}
		team.State, team.Error = "failed", "the lead closed while planning"
		return team, true
	})
}

// Ended moves lead's planning team on from the turn that just ended.
func (t *Teams) Ended(lead string) {
	t.mu.Lock()
	team, ok, err := t.Store.Get(lead)
	since := t.since[lead]
	t.mu.Unlock()
	if err != nil || !ok || team.State != "planning" {
		return
	}
	a, err := t.Agents.Get(lead)
	if err != nil {
		return
	}
	text, failed := t.read(a, since)
	t.change(lead, func(team proto.Team) (proto.Team, bool) {
		// A prompt sent while this turn was read starts a turn of its own.
		if team.State != "planning" || t.since[lead] != since {
			return team, false
		}
		next, repair := Planned(team, text, failed)
		if repair != "" {
			if err := t.send(a, repair); err != nil {
				next.State, next.Error = "failed", err.Error()
			}
		}
		return next, true
	})
}

// read is the text of a's turn after since, and whether it failed, looking again while its end hasn't reached the
// timeline. A turn whose end never shows is taken as it stands.
func (t *Teams) read(a *agent.Agent, since int64) (string, bool) {
	var text string
	for i := range reads {
		if i > 0 {
			time.Sleep(cmp.Or(t.Poll, time.Second))
		}
		items, _ := a.Timeline.Page(since, turnItems)
		var ended, failed bool
		if text, ended, failed = Reply(items); ended {
			return text, failed
		}
	}
	return text, a.Summary().Failed
}

// send prompts lead with text, noting where its timeline stood. It runs inside change, under mu.
func (t *Teams) send(lead *agent.Agent, text string) error {
	_, seq := lead.Timeline.State()
	if t.since == nil {
		t.since = map[string]int64{}
	}
	t.since[lead.ID()] = seq
	return lead.Driver().Prompt(text)
}

// change applies f to lead's team (one with only Lead set when it has none) under mu and, when f says it changed
// it, saves it and tells every client. It is the only writer of teams; f re-checks the state it is given.
func (t *Teams) change(lead string, f func(proto.Team) (proto.Team, bool)) (proto.Team, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	team, ok, err := t.Store.Get(lead)
	if err != nil {
		log.Printf("teams: %s: %v", lead, err)
		return team, false
	}
	if !ok {
		team = proto.Team{Lead: lead}
	}
	next, changed := f(team)
	if !changed {
		return team, false
	}
	next.UpdatedAt = time.Now().UnixMilli()
	if err := t.Store.Put(next); err != nil {
		log.Printf("teams: %s: %v", lead, err)
		return team, false
	}
	t.Hub.Publish(proto.NewTeamUpdate(next))
	t.awaits(lead, next.State)
	return next, true
}

// awaits shows lead as needing you while its team is proposed, and stops in any other state but planning: a
// re-plan's own turn ends the wait (Agent.Working), as clearing it first would read as a turn end.
func (t *Teams) awaits(lead, state string) {
	if state == "planning" {
		return
	}
	if a, err := t.Agents.Get(lead); err == nil {
		a.SetAwaiting(state == "proposed")
	}
}
