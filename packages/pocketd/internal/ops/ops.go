package ops

import (
	"bufio"
	"context"
	"encoding/json"
	"net"
	"os"
	"path/filepath"
	"sync"

	"pocketd/internal/session"
)

type Msg struct {
	Op    string         `json:"op,omitempty"`
	Ev    string         `json:"ev,omitempty"`
	ID    string         `json:"id,omitempty"`
	Cmd   string         `json:"cmd,omitempty"`
	Args  []string       `json:"args,omitempty"`
	Cwd   string         `json:"cwd,omitempty"`
	Env   []string       `json:"env,omitempty"`
	Cols  int            `json:"cols,omitempty"`
	Rows  int            `json:"rows,omitempty"`
	TTY   bool           `json:"tty,omitempty"`
	Text  string         `json:"text,omitempty"`
	Data  []byte         `json:"data,omitempty"`
	Code  int            `json:"code,omitempty"`
	Items []session.Info `json:"items,omitempty"`
	Error string         `json:"error,omitempty"`
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
	Sessions *session.Manager
	Spawn    func(Msg) (*session.Session, error)
	Hook     func(ctx context.Context, payload []byte) []byte
}

func (s *Server) Serve(ln net.Listener) error {
	for {
		c, err := ln.Accept()
		if err != nil {
			return err
		}
		go s.handle(newConn(c))
	}
}

func (s *Server) spawn(m Msg) (*session.Session, error) {
	if s.Spawn != nil {
		return s.Spawn(m)
	}
	return s.Sessions.Spawn(session.Spec{Cmd: m.Cmd, Args: m.Args, Cwd: m.Cwd, Env: m.Env, Cols: m.Cols, Rows: m.Rows})
}

func (s *Server) handle(c *Conn) {
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
		switch m.Op {
		case "list":
			c.Send(Msg{Ev: "sessions", Items: s.Sessions.List()})
			continue
		case "spawn":
			sess, err := s.spawn(m)
			if err != nil {
				c.Send(Msg{Ev: "error", Error: err.Error()})
				continue
			}
			c.Send(Msg{Ev: "spawned", ID: sess.Info().ID})
			continue
		case "hook":
			if s.Hook == nil {
				c.Send(Msg{Ev: "error", Error: "hooks unsupported"})
				continue
			}
			go func() { c.Send(Msg{Ev: "hook", Data: s.Hook(ctx, m.Data)}) }()
			continue
		}
		sess := s.Sessions.Get(m.ID)
		if sess == nil {
			c.Send(Msg{Ev: "error", ID: m.ID, Error: "no such session"})
			continue
		}
		switch m.Op {
		case "attach":
			detach, err := attach(c, sess, m)
			if err != nil {
				c.Send(Msg{Ev: "error", ID: m.ID, Error: err.Error()})
				continue
			}
			detaches = append(detaches, detach)
		case "input":
			sess.Write(m.Data)
		case "prompt":
			sess.Prompt(m.Text)
		case "resize":
			sess.Resize(m.Cols, m.Rows)
		case "screen":
			c.Send(Msg{Ev: "screen", ID: m.ID, Text: sess.Screen()})
		case "close":
			sess.Close()
		default:
			c.Send(Msg{Ev: "error", ID: m.ID, Error: "unknown op " + m.Op})
		}
	}
}

// attach sends the snapshot before any event. Events arrive from the moment
// Attach subscribes, before the snapshot can be sent, so they wait in pending.
func attach(c *Conn, sess *session.Session, m Msg) (func(), error) {
	var mu sync.Mutex
	var pending []Msg
	ready := false
	snap, detach, err := sess.Attach(m.TTY, func(e session.Event) {
		ev := Msg{Ev: e.Kind, ID: m.ID, Data: e.Data, Cols: e.Cols, Rows: e.Rows, Code: e.Code}
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
	info := sess.Info()
	mu.Lock()
	defer mu.Unlock()
	c.Send(Msg{Ev: "snapshot", ID: m.ID, Cols: info.Cols, Rows: info.Rows, Data: snap})
	for _, ev := range pending {
		c.Send(ev)
	}
	ready = true
	return detach, nil
}
