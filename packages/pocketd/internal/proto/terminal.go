package proto

import (
	"errors"
	"math"
)

// CapTerminal gates the terminal.* verbs and their binary frames.
const CapTerminal = "terminal.v1"

// CodeTerminalsOff: the owner turned phone.terminals off.
const CodeTerminalsOff = "terminals_off"

// CodeTerminalEnded: the terminal a verb names isn't running.
const CodeTerminalEnded = "terminal_ended"

const maxTermSize = 1000

var terminalVerbs = map[string]verbDecoder{
	"terminal.list": func(fields, *ClientMessage) bool { return true },
	"terminal.attach": func(f fields, m *ClientMessage) bool {
		return f.text("terminalId", &m.TerminalID)
	},
	"terminal.detach": func(f fields, m *ClientMessage) bool {
		return f.text("terminalId", &m.TerminalID)
	},
	"terminal.close": func(f fields, m *ClientMessage) bool {
		return f.text("terminalId", &m.TerminalID)
	},
	"terminal.input": func(f fields, m *ClientMessage) bool {
		return f.text("terminalId", &m.TerminalID) && f.bytes("data", &m.Data, math.MaxInt)
	},
	"terminal.resize": func(f fields, m *ClientMessage) bool {
		return f.text("terminalId", &m.TerminalID) && f.integer("cols", &m.Cols, 1, maxTermSize) && f.integer("rows", &m.Rows, 1, maxTermSize)
	},
	"terminal.create": func(f fields, m *ClientMessage) bool {
		return f.requestID(m) && f.optionalString("cwd", &m.Cwd) &&
			f.optionalString("localId", &m.LocalID) && (!f.has("localId") || ValidLocalID(m.LocalID)) &&
			f.optionalInt("cols", &m.Cols, 1, maxTermSize) && f.optionalInt("rows", &m.Rows, 1, maxTermSize)
	},
}

// Terminal is one PTY as a phone lists it. Title is the foreground process
// text; AgentID the agent running in it now, if any.
type Terminal struct {
	ID      string `json:"id"`
	Cwd     string `json:"cwd"`
	Cols    int    `json:"cols"`
	Rows    int    `json:"rows"`
	Title   string `json:"title,omitempty"`
	LocalID string `json:"localId,omitempty"`
	AgentID string `json:"agentId,omitempty"`
	Exited  bool   `json:"exited,omitempty"`
}

// TerminalList answers terminal.list, and is pushed whole without an id on spawn, exit and title change.
type TerminalList struct {
	Type      string     `json:"type"`
	ID        string     `json:"id,omitempty"`
	Terminals []Terminal `json:"terminals"`
}

func NewTerminalList(id string, terminals []Terminal) TerminalList {
	if terminals == nil {
		terminals = []Terminal{}
	}
	return TerminalList{"terminal.list", id, terminals}
}

type TerminalAttached struct {
	Type       string `json:"type"`
	ID         string `json:"id"`
	TerminalID string `json:"terminalId"`
	Cols       int    `json:"cols"`
	Rows       int    `json:"rows"`
}

func NewTerminalAttached(id, terminalID string, cols, rows int) TerminalAttached {
	return TerminalAttached{"terminal.attached", id, terminalID, cols, rows}
}

type TerminalResized struct {
	Type       string `json:"type"`
	TerminalID string `json:"terminalId"`
	Cols       int    `json:"cols"`
	Rows       int    `json:"rows"`
}

func NewTerminalResized(terminalID string, cols, rows int) TerminalResized {
	return TerminalResized{"terminal.resized", terminalID, cols, rows}
}

// TerminalExited ends every attach to the terminal.
type TerminalExited struct {
	Type       string `json:"type"`
	TerminalID string `json:"terminalId"`
	Code       int    `json:"code"`
}

func NewTerminalExited(terminalID string, code int) TerminalExited {
	return TerminalExited{"terminal.exited", terminalID, code}
}

type TerminalCreated struct {
	Type       string `json:"type"`
	ID         string `json:"id"`
	RequestID  string `json:"requestId"`
	TerminalID string `json:"terminalId"`
}

func NewTerminalCreated(id, requestID, terminalID string) TerminalCreated {
	return TerminalCreated{"terminal.created", id, requestID, terminalID}
}

// Binary frame kinds. A snapshot replaces the screen: reset, then feed.
const (
	FrameSnapshot byte = 1
	FrameOutput   byte = 2
)

// AppendFrame appends a binary terminal frame: kind, the id's length, the
// ASCII id, then the payload as is.
func AppendFrame(dst []byte, kind byte, terminalID string, payload []byte) ([]byte, error) {
	if terminalID == "" || len(terminalID) > math.MaxUint8 {
		return dst, errors.New("proto: a frame's terminal id is 1 to 255 bytes")
	}
	for i := range len(terminalID) {
		if terminalID[i] < 0x20 || terminalID[i] > 0x7e {
			return dst, errors.New("proto: a frame's terminal id is printable ASCII")
		}
	}
	dst = append(dst, kind, byte(len(terminalID)))
	dst = append(dst, terminalID...)
	return append(dst, payload...), nil
}
