// Package acp drives an agent through the Agent Client Protocol: JSON-RPC
// 2.0, one message per line, on the agent's stdio.
package acp

import (
	"context"
	"encoding/json"
	"errors"
	"math/rand/v2"
	"strconv"
	"strings"
	"sync"
)

type Handler interface {
	Notify(method string, params json.RawMessage)
	Request(id json.RawMessage, method string, params json.RawMessage)
	// Stray takes the result of a call this client didn't make: one an
	// earlier pocketd made on the same pipe.
	Stray(result json.RawMessage)
}

// Wire carries JSON-RPC messages, one per Read or Write.
type Wire interface {
	Read(ctx context.Context) ([]byte, error)
	Write(ctx context.Context, msg []byte) error
	Close()
}

// Error is what an agent refused a call with.
type Error struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

// Error is the agent's message with the detail it gave: Hermes says only
// "Internal error" and puts the reason in data.details; Cursor's detail
// repeats its message.
func (e *Error) Error() string {
	var d struct {
		Details string `json:"details"`
		Message string `json:"message"`
	}
	json.Unmarshal(e.Data, &d)
	switch detail := d.Details + d.Message; {
	case detail == "":
		return e.Message
	case strings.HasPrefix(detail, e.Message):
		return detail
	default:
		return e.Message + ": " + detail
	}
}

const methodNotFound = -32601

type frame struct {
	Version string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *Error          `json:"error,omitempty"`
}

type Client struct {
	w       Wire
	h       Handler
	mu      sync.Mutex
	first   int // the ids this client calls by follow it
	next    int
	pending map[string]chan frame
	done    chan struct{}
}

// Connect reads w for h. Its ids start anywhere, so a reply meant for an
// earlier pocketd on the same pipe answers none of its calls.
func Connect(w Wire, h Handler) *Client {
	first := rand.IntN(1 << 30)
	c := &Client{w: w, h: h, first: first, next: first, pending: map[string]chan frame{}, done: make(chan struct{})}
	go c.read()
	return c
}

// read hands each message to the handler in the order the agent wrote it,
// so the handler must not wait on a Call.
func (c *Client) read() {
	defer close(c.done)
	for {
		raw, err := c.w.Read(context.Background())
		if err != nil {
			return
		}
		var f frame
		if json.Unmarshal(raw, &f) != nil {
			continue
		}
		switch {
		case f.Method != "" && f.ID != nil:
			c.h.Request(f.ID, f.Method, f.Params)
		case f.Method != "":
			c.h.Notify(f.Method, f.Params)
		default:
			c.mu.Lock()
			ch := c.pending[string(f.ID)]
			delete(c.pending, string(f.ID))
			id, _ := strconv.Atoi(string(f.ID))
			// A call of this client's that gave up waiting is answered late.
			ours := id > c.first && id <= c.next
			c.mu.Unlock()
			if ch != nil {
				ch <- f
			} else if f.Error == nil && !ours {
				c.h.Stray(f.Result)
			}
		}
	}
}

func (c *Client) send(f frame) error {
	f.Version = "2.0"
	raw, _ := json.Marshal(f)
	return c.w.Write(context.Background(), raw)
}

// Call returns the agent's result, or its *Error.
func (c *Client) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	p, _ := json.Marshal(params)
	c.mu.Lock()
	c.next++
	id := json.RawMessage(strconv.Itoa(c.next))
	ch := make(chan frame, 1)
	c.pending[string(id)] = ch
	c.mu.Unlock()
	defer func() {
		c.mu.Lock()
		delete(c.pending, string(id))
		c.mu.Unlock()
	}()
	if err := c.send(frame{ID: id, Method: method, Params: p}); err != nil {
		return nil, err
	}
	select {
	case f := <-ch:
		if f.Error != nil {
			return nil, f.Error
		}
		return f.Result, nil
	case <-c.done:
		return nil, errors.New("the agent closed the connection")
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (c *Client) Notify(method string, params any) error {
	p, _ := json.Marshal(params)
	return c.send(frame{Method: method, Params: p})
}

func (c *Client) Reply(id json.RawMessage, result any) error {
	r, _ := json.Marshal(result)
	return c.send(frame{ID: id, Result: r})
}

func (c *Client) Refuse(id json.RawMessage, code int, message string) error {
	return c.send(frame{ID: id, Error: &Error{Code: code, Message: message}})
}

func (c *Client) Close() {
	c.w.Close()
	<-c.done
}
