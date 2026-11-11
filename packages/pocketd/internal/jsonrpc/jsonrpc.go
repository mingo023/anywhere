// Package jsonrpc is the calling end of a JSON-RPC 2.0 connection.
package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"math/rand/v2"
	"strconv"
	"sync"
)

// Wire carries messages, one per Read or Write.
type Wire interface {
	Read(ctx context.Context) ([]byte, error)
	Write(ctx context.Context, msg []byte) error
	Close()
}

// Error is what the other end refused a call with.
type Error struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

// Message is a call, a notification or a reply.
type Message struct {
	Version string          `json:"jsonrpc,omitempty"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *Error          `json:"error,omitempty"`
}

// ErrClosed is a call the other end closed the connection on.
var ErrClosed = errors.New("jsonrpc: connection closed")

type Conn struct {
	w       Wire
	version string
	handle  func(Message)
	mu      sync.Mutex
	first   int // the ids this conn calls by follow it
	next    int
	pending map[string]chan Message
	done    chan struct{}
}

// Connect reads w, and hands handle every message that answers none of the
// conn's calls, on the reader, in the order they came: handle must not wait
// on a Call. Every message sent carries version, unless it is "". Its ids
// start anywhere, so a reply meant for an earlier pocketd on the same pipe
// answers none of its calls.
func Connect(w Wire, version string, handle func(Message)) *Conn {
	first := rand.IntN(1 << 30)
	c := &Conn{w: w, version: version, handle: handle, first: first, next: first, pending: map[string]chan Message{}, done: make(chan struct{})}
	go c.read()
	return c
}

func (c *Conn) read() {
	defer close(c.done)
	for {
		raw, err := c.w.Read(context.Background())
		if err != nil {
			return
		}
		var m Message
		if json.Unmarshal(raw, &m) != nil {
			continue
		}
		if m.Method != "" || !c.answer(m) {
			c.handle(m)
		}
	}
}

// answer hands reply m to the call waiting on it, and says whether it
// answers one of the conn's calls, waiting or not.
func (c *Conn) answer(m Message) bool {
	c.mu.Lock()
	ch := c.pending[string(m.ID)]
	delete(c.pending, string(m.ID))
	id, _ := strconv.Atoi(string(m.ID))
	// A call that gave up waiting is answered late.
	ours := id > c.first && id <= c.next
	c.mu.Unlock()
	if ch != nil {
		ch <- m
	}
	return ours
}

func (c *Conn) Send(m Message) error {
	m.Version = c.version
	raw, _ := json.Marshal(m)
	return c.w.Write(context.Background(), raw)
}

// Call returns the reply to method, which may carry an Error, or ErrClosed.
func (c *Conn) Call(ctx context.Context, method string, params any) (Message, error) {
	p, _ := json.Marshal(params)
	c.mu.Lock()
	c.next++
	id := json.RawMessage(strconv.Itoa(c.next))
	ch := make(chan Message, 1)
	c.pending[string(id)] = ch
	c.mu.Unlock()
	defer func() {
		c.mu.Lock()
		delete(c.pending, string(id))
		c.mu.Unlock()
	}()
	if err := c.Send(Message{ID: id, Method: method, Params: p}); err != nil {
		return Message{}, err
	}
	select {
	case m := <-ch:
		return m, nil
	case <-c.done:
		return Message{}, ErrClosed
	case <-ctx.Done():
		return Message{}, ctx.Err()
	}
}

func (c *Conn) Reply(id json.RawMessage, result any) error {
	r, _ := json.Marshal(result)
	return c.Send(Message{ID: id, Result: r})
}

// Done is closed once the connection is.
func (c *Conn) Done() <-chan struct{} { return c.done }

func (c *Conn) Close() {
	c.w.Close()
	<-c.done
}
