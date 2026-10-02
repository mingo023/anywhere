package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/config"
	"pocketd/internal/launch"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

func launchWho(p peer.Principal) launch.Who {
	switch p.Kind {
	case peer.Owner:
		return launch.Who{Owner: true, Key: "owner"}
	case peer.Device:
		return launch.Who{Key: "device:" + p.Device}
	}
	return launch.Who{Key: "pty:" + p.Terminal}
}

// create reads the conn's principal and caps before going off the read loop,
// which a second hello rewrites. The create itself can wait minutes on setup.
func (c *conn) create(m proto.ClientMessage) {
	w := launchWho(c.who)
	if !w.Owner && !slices.Contains(c.caps, proto.CapLaunch) {
		c.send(proto.NewCodedError(m.ID, "access_not_allowed", "Update Pocket on your Mac.", ""))
		return
	}
	go func() {
		r := c.s.Launch.Create(w, m.RequestID, *m.Spec, func(step, note string) {
			c.send(proto.NewAgentProgress(m.ID, m.RequestID, step, note))
		}, func(cr launch.Creating) {
			c.send(proto.NewAgentCreating(m.ID, m.RequestID, cr.Terminal, cr.Cwd, cr.Setup))
		})
		if r.Err != nil {
			c.send(proto.NewCodedError(m.ID, r.Err.Code, r.Err.Message, r.Err.Detail))
			return
		}
		c.send(proto.NewAgentCreated(m.ID, m.RequestID, r.AgentID, r.TerminalID))
	}()
}

func (c *conn) providers(m proto.ClientMessage) {
	list, maxAccess, phoneMax := c.s.Launch.Providers(launchWho(c.who))
	c.send(proto.NewAgentProviders(m.ID, list, maxAccess, phoneMax))
}

func (c *conn) configSet(m proto.ClientMessage) {
	err := c.s.Launch.SetPhoneMaxAccess(m.Value)
	switch {
	case errors.Is(err, config.ErrInvalid):
		c.send(proto.NewCodedError(m.ID, "invalid_config", err.Error(), ""))
	case err != nil:
		c.send(proto.NewError(m.ID, err.Error()))
	default:
		c.send(proto.NewAck(m.ID))
	}
}
