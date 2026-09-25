// Package codex follows a Codex thread through its app-server, next to the TUI.
package codex

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"path/filepath"
	"strconv"
	"sync"

	"github.com/coder/websocket"

	"pocketd/internal/envdir"
)

type Handler interface {
	Notify(method string, params json.RawMessage)
	Request(id json.RawMessage, method string, params json.RawMessage)
}

type Client struct {
	ws      *websocket.Conn
	h       Handler
	mu      sync.Mutex
	next    int
	pending map[string]chan frame
	done    chan struct{}
}

type frame struct {
	ID     json.RawMessage `json:"id,omitempty"`
	Method string          `json:"method,omitempty"`
	Params json.RawMessage `json:"params,omitempty"`
	Result json.RawMessage `json:"result,omitempty"`
	Error  *struct {
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

// Dial connects to the app-server control socket. It only speaks
// uncompressed WebSocket, so permessage-deflate must stay off.
func Dial(ctx context.Context, sock string, h Handler) (*Client, error) {
	httpc := &http.Client{Transport: &http.Transport{
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			return (&net.Dialer{}).DialContext(ctx, "unix", sock)
		},
	}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: httpc, CompressionMode: websocket.CompressionDisabled})
	if err != nil {
		return nil, err
	}
	ws.SetReadLimit(64 << 20)
	c := &Client{ws: ws, h: h, pending: map[string]chan frame{}, done: make(chan struct{})}
	go c.read()
	if _, err := c.Call(ctx, "initialize", map[string]any{"clientInfo": map[string]any{"name": "pocketd", "title": nil, "version": "0"}, "capabilities": nil}); err != nil {
		ws.CloseNow()
		return nil, err
	}
	if err := c.send(frame{Method: "initialized"}); err != nil {
		ws.CloseNow()
		return nil, err
	}
	return c, nil
}

// read hands notifications and server requests to the handler in order, but
// off this goroutine, so a handler may Call: only read delivers the reply.
func (c *Client) read() {
	defer close(c.done)
	handled := make(chan struct{})
	close(handled)
	for {
		_, raw, err := c.ws.Read(context.Background())
		if err != nil {
			return
		}
		var f frame
		if json.Unmarshal(raw, &f) != nil {
			continue
		}
		if f.Method != "" {
			prev, next := handled, make(chan struct{})
			go func() {
				<-prev
				c.handle(f)
				close(next)
			}()
			handled = next
			continue
		}
		c.mu.Lock()
		ch := c.pending[string(f.ID)]
		delete(c.pending, string(f.ID))
		c.mu.Unlock()
		if ch != nil {
			ch <- f
		}
	}
}

func (c *Client) handle(f frame) {
	if f.ID != nil {
		c.h.Request(f.ID, f.Method, f.Params)
	} else {
		c.h.Notify(f.Method, f.Params)
	}
}

func (c *Client) send(f frame) error {
	raw, _ := json.Marshal(f)
	return c.ws.Write(context.Background(), websocket.MessageText, raw)
}

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
			return nil, errors.New(method + ": " + f.Error.Message)
		}
		return f.Result, nil
	case <-c.done:
		return nil, errors.New("codex app-server closed the connection")
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (c *Client) Reply(id json.RawMessage, result any) error {
	r, _ := json.Marshal(result)
	return c.send(frame{ID: id, Result: r})
}

func (c *Client) Done() <-chan struct{} { return c.done }

func (c *Client) Close() { c.ws.CloseNow() }

type nopHandler struct{}

func (nopHandler) Notify(string, json.RawMessage)                   {}
func (nopHandler) Request(json.RawMessage, string, json.RawMessage) {}

func Loaded(ctx context.Context, sock string) ([]string, error) {
	c, err := Dial(ctx, sock, nopHandler{})
	if err != nil {
		return nil, err
	}
	defer c.Close()
	res, err := c.Call(ctx, "thread/loaded/list", map[string]any{})
	if err != nil {
		return nil, err
	}
	var r struct {
		Data []string `json:"data"`
	}
	if err := json.Unmarshal(res, &r); err != nil {
		return nil, err
	}
	return r.Data, nil
}

// Sock is the app-server control socket for the account in env.
func Sock(env []string) string {
	return filepath.Join(envdir.Lookup(env, "CODEX_HOME", ".codex"), "app-server-control", "app-server-control.sock")
}
