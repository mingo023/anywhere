package ops

import (
	"bufio"
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/pairing"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

type Msg struct {
	Op    string          `json:"op,omitempty"`
	Ev    string          `json:"ev,omitempty"`
	ID    string          `json:"id,omitempty"`
	Cmd   string          `json:"cmd,omitempty"`
	Args  []string        `json:"args,omitempty"`
	Cwd   string          `json:"cwd,omitempty"`
	Env   []string        `json:"env,omitempty"`
	Cols  int             `json:"cols,omitempty"`
	Rows  int             `json:"rows,omitempty"`
	TTY   bool            `json:"tty,omitempty"`
	Text  string          `json:"text,omitempty"`
	Data  []byte          `json:"data,omitempty"`
	Code  int             `json:"code,omitempty"`
	Items []terminal.Info `json:"items,omitempty"`
	Error string          `json:"error,omitempty"`
	// ErrorCode is for callers that act on an error; Code is an exit code.
	ErrorCode string           `json:"errorCode,omitempty"`
	Devices   []devices.Device `json:"devices,omitempty"`
	Pair      *pairing.Offer   `json:"pair,omitempty"`
	Status    *Status          `json:"status,omitempty"`
}

type Conn struct {
	mu   sync.Mutex
	conn net.Conn
	enc  *json.Encoder
	dec  *json.Decoder
}

func newConn(c net.Conn) *Conn {
	return &Conn{conn: c, enc: json.NewEncoder(c), dec: json.NewDecoder(bufio.NewReader(c))}
}

func Dial(path string) (*Conn, error) {
	c, err := net.Dial("unix", path)
	if err != nil {
		return nil, err
	}
	return newConn(c), nil
}

func (c *Conn) Send(m Msg) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.enc.Encode(m)
}

func (c *Conn) Recv() (Msg, error) {
	var m Msg
	err := c.dec.Decode(&m)
	return m, err
}

func (c *Conn) Close() error { return c.conn.Close() }

func Listen(path string) (net.Listener, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	os.Remove(path)
	ln, err := net.Listen("unix", path)
	if err != nil {
		return nil, err
	}
	return ln, os.Chmod(path, 0o600)
}

type Server struct {
	Terminals *terminal.Manager
	Spawn     func(Msg) (*terminal.Terminal, error)
	Hook      func(ctx context.Context, who peer.Principal, m Msg) ([]byte, error)
	Devices   *devices.Store
	// Kick closes a revoked device's live phone sockets.
	Kick    func(id, reason string)
	Pairing *pairing.Manager
	// Host is the tailnet ip:port a phone pairs with; false when Tailscale is off.
	Host    func() (string, bool)
	MacName string
	// WS serves the conns that open with an HTTP request: the owner's WebSocket.
	WS http.Handler
	// PeekTimeout closes a conn that sends nothing; zero means defaultPeekTimeout.
	PeekTimeout time.Duration
	// AskOpen reports whether the agent in a Terminal waits on the user.
	AskOpen func(terminalID string) bool
	Status  func() Status
}

func (s *Server) asking(id string) string {
	if s.AskOpen != nil && s.AskOpen(id) {
		return id
	}
	return ""
}

func (s *Server) Serve(ln net.Listener) error {
	ws := newChanListener(ln.Addr())
	defer ws.Close()
	if s.WS != nil {
		hs := &http.Server{Handler: s.WS, ConnContext: func(ctx context.Context, c net.Conn) context.Context {
			return peer.With(ctx, c.(*sniffed).who)
		}}
		go hs.Serve(ws)
	}
	for {
		c, err := ln.Accept()
		if err != nil {
			return err
		}
		go s.route(c, ws)
	}
}

func (s *Server) route(c net.Conn, ws *chanListener) {
	sc, first, err := sniff(c, s.principal(c), cmp.Or(s.PeekTimeout, defaultPeekTimeout))
	switch {
	case err != nil:
		c.Close()
	case first == 'G' && s.WS != nil:
		ws.hand(sc)
	default:
		s.handle(newConn(sc), sc.who)
	}
}

// principal fails closed: a peer it can't identify is a PTY peer of no Terminal.
func (s *Server) principal(c net.Conn) peer.Principal {
	uc, ok := c.(*net.UnixConn)
	if !ok {
		return peer.Principal{Kind: peer.PTY, Scopes: peer.PTYScopes}
	}
	pid, err := peer.PID(uc)
	if err != nil {
		return peer.Principal{Kind: peer.PTY, Scopes: peer.PTYScopes}
	}
	return peer.Classify(pid, s.Terminals.Roots())
}

func refused(id string, err error) Msg {
	m := Msg{Ev: "error", ID: id, Error: err.Error()}
	if r, ok := errors.AsType[*peer.Refusal](err); ok {
		m.ErrorCode = r.Code
	}
	if errors.Is(err, terminal.ErrPromptTooLarge) {
		m.ErrorCode = proto.CodePromptTooLarge
	}
	return m
}

func (s *Server) spawn(m Msg) (*terminal.Terminal, error) {
	if s.Spawn != nil {
		return s.Spawn(m)
	}
	return s.Terminals.Spawn(terminal.Spec{Cmd: m.Cmd, Args: m.Args, Cwd: m.Cwd, Env: m.Env, Cols: m.Cols, Rows: m.Rows})
}

func (s *Server) handle(c *Conn, who peer.Principal) {
	ctx, cancel := context.WithCancel(context.Background())
	var detaches []func()
	defer func() {
		cancel()
		for _, d := range detaches {
			d()
		}
		c.Close()
	}()
	for {
		m, err := c.Recv()
		if err != nil {
			return
		}
		if r := who.Check("ops:"+m.Op, s.asking(m.ID), m.Text); r != nil {
			c.Send(refused(m.ID, r))
			continue
		}
		switch m.Op {
		case "list":
			c.Send(Msg{Ev: "terminals", Items: s.Terminals.List()})
			continue
		case "spawn":
			t, err := s.spawn(m)
			if err != nil {
				c.Send(Msg{Ev: "error", Error: err.Error()})
				continue
			}
			c.Send(Msg{Ev: "spawned", ID: t.Info().ID})
			continue
		case "status":
			if s.Status == nil {
				c.Send(Msg{Ev: "error", Error: "status unsupported"})
				continue
			}
			st := s.Status()
			c.Send(Msg{Ev: "status", Status: &st})
			continue
		case "hook":
			if s.Hook == nil {
				c.Send(Msg{Ev: "error", Error: "hooks unsupported"})
				continue
			}
			go func() {
				data, err := s.Hook(ctx, who, m)
				if err != nil {
					c.Send(refused(m.ID, err))
					return
				}
				c.Send(Msg{Ev: "hook", Data: data})
			}()
			continue
		case "devices", "devices.rename", "devices.revoke":
			c.Send(s.deviceOp(m))
			continue
		case "pair.begin":
			s.pairBegin(ctx, c, m)
			continue
		}
		t := s.Terminals.Get(m.ID)
		if t == nil {
			c.Send(Msg{Ev: "error", ID: m.ID, Error: "no such terminal"})
			continue
		}
		switch m.Op {
		case "attach":
			detach, err := attach(c, t, m)
			if err != nil {
				c.Send(Msg{Ev: "error", ID: m.ID, Error: err.Error()})
				continue
			}
			detaches = append(detaches, detach)
		case "input":
			t.Write(m.Data)
		case "prompt":
			if err := t.Prompt(m.Text); err != nil {
				c.Send(refused(m.ID, err))
			}
		case "resize":
			t.Resize(m.Cols, m.Rows)
		case "screen":
			c.Send(Msg{Ev: "screen", ID: m.ID, Text: t.Screen()})
		case "close":
			t.Close()
		default:
			c.Send(Msg{Ev: "error", ID: m.ID, Error: "unknown op " + m.Op})
		}
	}
}

// attach sends the snapshot before any event. Events arrive from the moment
// Attach subscribes, before the snapshot can be sent, so they wait in pending.
func attach(c *Conn, t *terminal.Terminal, m Msg) (func(), error) {
	var mu sync.Mutex
	var pending []Msg
	ready := false
	snap, detach, err := t.Attach(m.TTY, func(e terminal.Event) {
		ev := Msg{Ev: e.Kind, ID: m.ID, Data: e.Data, Cols: e.Cols, Rows: e.Rows, Code: e.Code, Text: e.Text}
		mu.Lock()
		defer mu.Unlock()
		if !ready {
			pending = append(pending, ev)
			return
		}
		c.Send(ev)
	})
	if err != nil {
		return nil, err
	}
	info := t.Info()
	mu.Lock()
	defer mu.Unlock()
	c.Send(Msg{Ev: "snapshot", ID: m.ID, Cols: info.Cols, Rows: info.Rows, Data: snap})
	for _, ev := range pending {
		c.Send(ev)
	}
	ready = true
	return detach, nil
}
