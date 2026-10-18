// Package pipe relays the JSON lines of an agent that Pocket runs without its
// TUI between the agent's stdio and pocketd. `pocketd pipe` runs it inside the
// agent's Terminal, so the agent outlives a pocketd that restarts.
package pipe

import (
	"bufio"
	"encoding/json"
	"io"
	"slices"
	"sync"
	"time"

	"pocketd/internal/ops"
)

// keep is how many lines wait for a pocketd that is away; older ones go.
const keep = 2000

// Redial is how long the relay waits before dialing pocketd again.
var Redial = 250 * time.Millisecond

// open is the requests the agent made that no reply has answered. A pocketd
// that went away took their questions with it, so the next one gets them again.
type open struct {
	ids   []string
	lines map[string]string
}

type line struct {
	Type      string `json:"type"`
	RequestID string `json:"request_id"`
	Response  struct {
		RequestID string `json:"request_id"`
	} `json:"response"`
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
}

// out notes a line from the agent: claude's control_request, or a JSON-RPC
// request from codex's app-server. It reports whether the line is a request.
func (o *open) out(s string) bool {
	var l line
	if json.Unmarshal([]byte(s), &l) != nil {
		return false
	}
	switch {
	case l.Type == "control_request":
		o.add(l.RequestID, s)
		return true
	case l.Type == "control_cancel_request":
		o.del(l.RequestID)
	case l.Method != "" && len(l.ID) > 0:
		o.add("rpc:"+string(l.ID), s)
		return true
	}
	return false
}

// in notes a line to the agent, which may answer one of its requests.
func (o *open) in(s string) {
	var l line
	if json.Unmarshal([]byte(s), &l) != nil {
		return
	}
	switch {
	case l.Type == "control_response":
		o.del(l.Response.RequestID)
	case l.Type == "" && l.Method == "" && len(l.ID) > 0:
		o.del("rpc:" + string(l.ID))
	}
}

func (o *open) add(id, s string) {
	if o.lines == nil {
		o.lines = map[string]string{}
	}
	if _, ok := o.lines[id]; !ok {
		o.ids = append(o.ids, id)
	}
	o.lines[id] = s
}

func (o *open) del(id string) {
	if _, ok := o.lines[id]; ok {
		delete(o.lines, id)
		o.ids = slices.DeleteFunc(o.ids, func(x string) bool { return x == id })
	}
}

func (o *open) all() []string {
	out := make([]string, len(o.ids))
	for i, id := range o.ids {
		out[i] = o.lines[id]
	}
	return out
}

// Relay carries lines between an agent and whichever pocketd is up.
type Relay struct {
	mu    sync.Mutex
	conn  *ops.Conn
	held  []string // what the agent said while no pocketd listened
	asked open
}

// Out reads the agent's stdout until it ends, passing each line on.
func (r *Relay) Out(stdout io.Reader) error {
	br := bufio.NewReaderSize(stdout, 64<<10)
	for {
		b, err := br.ReadBytes('\n')
		if len(b) > 1 {
			r.said(string(b[:len(b)-1]))
		}
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
	}
}

func (r *Relay) said(s string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	request := r.asked.out(s)
	if r.conn != nil && r.conn.Send(ops.Msg{Ev: "out", Text: s}) == nil {
		return
	}
	r.conn = nil
	if !request {
		if r.held = append(r.held, s); len(r.held) > keep {
			r.held = slices.Delete(r.held, 0, len(r.held)-keep)
		}
	}
}

// Serve keeps the agent's stdin connected to pocketd at sock as terminal id,
// dialing again whenever pocketd goes away. It returns once stdin fails.
func (r *Relay) Serve(sock, id string, stdin io.Writer) error {
	for {
		c, err := ops.Dial(sock)
		if err != nil {
			time.Sleep(Redial)
			continue
		}
		if err := r.serve(c, id, stdin); err != nil {
			return err
		}
		time.Sleep(Redial)
	}
}

// serve relays one connection; only a write to the agent that fails is an error.
func (r *Relay) serve(c *ops.Conn, id string, stdin io.Writer) error {
	defer c.Close()
	if !r.attach(c, id) {
		return nil
	}
	for {
		m, err := c.Recv()
		if err != nil || m.Ev == "error" {
			r.detach(c)
			return nil
		}
		if m.Op != "in" {
			continue
		}
		r.mu.Lock()
		r.asked.in(m.Text)
		r.mu.Unlock()
		if _, err := io.WriteString(stdin, m.Text+"\n"); err != nil {
			return err
		}
	}
}

// attach hands c, once pocketd takes it, the requests still open, then what
// was held. Lines the agent says meanwhile wait, so they keep their order.
func (r *Relay) attach(c *ops.Conn, id string) bool {
	if c.Send(ops.Msg{Op: "pipe", ID: id}) != nil {
		return false
	}
	if m, err := c.Recv(); err != nil || m.Ev != "ok" {
		return false
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, s := range slices.Concat(r.asked.all(), r.held) {
		if c.Send(ops.Msg{Ev: "out", Text: s}) != nil {
			return false
		}
	}
	r.held, r.conn = nil, c
	return true
}

func (r *Relay) detach(c *ops.Conn) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.conn == c {
		r.conn = nil
	}
}
