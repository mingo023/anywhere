package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/board"
	"pocketd/internal/factory"
	"pocketd/internal/proto"
)

// factoryHint sends a hint to the lead of a Ticket that failed at Checks.
func (c *conn) factoryHint(m proto.ClientMessage) error {
	err := factory.ErrOff
	if slices.Contains(c.caps, proto.CapFactory) {
		err = c.s.Factory.Hint(m.TicketID, m.Text)
	}
	switch {
	case err == nil:
		c.send(proto.NewAck(m.ID))
	case errors.Is(err, factory.ErrOff):
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, err.Error()))
	case errors.Is(err, board.ErrUnknown):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownTicket, err.Error()))
	default:
		c.send(proto.NewErrorCode(m.ID, proto.CodeTicketBusy, err.Error()))
	}
	return nil
}
