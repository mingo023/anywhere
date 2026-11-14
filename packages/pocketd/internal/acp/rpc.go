// Package acp drives an agent through the Agent Client Protocol: JSON-RPC
// 2.0, one message per line, on the agent's stdio.
package acp

import (
	"context"
	"encoding/json"
	"errors"
	"strings"

	"pocketd/internal/jsonrpc"
)

type Handler interface {
	Notify(method string, params json.RawMessage)
	Request(id json.RawMessage, method string, params json.RawMessage)
	// Stray takes the result of a call this client didn't make: one an
	// earlier pocketd made on the same pipe.
	Stray(result json.RawMessage)
}

// Error is what an agent refused a call with.
type Error jsonrpc.Error

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

type frame = jsonrpc.Message

type Client struct {
	conn *jsonrpc.Conn
	h    Handler
}

// Connect reads w for h. Its ids start anywhere, so a reply meant for an
// earlier pocketd on the same pipe answers none of its calls.
func Connect(w jsonrpc.Wire, h Handler) *Client {
	c := &Client{h: h}
	c.conn = jsonrpc.Connect(w, "2.0", c.take)
	return c
}

// take hands each message to the handler in the order the agent wrote it,
// so the handler must not wait on a Call.
func (c *Client) take(f frame) {
	switch {
	case f.Method != "" && f.ID != nil:
		c.h.Request(f.ID, f.Method, f.Params)
	case f.Method != "":
		c.h.Notify(f.Method, f.Params)
	case f.Error == nil:
		c.h.Stray(f.Result)
	}
}

// Call returns the agent's result, or its *Error.
func (c *Client) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	return result(c.conn.Call(ctx, method, params))
}

// Start sends method and returns the wait for its result, so what is sent after Start returns reaches the agent after it.
func (c *Client) Start(method string, params any) (func(context.Context) (json.RawMessage, error), error) {
	wait, err := c.conn.Start(method, params)
	if err != nil {
		return nil, err
	}
	return func(ctx context.Context) (json.RawMessage, error) { return result(wait(ctx)) }, nil
}

func result(f frame, err error) (json.RawMessage, error) {
	switch {
	case errors.Is(err, jsonrpc.ErrClosed):
		return nil, errors.New("the agent closed the connection")
	case err != nil:
		return nil, err
	case f.Error != nil:
		return nil, (*Error)(f.Error)
	}
	return f.Result, nil
}

func (c *Client) Notify(method string, params any) error {
	p, _ := json.Marshal(params)
	return c.conn.Send(frame{Method: method, Params: p})
}

func (c *Client) Reply(id json.RawMessage, result any) error {
	return c.conn.Reply(id, result)
}

func (c *Client) Refuse(id json.RawMessage, code int, message string) error {
	return c.conn.Send(frame{ID: id, Error: &jsonrpc.Error{Code: code, Message: message}})
}

func (c *Client) Close() { c.conn.Close() }
