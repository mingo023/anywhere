package daemon

import (
	"context"
	"errors"

	"pocketd/internal/acp"
	"pocketd/internal/agent"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

// acpChat drives an agent run as an ACP server on its stdio, which `pocketd
// pipe` carries.
type acpChat struct {
	*pipeWire
	d  *Daemon
	pr *presence
	s  *acp.Session
}

// newACPChat is the chat of an agent run as an ACP server behind pr's pipe.
// pr.launch holds the access, plan, model and effort it was launched with.
func newACPChat(d *Daemon, pr *presence) pipeChat {
	return &acpChat{pipeWire: newPipeWire(pr), d: d, pr: pr}
}

// driver makes the session too: it is the first to know the agent.
func (x *acpChat) driver(c termDriver) agent.Driver {
	x.s = acp.NewSession(c.a.ID(), x.pr.provider, acpSink{chatSink{x.d, x.pr, c.a}}, x.d.Broker)
	return acpDriver{c, x}
}

// attach opens the session: the one a resumed or handed-down agent had, else a new one.
func (x *acpChat) attach() {
	pr := x.pr
	saved, left := "", acp.Ended
	if h := x.d.hintOf(pr); h != nil {
		saved = h.saved.ConversationID
		switch {
		case midTurn(pr.a.Turn()):
			left = acp.Running
		case midTurn(h.saved.Status):
			left = acp.Cut
		}
	}
	pr.mu.Lock()
	launch := pr.launch
	pr.mu.Unlock()
	context.AfterFunc(pr.ctx, x.s.Close)
	x.d.openChat(pr, func() ([]string, error) {
		return x.s.Open(x, pr.t.Info().Cwd, saved, launch, left)
	})
}

// acpSink is a chatSink for ACP, which has no status of its own.
type acpSink struct{ chatSink }

// Loaded starts the timeline over: the agent replays the session's history into it.
func (s acpSink) Loaded() { s.a.ClearTimeline() }

func (s acpSink) Compacting() { s.a.SetCompacting() }
func (s acpSink) Compacted()  { s.a.Compacted() }

// acpDriver drives an agent through its ACP session.
type acpDriver struct {
	termDriver
	x *acpChat
}

func (c acpDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.x.s.Prompt(text)
}

// Interrupt drops the prompts sent mid-turn with the turn; the chat puts them back in its queue.
func (c acpDriver) Interrupt() error {
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

func (c acpDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.x.s.Compact()
}

// Settings applies to the session's next turns; a resume keeps it, as the
// switch is saved with the Terminal.
func (c acpDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := c.x.s.Settings(model, effort); err != nil {
		return err
	}
	c.pr.switched(model, effort)
	return nil
}

func (c acpDriver) Access(access string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if c.x.s.Respawns(access) {
		return c.x.respawn(access)
	}
	if err := c.x.s.Access(access); err != nil {
		return err
	}
	c.pr.accessSwitched(access)
	return nil
}

// respawn restarts the agent on the same session with access, out of plan.
func (x *acpChat) respawn(access string) error {
	name := providers.Get(x.pr.provider).Name
	switch {
	case x.d.Resume == nil:
		return errors.New(name + " switches plan and full access by resuming its session, which Settings has off")
	case !restartable(between(x.pr.a)):
		return errors.New(name + " can switch plan and full access once its turn ends")
	case !x.d.restart(x.pr, func(e *state.Terminal) { e.Launch.Access, e.Launch.Plan = access, false }, restartable, ""):
		return errors.New(name + " couldn't switch")
	}
	return nil
}

func (c acpDriver) Commands() ([]proto.Command, error) {
	if !c.inForeground() {
		return nil, errNotForeground
	}
	return c.x.s.Commands()
}
