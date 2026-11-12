// Package codex follows a Codex thread through its app-server, next to the TUI.
package codex

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"path/filepath"
	"sync"

	"github.com/coder/websocket"

	"pocketd/internal/envdir"
	"pocketd/internal/jsonrpc"
)

type Handler interface {
	Notify(method string, params json.RawMessage)
	Request(id json.RawMessage, method string, params json.RawMessage)
}

type wsWire struct{ *websocket.Conn }

func (w wsWire) Read(ctx context.Context) ([]byte, error) {
	_, b, err := w.Conn.Read(ctx)
	return b, err
}

func (w wsWire) Write(ctx context.Context, msg []byte) error {
	return w.Conn.Write(ctx, websocket.MessageText, msg)
}

func (w wsWire) Close() { w.CloseNow() }

type Client struct {
	conn     *jsonrpc.Conn
	h        Handler
	handlers sync.WaitGroup
	handled  chan struct{} // closed once the handler has taken the last message read
}

// Dial connects to the app-server control socket. It only speaks
// uncompressed WebSocket, so permessage-deflate must stay off.
func Dial(ctx context.Context, sock string, h Handler) (*Client, error) {
	return connect(ctx, sock, h, nil)
}

func connect(ctx context.Context, sock string, h Handler, capabilities any) (*Client, error) {
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
	return Connect(ctx, wsWire{ws}, h, capabilities)
}

// errInitialized is an app-server that an earlier pocketd initialized, over
// a pipe that outlived it.
const errInitialized = "initialize: Already initialized"

// Connect initializes the app-server at the other end of w. Its ids start
// anywhere, so a reply meant for an earlier pocketd on the same pipe answers
// none of its calls.
func Connect(ctx context.Context, w jsonrpc.Wire, h Handler, capabilities any) (*Client, error) {
	c := &Client{h: h, handled: make(chan struct{})}
	close(c.handled)
	c.conn = jsonrpc.Connect(w, "", c.take)
	_, err := c.Call(ctx, "initialize", map[string]any{"clientInfo": map[string]any{"name": "codex_app_server_daemon", "title": nil, "version": "0"}, "capabilities": capabilities})
	if err != nil && err.Error() == errInitialized {
		return c, nil
	}
	if err == nil {
		err = c.conn.Send(jsonrpc.Message{Method: "initialized"})
	}
	if err != nil {
		w.Close()
		return nil, err
	}
	return c, nil
}

// take hands notifications and server requests to the handler in order, but
// off the reader, so a handler may Call: only the reader delivers the reply.
func (c *Client) take(f jsonrpc.Message) {
	if f.Method == "" {
		return
	}
	prev, next := c.handled, make(chan struct{})
	c.handlers.Go(func() {
		<-prev
		c.handle(f)
		close(next)
	})
	c.handled = next
}

func (c *Client) handle(f jsonrpc.Message) {
	if f.ID != nil {
		c.h.Request(f.ID, f.Method, f.Params)
	} else {
		c.h.Notify(f.Method, f.Params)
	}
}

func (c *Client) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	f, err := c.conn.Call(ctx, method, params)
	switch {
	case errors.Is(err, jsonrpc.ErrClosed):
		return nil, errors.New("codex app-server closed the connection")
	case err != nil:
		return nil, err
	case f.Error != nil:
		return nil, errors.New(method + ": " + f.Error.Message)
	}
	return f.Result, nil
}

func (c *Client) Reply(id json.RawMessage, result any) error {
	return c.conn.Reply(id, result)
}

func (c *Client) Done() <-chan struct{} { return c.conn.Done() }

// Close returns once the handler has taken its last message, so the handler
// must not call it.
func (c *Client) Close() {
	c.conn.Close()
	c.handlers.Wait()
}

type ignore struct{}

func (ignore) Notify(string, json.RawMessage)                   {}
func (ignore) Request(json.RawMessage, string, json.RawMessage) {}

// UpdateSettings sets the model and effort of thread's next turns; "" keeps
// one as it is. Its method is experimental, so it dials with the experimental API.
func UpdateSettings(ctx context.Context, sock, thread, model, effort string) error {
	c, err := connect(ctx, sock, ignore{}, map[string]any{"experimentalApi": true})
	if err != nil {
		return err
	}
	defer c.Close()
	params := map[string]any{"threadId": thread}
	if model != "" {
		params["model"] = model
	}
	if effort != "" {
		params["effort"] = effort
	}
	_, err = c.Call(ctx, "thread/settings/update", params)
	return err
}

// Sock is the app-server control socket for the account in env.
func Sock(env []string) string {
	return filepath.Join(envdir.Lookup(env, "CODEX_HOME", ".codex"), "app-server-control", "app-server-control.sock")
}
