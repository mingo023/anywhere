package hosts

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"strconv"
	"sync"

	"github.com/coder/websocket"

	"pocketd/internal/proto"
)

var errClientClosed = errors.New("The connection to the host closed")

// Client is one WebSocket conn to a host's pocketd through its tunnel. Its reader hands each reply to the call
// that sent its id, and every other frame to on, in the order read.
type Client struct {
	ws   *websocket.Conn
	Caps []string
	done chan struct{}

	mu    sync.Mutex
	next  int
	waits map[string]waiter
}

type waiter struct {
	want  string
	reply chan []byte
}

// Dial says hello to the pocketd at socket, with token when a Windows host's wants one, asking for caps or else
// launch.v1 alone; on must not block.
func Dial(ctx context.Context, socket, token string, on func(kind string, raw []byte), caps ...string) (*Client, error) {
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", socket)
	}}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		return nil, err
	}
	// agent.list and a timeline page can be far over the 32KB default.
	ws.SetReadLimit(-1)
	if caps == nil {
		caps = []string{proto.CapLaunch}
	}
	hello := map[string]any{"type": "hello", "id": "hello", "clientId": "pocketd-host-link", "protocolVersion": proto.MaxVersion, "caps": caps}
	if token != "" {
		hello["ownerToken"] = token
	}
	raw, _ := json.Marshal(hello)
	if err := ws.Write(ctx, websocket.MessageText, raw); err != nil {
		ws.CloseNow()
		return nil, err
	}
	var ok proto.HelloOK
	for ok.Type != "hello.ok" {
		_, raw, err := ws.Read(ctx)
		if err == nil {
			err = json.Unmarshal(raw, &ok)
		}
		if err == nil && ok.Type == "error" {
			err = replyError(raw)
		}
		if err != nil {
			ws.CloseNow()
			return nil, err
		}
	}
	c := &Client{ws: ws, Caps: ok.Caps, done: make(chan struct{}), waits: map[string]waiter{}}
	go c.read(on)
	return c, nil
}

func (c *Client) read(on func(kind string, raw []byte)) {
	defer close(c.done)
	for {
		_, raw, err := c.ws.Read(context.Background())
		if err != nil {
			return
		}
		var head struct{ Type, ID string }
		if json.Unmarshal(raw, &head) != nil {
			continue
		}
		c.mu.Lock()
		w, waited := c.waits[head.ID]
		replied := waited && (head.Type == w.want || head.Type == "error")
		if replied {
			delete(c.waits, head.ID)
		}
		c.mu.Unlock()
		switch {
		case replied:
			w.reply <- raw
		case head.ID == "":
			on(head.Type, raw)
		}
	}
}

// Call sends msg under a fresh id and returns the reply of type want; a verb may send others under that id
// first, as agent.create does, and an error reply is the error.
func (c *Client) Call(ctx context.Context, want string, msg map[string]any) ([]byte, error) {
	c.mu.Lock()
	c.next++
	id := strconv.Itoa(c.next)
	w := waiter{want, make(chan []byte, 1)}
	c.waits[id] = w
	c.mu.Unlock()
	defer func() {
		c.mu.Lock()
		delete(c.waits, id)
		c.mu.Unlock()
	}()
	msg["id"] = id
	raw, _ := json.Marshal(msg)
	if err := c.ws.Write(ctx, websocket.MessageText, raw); err != nil {
		return nil, err
	}
	select {
	case raw := <-w.reply:
		return raw, replyError(raw)
	case <-c.done:
		return nil, errClientClosed
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (c *Client) Done() <-chan struct{} { return c.done }

func (c *Client) Close() { c.ws.CloseNow() }

func replyError(raw []byte) error {
	var e proto.Error
	if json.Unmarshal(raw, &e) == nil && e.Type == "error" {
		return errors.New(e.Message)
	}
	return nil
}
