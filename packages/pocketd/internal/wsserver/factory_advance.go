package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/board"
	"pocketd/internal/factory"
	"pocketd/internal/proto"
)

// factoryAdvance is Move to Checks anyway for a Ticket whose lead gave no verdict.
func (c *conn) factoryAdvance(m proto.ClientMessage) error {
	err := factory.ErrOff
	if slices.Contains(c.caps, proto.CapFactory) {
		err = c.s.Factory.Advance(m.TicketID)
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
