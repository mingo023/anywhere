package daemon

import (
	"context"
	"errors"

	"pocketd/internal/agent"
	"pocketd/internal/pi"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

// piChat drives a pi or omp run with `--mode rpc` on its stdio, which
// `pocketd pipe` carries.
type piChat struct {
	*pipeWire
	d  *Daemon
	pr *presence
	s  *pi.Session
}

// newPiChat is the chat of a pi or omp run with `--mode rpc` behind pr's pipe.
// pr.launch holds the plan, model and effort it was launched with.
func newPiChat(d *Daemon, pr *presence) pipeChat {
	return &piChat{pipeWire: newPipeWire(pr), d: d, pr: pr}
}

// driver makes the session too: it is the first to know the agent.
func (x *piChat) driver(c termDriver) agent.Driver {
	x.s = pi.NewSession(c.a.ID(), x.pr.provider, chatSink{x.d, x.pr, c.a}, x.d.Broker)
	return piDriver{pipeDriver{c, x.d, x.s}, x}
}

// attach reads the session pi is on: its command line named the one a
// resumed agent had, and a handed-down agent never left its own.
func (x *piChat) attach() {
	pr := x.pr
	h := x.d.hintOf(pr)
	x.s.Shown = h != nil && h.moved
	if h != nil && !h.moved {
		// The session reads pi's history, so the timeline starts over.
		pr.a.ClearTimeline()
	}
	pr.mu.Lock()
	launch := pr.launch
	pr.mu.Unlock()
	context.AfterFunc(pr.ctx, x.s.Close)
	x.d.openChat(pr, func() ([]string, error) {
		return x.s.Open(x, launch, midTurn(pr.a.Turn()))
	})
}

type piDriver struct {
	pipeDriver
	x *piChat
}

// Access to full leaves plan, which is the read-only tools pi started with;
// there is no other access to switch to, as pi asks before no tool.
func (c piDriver) Access(access string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	name := providers.Get(c.pr.provider).Name
	if access != "full" {
		return errors.New(name + " runs every tool without asking, so its access is always full")
	}
	c.pr.mu.Lock()
	planning := c.pr.launch.Plan
	c.pr.mu.Unlock()
	if !planning {
		return nil
	}
	return c.x.leavePlan(name)
}

// leavePlan restarts pi on the same session without its read-only tools: pi
// takes its tools only on its command line.
func (x *piChat) leavePlan(name string) error {
	switch {
	case x.d.Resume == nil:
		return errors.New(name + " leaves plan by resuming its session, which Settings has off")
	case !restartable(between(x.pr.a)):
		return errors.New(name + " can leave plan once its turn ends")
	}
	switch saved, err := x.s.Saved(); {
	case err != nil:
		return err
	case !saved:
		return errors.New(name + " has no session to resume yet, so start a new session to build")
	case !x.d.restart(x.pr, func(e *state.Terminal) { e.Launch.Plan = false }, restartable, ""):
		return errors.New(name + " couldn't leave plan")
	}
	return nil
}
