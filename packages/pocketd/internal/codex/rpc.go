// Package codex follows a Codex thread through its app-server, next to the TUI.
package codex

import (
	"context"
	"encoding/json"
	"errors"
	"math/rand/v2"
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

// Wire carries JSON-RPC messages, one per Read or Write.
type Wire interface {
	Read(ctx context.Context) ([]byte, error)
	Write(ctx context.Context, msg []byte) error
	Close()
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
	w        Wire
	h        Handler
	handlers sync.WaitGroup
	mu       sync.Mutex
	next     int
	pending  map[string]chan frame
	done     chan struct{}
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
func Connect(ctx context.Context, w Wire, h Handler, capabilities any) (*Client, error) {
	c := &Client{w: w, h: h, next: rand.IntN(1 << 30), pending: map[string]chan frame{}, done: make(chan struct{})}
	go c.read()
	_, err := c.Call(ctx, "initialize", map[string]any{"clientInfo": map[string]any{"name": "codex_app_server_daemon", "title": nil, "version": "0"}, "capabilities": capabilities})
	if err != nil && err.Error() == errInitialized {
		return c, nil
	}
	if err == nil {
		err = c.send(frame{Method: "initialized"})
	}
	if err != nil {
		w.Close()
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
		raw, err := c.w.Read(context.Background())
		if err != nil {
			return
		}
		var f frame
		if json.Unmarshal(raw, &f) != nil {
			continue
		}
		if f.Method != "" {
			prev, next := handled, make(chan struct{})
			c.handlers.Go(func() {
				<-prev
				c.handle(f)
				close(next)
			})
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
	return c.w.Write(context.Background(), raw)
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

// Close returns once the handler has taken its last message, so the handler
// must not call it.
func (c *Client) Close() {
	c.w.Close()
	<-c.done
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
