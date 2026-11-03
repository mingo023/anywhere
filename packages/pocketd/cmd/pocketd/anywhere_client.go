package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"strconv"

	"github.com/coder/websocket"

	"pocketd/internal/proto"
)

// client is one conn to pocketd's WebSocket protocol over its unix socket,
// where a process outside every Terminal is the owner.
type client struct {
	ctx    context.Context
	ws     *websocket.Conn
	calls  int
	listed bool
	agents map[string]proto.AgentSummary
	// began holds each agent seen in a turn. hello subscribes before any verb, so no turn slips past.
	began map[string]bool
}

func dialClient(ctx context.Context, sock string) (*client, error) {
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", sock)
	}}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		return nil, fmt.Errorf("can't reach Anywhere's service at %s: %w", sock, err)
	}
	// agent.list and a timeline page can be far over the 32KB default.
	ws.SetReadLimit(-1)
	c := &client{ctx: ctx, ws: ws, agents: map[string]proto.AgentSummary{}, began: map[string]bool{}}
	if _, err := c.call("hello.ok", map[string]any{"type": "hello", "clientId": "anywhere", "protocolVersion": proto.MaxVersion}); err != nil {
		ws.CloseNow()
		return nil, err
	}
	for !c.listed {
		if _, _, _, err := c.read(ctx); err != nil {
			ws.CloseNow()
			return nil, err
		}
	}
	return c, nil
}

func (c *client) close() { c.ws.CloseNow() }

// read returns pocketd's next message, keeping the agents up to date.
func (c *client) read(ctx context.Context) (kind, id string, raw []byte, err error) {
	if _, raw, err = c.ws.Read(ctx); err != nil {
		return "", "", nil, err
	}
	var head struct {
		Type string `json:"type"`
		ID   string `json:"id"`
	}
	if err := json.Unmarshal(raw, &head); err != nil {
		return "", "", nil, err
	}
	switch head.Type {
	case "agent.list":
		var l proto.AgentList
		if err := json.Unmarshal(raw, &l); err != nil {
			return "", "", nil, err
		}
		for _, a := range l.Agents {
			c.saw(a)
		}
		c.listed = true
	case "agent.update":
		var u proto.AgentUpdate
		if err := json.Unmarshal(raw, &u); err != nil {
			return "", "", nil, err
		}
		c.saw(u.Agent)
	}
	return head.Type, head.ID, raw, nil
}

func (c *client) saw(a proto.AgentSummary) {
	c.agents[a.ID] = a
	if a.Status == "working" || a.Status == "needsYou" {
		c.began[a.ID] = true
	}
}

// call sends m under a fresh id and returns the reply of type want. A verb may
// send others under that id first, as agent.create does.
func (c *client) call(want string, m map[string]any) ([]byte, error) {
	c.calls++
	id := strconv.Itoa(c.calls)
	m["id"] = id
	raw, err := json.Marshal(m)
	if err != nil {
		return nil, err
	}
	if err := c.ws.Write(c.ctx, websocket.MessageText, raw); err != nil {
		return nil, err
	}
	for {
		kind, got, raw, err := c.read(c.ctx)
		if err != nil {
			return nil, err
		}
		switch {
		case got != id:
		case kind == want:
			return raw, nil
		case kind == "error":
			var e proto.Error
			if err := json.Unmarshal(raw, &e); err != nil {
				return nil, err
			}
			return nil, refusal(e)
		}
	}
}

func refusal(e proto.Error) error {
	if e.Code == proto.CodeScopeDenied {
		return errors.New("a process inside an Anywhere terminal can only read sessions; run this from outside Anywhere")
	}
	return errors.New(e.Message)
}
