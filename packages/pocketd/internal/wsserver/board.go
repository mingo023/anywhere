package wsserver

import (
	"context"
	"errors"
	"slices"
	"time"

	"pocketd/internal/board"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// boardFeed keeps the owner's desktop current: a snapshot on hello and after every change.
func (c *conn) boardFeed(caps []string) {
	if c.s.Board == nil || !c.who.Has(peer.Own) || !slices.Contains(caps, proto.CapBoard) {
		return
	}
	if c.stopBoard != nil {
		c.pushBoard()
		return
	}
	ctx, cancel := context.WithCancel(c.ctx)
	c.stopBoard = cancel
	go func() {
		for {
			changed := c.s.Board.Changed()
			c.pushBoard()
			select {
			case <-ctx.Done():
				return
			case <-changed:
			}
		}
	}()
}

// pushBoard waits for a verb in flight, so its ack comes first.
func (c *conn) pushBoard() {
	c.boardMu.Lock()
	defer c.boardMu.Unlock()
	c.send(proto.NewBoard(c.s.Board.Snapshot()))
}

func (c *conn) boardVerb(m proto.ClientMessage) error {
	if c.s.Board == nil || !slices.Contains(c.caps, proto.CapBoard) {
		return proto.ErrMalformed
	}
	c.boardMu.Lock()
	defer c.boardMu.Unlock()
	if err := c.boardDo(m); err != nil {
		c.send(boardError(m.ID, err))
		return nil
	}
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) boardDo(m proto.ClientMessage) error {
	store, now := c.s.Board, time.Now()
	switch m.Type {
	case "board.save":
		if err := c.usableFolder(m.Ticket.Folder); err != nil {
			return err
		}
		_, err := store.Save(*m.Ticket, m.StartNow, now)
		return err
	case "board.delete":
		return store.Delete(m.TicketID)
	case "board.start":
		return store.Start(m.TicketID, now)
	case "board.move":
		return store.Move(m.TicketID, m.Column, m.Before, now)
	case "board.close":
		return store.Close(m.TicketID, proto.ClosedByUser, now)
	case "board.reopen":
		return store.Reopen(m.TicketID, now)
	}
	return store.SetLimit(m.BoardLimit)
}

func boardError(id string, err error) proto.Error {
	switch {
	case errors.Is(err, board.ErrInvalid), errors.Is(err, errFolderUnusable):
		return proto.NewCodedError(id, proto.CodeInvalidTicket, err.Error(), "")
	case errors.Is(err, board.ErrUnknown):
		return proto.NewCodedError(id, proto.CodeUnknownTicket, err.Error(), "")
	case errors.Is(err, errUnknownProject):
		return proto.NewCodedError(id, proto.CodeUnknownProject, err.Error(), "")
	case errors.Is(err, board.ErrBusy), errors.Is(err, board.ErrStarted):
		return proto.NewCodedError(id, proto.CodeTicketBusy, err.Error(), "")
	}
	return proto.NewError(id, err.Error())
}
