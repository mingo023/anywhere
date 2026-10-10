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
	var err error
	switch m.Type {
	case "local.create":
		if !slices.Contains(c.s.Registry.Load().Projects, m.Project) {
			c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownProject, errUnknownProject.Error()))
			return nil
		}
		err = c.s.Locals.Create(m.LocalID, m.Project, m.Name)
	case "local.rename":
		err = c.s.Locals.Rename(m.LocalID, m.Title)
	case "local.delete":
		// Removed first, so a create can't start in it while its terminals close.
		if err = c.s.Locals.Delete(m.LocalID); err == nil {
			for _, t := range c.s.Terminals.All() {
				if t.Info().Local == m.LocalID {
					t.Close()
				}
			}
		}
	}
	if errors.Is(err, locals.ErrUnknown) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownLocal, err.Error()))
		return nil
	}
	if err != nil {
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
