package daemon

import (
	"context"
	"errors"

	"pocketd/internal/acp"
	"pocketd/internal/agent"
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
	x.s = acp.NewSession(c.a.ID(), x.pr.provider, chatSink{x.d, x.pr, c.a}, x.d.Broker)
	return acpDriver{pipeDriver{c, x.d, x.s}, x}
}

// attach opens the session: the one a resumed or handed-down agent had, else a new one.
func (x *acpChat) attach() {
	pr := x.pr
	saved, left := "", acp.Ended
	if h := x.d.hintOf(pr); h != nil {
		saved = h.saved.ConversationID
		if h.handoff {
			x.s.Carried()
		}
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
	x.s.Server(pr.t.Info().Args)
	x.d.openChat(pr, func() ([]string, error) {
		return x.s.Open(x, pr.t.Info().Cwd, saved, launch, left)
	})
}

// acpDriver drives an agent through its ACP session.
type acpDriver struct {
	pipeDriver
	x *acpChat
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
