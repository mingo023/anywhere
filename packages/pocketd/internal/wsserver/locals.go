package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/locals"
	"pocketd/internal/proto"
)

// withLocals subscribes c to local.list when it speaks locals.v1.
func (c *conn) withLocals(caps []string) <-chan []byte {
	if c.s.Locals == nil || !slices.Contains(caps, proto.CapLocals) || c.stopLocals != nil {
		return nil
	}
	var msgs <-chan []byte
	msgs, c.stopLocals = c.s.Locals.Subscribe()
	return msgs
}

// local serves local.create, local.rename and local.delete; without the cap
// they are as unknown as they were before it.
func (c *conn) local(m proto.ClientMessage) error {
	if c.s.Locals == nil || !slices.Contains(c.caps, proto.CapLocals) {
		return proto.ErrMalformed
	}
	err := c.changeLocal(m)
	switch {
	case err == nil:
		c.send(proto.NewAck(m.ID))
		return nil
	case errors.Is(err, errUnknownProject):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownProject, err.Error()))
	case errors.Is(err, locals.ErrUnknown):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownLocal, err.Error()))
	default:
		c.send(proto.NewError(m.ID, err.Error()))
	}
	// The client shows its change before asking; the list as it stands takes a refused one back.
	c.send(proto.NewLocalList(c.s.Locals.List()))
	return nil
}

func (c *conn) changeLocal(m proto.ClientMessage) error {
	switch m.Type {
	case "local.create":
		if !slices.Contains(c.s.Registry.Load().Projects, m.Project) {
			return errUnknownProject
		}
		return c.s.Locals.Create(m.LocalID, m.Project, m.Name)
	case "local.rename":
		return c.s.Locals.Rename(m.LocalID, m.Title)
	}
	// Removed first, so a create can't start in it while its terminals close.
	if err := c.s.Locals.Delete(m.LocalID); err != nil {
		return err
	}
	for _, t := range c.s.Terminals.All() {
		if t.Info().Local == m.LocalID {
			t.Close()
		}
	}
	return nil
}
