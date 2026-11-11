// Package pi drives pi, or its fork omp, run with `--mode rpc`: JSON, one
// record per line, on the agent's stdio. A command carries an id its
// response repeats; every other record the agent writes is an event.
package pi

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"math/rand/v2"
	"strconv"
	"strings"
	"sync"
)

type Handler interface {
	// Event takes a record that answers no command, in the order the agent
	// wrote it: an event, or another response to a command already answered.
	Event(kind string, raw json.RawMessage)
}

// Wire carries records, one per Read or Write.
type Wire interface {
	Read(ctx context.Context) ([]byte, error)
	Write(ctx context.Context, msg []byte) error
	Close()
}

type record struct {
	Type    string          `json:"type"`
	ID      string          `json:"id"`
	Command string          `json:"command"`
	Success bool            `json:"success"`
	Error   json.RawMessage `json:"error"`
	Data    json.RawMessage `json:"data"`
}

type Client struct {
	w       Wire
	h       Handler
	mu      sync.Mutex
	prefix  string
	next    int
	pending map[string]func(json.RawMessage, error)
	done    chan struct{}
}

// Connect reads w for h. Its ids start anywhere, so a response meant for an
// earlier pocketd on the same pipe answers none of its commands.
func Connect(w Wire, h Handler) *Client {
	c := &Client{w: w, h: h, prefix: "aw" + strconv.Itoa(rand.IntN(1<<30)) + "-", pending: map[string]func(json.RawMessage, error){}, done: make(chan struct{})}
	go c.read()
	return c
}

func (c *Client) read() {
	defer close(c.done)
	for {
		raw, err := c.w.Read(context.Background())
		if err != nil {
			return
		}
		var r record
		// An event's id may be no string; only a response's is read.
		json.Unmarshal(raw, &r)
		if r.Type == "" {
			continue
		}
		if r.Type != "response" {
			c.h.Event(r.Type, raw)
			continue
		}
		c.mu.Lock()
		reply := c.pending[r.ID]
		delete(c.pending, r.ID)
		c.mu.Unlock()
		switch {
		case reply == nil:
			if strings.HasPrefix(r.ID, c.prefix) {
				c.h.Event(r.Type, raw)
			}
		case r.Success:
			reply(r.Data, nil)
		default:
			reply(nil, errors.New(cmp.Or(text(r.Error), r.Command+" failed")))
		}
	}
}

// Send writes a command and returns its id. reply, if set, takes the agent's
// response on the reader, in the order it came among the events: it must not
// wait on a Call.
func (c *Client) Send(kind string, fields map[string]any, reply func(data json.RawMessage, err error)) (string, error) {
	c.mu.Lock()
	c.next++
	id := c.prefix + strconv.Itoa(c.next)
	if reply != nil {
		c.pending[id] = reply
	}
	c.mu.Unlock()
	cmd := map[string]any{"type": kind, "id": id}
	for k, v := range fields {
		cmd[k] = v
	}
	err := c.Write(cmd)
	if err != nil {
		c.forget(id)
	}
	return id, err
}

func (c *Client) forget(id string) {
	c.mu.Lock()
	delete(c.pending, id)
	c.mu.Unlock()
}

// Call returns the data the agent answers a command with, or the error it refused it with.
func (c *Client) Call(ctx context.Context, kind string, fields map[string]any) (json.RawMessage, error) {
	type result struct {
		data json.RawMessage
		err  error
	}
	ch := make(chan result, 1)
	id, err := c.Send(kind, fields, func(data json.RawMessage, err error) { ch <- result{data, err} })
	if err != nil {
		return nil, err
	}
	select {
	case r := <-ch:
		return r.data, r.err
	case <-c.done:
		return nil, errors.New("the agent closed the connection")
	case <-ctx.Done():
		c.forget(id)
		return nil, ctx.Err()
	}
}

// Write writes a record that is no command: the answer to an extension's dialog.
func (c *Client) Write(v any) error {
	raw, _ := json.Marshal(v)
	return c.w.Write(context.Background(), raw)
}

func (c *Client) Close() {
	c.w.Close()
	<-c.done
}
