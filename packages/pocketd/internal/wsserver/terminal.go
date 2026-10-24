package wsserver

import (
	"context"
	"slices"

	"github.com/coder/websocket"

	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// TerminalService serves the terminal.v1 verbs.
type TerminalService interface{}

// terminalConn is a conn's terminal.v1 state.
type terminalConn struct{}

func (c *conn) helloTerminal() {}

func (c *conn) closeTerminal() {}

// terminalVerb serves terminal.v1; without the cap its verbs are unknown.
func (c *conn) terminalVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapTerminal) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "terminal.list":
		return c.terminalList(m)
	case "terminal.attach":
		return c.terminalAttach(m)
	case "terminal.detach":
		return c.terminalDetach(m)
	}
	if c.who.Kind != peer.Owner && c.s.Settings != nil && !c.s.Settings.PhoneTerminals() {
		c.send(proto.NewErrorCode(m.ID, proto.CodeTerminalsOff, "Terminals are off for phones on this Mac"))
		return nil
	}
	switch m.Type {
	case "terminal.input":
		return c.terminalInput(m)
	case "terminal.resize":
		return c.terminalResize(m)
	case "terminal.create":
		return c.terminalCreate(m)
	}
	return c.terminalClose(m)
}

func (c *conn) terminalList(m proto.ClientMessage) error   { return c.notImplemented(m) }
func (c *conn) terminalAttach(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) terminalDetach(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) terminalInput(m proto.ClientMessage) error  { return c.notImplemented(m) }
func (c *conn) terminalResize(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) terminalCreate(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) terminalClose(m proto.ClientMessage) error  { return c.notImplemented(m) }

// writeFrame sends one binary terminal frame: proto.FrameSnapshot or proto.FrameOutput.
func writeFrame(ctx context.Context, ws *websocket.Conn, kind byte, terminalID string, payload []byte) error {
	frame, err := proto.AppendFrame(make([]byte, 0, 2+len(terminalID)+len(payload)), kind, terminalID, payload)
	if err != nil {
		return err
	}
	return ws.Write(ctx, websocket.MessageBinary, frame)
}
