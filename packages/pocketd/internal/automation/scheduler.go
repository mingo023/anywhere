package automation

import (
	"cmp"
	"context"
	"log"
	"sync/atomic"
	"time"

	"pocketd/internal/launch"
	"pocketd/internal/proto"
)

const (
	maxSummaryPolls = 3
	defaultTick     = 10 * time.Second
	defaultPoll     = time.Second
	// reattachWait is how long after a restart an agent may be missing while
	// its session comes back.
	reattachWait = 30 * time.Second
)

// Starter launches the Session of run runID; runID is the launch's request id.
type Starter func(runID string, a proto.Automation) launch.Result

// Watcher looks at an agent; false when there is no such agent.
type Watcher func(agentID string) (Seen, bool)

type Scheduler struct {
	Store *Store
	Now   func() time.Time
	// Tick is how often due automations are looked for, Poll how often a run's agent is.
	// Zero means 10s and 1s.
	Tick, Poll time.Duration
	Start      Starter
	Watch      Watcher

	stopped atomic.Bool
}

// Run starts what is due now, then at every Tick on the wall clock: a timer
// set for the next due time would stall while the Mac sleeps. It returns when
// ctx ends, and observers stop with it so their runs stay as they were.
func (s *Scheduler) Run(ctx context.Context) {
	tick := time.NewTicker(cmp.Or(s.Tick, defaultTick))
	defer tick.Stop()
	for {
		s.check()
		select {
		case <-ctx.Done():
			s.stopped.Store(true)
			return
		case <-tick.C:
		}
	}
}

func (s *Scheduler) check() {
	now := s.Now()
	autos, _ := s.Store.Snapshot()
	for _, a := range autos {
		if !a.Enabled || a.NextRunAt == 0 || a.NextRunAt > now.UnixMilli() {
			continue
		}
		if run, ok := s.Store.Claim(a.ID, time.UnixMilli(a.NextRunAt), now); ok && run.Status == "pending" {
			go s.launch(run, a)
		}
	}
}

// RunNow starts a run of the automation now, ahead of its schedule.
func (s *Scheduler) RunNow(id string) (proto.Run, error) {
	run, err := s.Store.Begin(id, s.Now())
	if err != nil {
		return run, err
	}
	a, ok := s.Store.Automation(id)
	if !ok {
		return run, ErrUnknown
	}
	go s.launch(run, a)
	return run, nil
}

// Resume watches again the runs a restart left going.
func (s *Scheduler) Resume() {
	_, runs := s.Store.Snapshot()
	for _, r := range runs {
		if (r.Status == "running" || r.Status == "waiting") && r.AgentID != "" {
			go s.observe(r.ID, true, reattachWait)
		}
	}
}

func (s *Scheduler) launch(run proto.Run, a proto.Automation) {
	res := s.Start(run.ID, a)
	err := s.Store.Update(run.ID, func(r *proto.Run) {
		if res.Err != nil {
			r.Status, r.Why, r.FinishedAt = "failed", plain(res.Err.Message, maxWhy), s.Now().UnixMilli()
			return
		}
		r.Status, r.AgentID, r.TerminalID = "running", res.AgentID, res.TerminalID
	})
	if err != nil {
		log.Printf("automations: run %s: %v", run.ID, err)
	}
	if res.Err == nil {
		s.observe(run.ID, false, 0)
	}
}

// observe follows a run's agent until the run finishes. A missing agent is
// waited for during absentWait before the run counts as closed.
func (s *Scheduler) observe(runID string, worked bool, absentWait time.Duration) {
	began := s.Now()
	summaryPolls := 0
	tick := time.NewTicker(cmp.Or(s.Poll, defaultPoll))
	defer tick.Stop()
	for ; ; <-tick.C {
		r, ok := s.Store.Run(runID)
		if !ok || finished(r.Status) || s.stopped.Load() {
			return
		}
		seen, present := s.Watch(r.AgentID)
		if !present && s.Now().Sub(began) < absentWait {
			continue
		}
		worked = worked || seen.Status == "working" || seen.Status == "needsYou"
		next, changed := Step(r, seen, present, worked)
		if !changed {
			continue
		}
		// The transcript is read after the turn ends, so its last text can lag the status.
		if next.Status == "succeeded" && next.Summary == "" && summaryPolls < maxSummaryPolls {
			summaryPolls++
			continue
		}
		if finished(next.Status) {
			next.FinishedAt = s.Now().UnixMilli()
		}
		err := s.Store.Update(runID, func(r *proto.Run) {
			r.Status, r.Why, r.Summary, r.FinishedAt = next.Status, next.Why, next.Summary, next.FinishedAt
		})
		if err != nil {
			log.Printf("automations: run %s: %v", runID, err)
		}
	}
}
