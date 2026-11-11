package daemon

import (
	"context"
	"errors"

	"pocketd/internal/agent"
	"pocketd/internal/pi"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
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
	x.s = pi.NewSession(c.a.ID(), x.pr.provider, piSink{chatSink{x.d, x.pr, c.a}}, x.d.Broker)
	return piDriver{c, x}
}

// attach reads the session pi is on: its command line named the one a
// resumed agent had, and a handed-down agent never left its own.
func (x *piChat) attach() {
	pr := x.pr
	if x.d.hintOf(pr) != nil {
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

// piSink is a chatSink for pi, which tells its status only as the events of
// its runs, which the session reads.
type piSink struct{ chatSink }

func (s piSink) SetEffort(effort string) { s.a.SetEffort(effort) }
func (s piSink) Compacting()             { s.a.SetCompacting() }
func (s piSink) Compacted()              { s.a.Compacted() }

type piDriver struct {
	termDriver
	x *piChat
}

func (c piDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.x.s.Prompt(text)
}

// Interrupt drops the prompts sent mid-turn with the turn; the chat puts them back in its queue.
func (c piDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	if !c.a.Stop() {
		return nil
	}
	c.x.s.Cancel()
	c.x.d.cleared(c.pr)
	return nil
}

func (c piDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.x.s.Compact()
}

// Settings applies to the session's next turns; a resume keeps it, as the
// switch is saved with the Terminal.
func (c piDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := c.x.s.Settings(model, effort); err != nil {
		return err
	}
	c.pr.switched(model, effort)
	return nil
}

// Access has nothing to switch: pi asks before no tool, so it is always full.
func (c piDriver) Access(access string) error {
	if access == "full" {
		return nil
	}
	return errors.New(providers.Get(c.pr.provider).Name + " runs every tool without asking, so its access is always full")
}

func (c piDriver) Commands() ([]proto.Command, error) {
	if !c.inForeground() {
		return nil, errNotForeground
	}
	return c.x.s.Commands()
}
