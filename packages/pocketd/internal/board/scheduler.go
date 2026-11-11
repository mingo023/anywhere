package board

import (
	"cmp"
	"context"
	"log"
	"sync/atomic"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/terminal"
)

const (
	defaultPoll = time.Second
	sweepEvery  = 5 * time.Second
	// mergedEvery is how often gh is asked about a tree nobody watches.
	mergedEvery = 5 * time.Minute
	// reattachWait is how long after a restart an agent may be missing while its session comes back.
	reattachWait = 30 * time.Second
	maxWhy       = 80
)

// Seen is what a ticket's agent looks like now.
type Seen struct {
	Status string // idle, working, needsYou, done or closed
	Failed bool
	// Answered: the transcript holds a reply to the prompt. A fresh agent is idle
	// before its first turn, and a turn shorter than a poll is never seen working.
	Answered bool
}

// Starter launches a ticket's session; requestID is new for every launch.
// created is told its Worktree as soon as it exists.
type Starter func(requestID string, t proto.Ticket, created func(cwd string)) launch.Result

// Watcher looks at an agent; false when there is no such agent. starting says the transcript matters.
type Watcher func(agentID string, starting bool) (Seen, bool)

type Scheduler struct {
	Store *Store
	Now   func() time.Time
	// Poll is how often the agents are looked at; zero means 1s.
	Poll  time.Duration
	Start Starter
	Watch Watcher
	// Merged says tree's PR is merged; nil never closes a ticket for it.
	Merged func(ctx context.Context, tree string) bool

	asking atomic.Bool
}

// Spec is the session t starts: in the Worktree it already has, else in a
// fresh one off its base that pocketd names from the prompt. It is a chat for
// an agent whose work is followed only there.
func Spec(t proto.Ticket, requestID string) proto.LaunchSpec {
	checkout := proto.Checkout{New: &proto.NewWorktree{Name: "ticket-" + requestID[:min(8, len(requestID))], Base: t.Base, AutoName: true}}
	if t.Worktree != "" {
		checkout = proto.Checkout{Worktree: t.Worktree}
	}
	return proto.LaunchSpec{Project: t.Folder, Checkout: checkout, Provider: t.Provider, Access: "settings", Prompt: t.Prompt, Chat: providers.Get(t.Provider).OnlyChats()}
}

// AgentWatcher looks at agents in reg, reading a transcript only while a ticket starts.
func AgentWatcher(reg *agent.Registry) Watcher {
	return func(id string, starting bool) (Seen, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return Seen{}, false
		}
		sum := a.Summary()
		seen := Seen{Status: sum.Status, Failed: sum.Failed}
		if starting && (sum.Status == "done" || sum.Status == "idle") {
			items, _ := a.Timeline.Page(0, 500)
			seen.Answered = automation.Answered(items)
		}
		return seen, true
	}
}

// Step is t after looking at its agent, and whether that changed it.
func Step(t proto.Ticket, seen Seen, present bool) (proto.Ticket, bool) {
	next := t
	switch {
	case !present || seen.Status == "closed":
		next.Status, next.Why = proto.TicketStopped, "Session closed"
	case seen.Status == "working":
		next.Status, next.Why = proto.TicketWorking, ""
	case seen.Status == "needsYou":
		next.Status, next.Why = proto.TicketNeedsYou, ""
	case seen.Status != "done" && seen.Status != "idle":
	case seen.Failed:
		next.Status, next.Why = proto.TicketFailed, "The agent failed"
	case seen.Status == "done":
		next.Status, next.Why = proto.TicketDone, ""
	case t.Status != proto.TicketStarting || seen.Answered:
		next.Status, next.Why = proto.TicketIdle, ""
	}
	return next, next.Status != t.Status || next.Why != t.Why
}

// Run keeps the Board moving until ctx ends: queued tickets start as slots
// free, and live ones follow their agents.
func (s *Scheduler) Run(ctx context.Context) {
	began := s.Now()
	if err := s.Store.Reconcile(began); err != nil {
		log.Printf("board: %v", err)
	}
	tick := time.NewTicker(cmp.Or(s.Poll, defaultPoll))
	defer tick.Stop()
	var swept, asked time.Time
	for {
		changed := s.Store.Changed()
		now := s.Now()
		for _, t := range s.Store.Claim(now) {
			go s.launch(t)
		}
		s.observe(now.Sub(began) < reattachWait)
		if now.Sub(swept) >= sweepEvery {
			swept = now
			s.sweep(now)
		}
		if s.Merged != nil && now.Sub(asked) >= mergedEvery {
			asked = now
			go s.closeMerged(ctx)
		}
		select {
		case <-ctx.Done():
			return
		case <-changed:
		case <-tick.C:
		}
	}
}

func (s *Scheduler) launch(t proto.Ticket) {
	// Recorded before the launch ends, so a restart can't orphan a Worktree a Start again wouldn't find.
	res := s.Start(terminal.NewID(), t, func(cwd string) {
		if err := s.Store.Update(t.ID, func(x *proto.Ticket) { x.Worktree = cwd }); err != nil {
			log.Printf("board: ticket %s: %v", t.ID, err)
		}
	})
	err := s.Store.Update(t.ID, func(x *proto.Ticket) {
		x.ChangedAt = s.Now().UnixMilli()
		if res.Cwd != "" {
			x.Worktree = res.Cwd
		}
		if res.Err != nil {
			x.Status, x.Why = proto.TicketFailed, automation.Plain(res.Err.Message, maxWhy)
			return
		}
		x.AgentID, x.TerminalID = res.AgentID, res.TerminalID
	})
	if err != nil {
		log.Printf("board: ticket %s: %v", t.ID, err)
	}
}

// observe moves live tickets with their agents, a Stopped one too since its
// session can be restored. Right after a restart a missing agent may still be
// coming back, so it is left alone.
func (s *Scheduler) observe(reattaching bool) {
	tickets, _ := s.Store.Snapshot()
	for _, t := range tickets {
		if t.State != proto.TicketLive || t.AgentID == "" {
			continue
		}
		seen, present := s.Watch(t.AgentID, t.Status == proto.TicketStarting)
		if !present && reattaching {
			continue
		}
		next, changed := Step(t, seen, present)
		if !changed {
			continue
		}
		err := s.Store.Update(t.ID, func(x *proto.Ticket) {
			x.Status, x.Why, x.ChangedAt = next.Status, next.Why, s.Now().UnixMilli()
		})
		if err != nil {
			log.Printf("board: ticket %s: %v", t.ID, err)
		}
	}
}

// sweep notices Worktrees deleted outside the Board: a live ticket closes, a
// closed one stops offering what is gone.
func (s *Scheduler) sweep(now time.Time) {
	tickets, _ := s.Store.Snapshot()
	for _, t := range tickets {
		if t.Worktree == "" || t.HoldsSlot() {
			continue
		}
		if !gone(t.Worktree) {
			continue
		}
		err := s.Store.Update(t.ID, func(x *proto.Ticket) {
			x.Worktree = ""
			if x.State == proto.TicketLive {
				x.State, x.ClosedAt = proto.TicketClosed, now.UnixMilli()
			}
			if x.ClosedWhy != proto.ClosedMerged {
				x.ClosedWhy = proto.ClosedDeleted
			}
		})
		if err != nil {
			log.Printf("board: ticket %s: %v", t.ID, err)
		}
	}
}

func (s *Scheduler) closeMerged(ctx context.Context) {
	if !s.asking.CompareAndSwap(false, true) {
		return
	}
	defer s.asking.Store(false)
	tickets, _ := s.Store.Snapshot()
	for _, t := range tickets {
		review := t.Status == proto.TicketDone || t.Status == proto.TicketIdle || t.Status == proto.TicketStopped
		if t.State != proto.TicketLive || !review || t.Worktree == "" || !s.Merged(ctx, t.Worktree) {
			continue
		}
		if err := s.Store.Close(t.ID, proto.ClosedMerged, s.Now()); err != nil {
			log.Printf("board: ticket %s: %v", t.ID, err)
		}
	}
}
