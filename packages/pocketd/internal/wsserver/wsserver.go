// Package wsserver speaks the phone protocol (packages/protocol) over WebSocket.
package wsserver

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

const (
	defaultPage         = 200
	defaultHelloTimeout = 10 * time.Second
)

type Server struct {
	Token    string
	Hostname string
	Agents   *agent.Registry
	Broker   *broker.Broker
	Hub      *hub.Hub
	// HelloTimeout closes connections still unauthenticated after it; zero means defaultHelloTimeout.
	HelloTimeout time.Duration
}

type conn struct {
	s      *Server
	ws     *websocket.Conn
	ctx    context.Context
	cancel func()
	authed bool
	stop   func()
}

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	ws, err := websocket.Accept(w, r, &websocket.AcceptOptions{InsecureSkipVerify: true})
	if err != nil {
		return
	}
	ws.SetReadLimit(1 << 20)
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()
	c := &conn{s: s, ws: ws, ctx: ctx, cancel: cancel}
	defer func() {
		if c.stop != nil {
			c.stop()
		}
	}()
	helloCtx, cancelHello := context.WithTimeout(ctx, cmp.Or(s.HelloTimeout, defaultHelloTimeout))
	defer cancelHello()
	for {
		readCtx := ctx
		if !c.authed {
			readCtx = helloCtx
		}
		_, raw, err := ws.Read(readCtx)
		if err != nil {
			ws.CloseNow()
			return
		}
		c.handle(raw)
	}
}

func (c *conn) forward(msgs <-chan []byte) {
	for raw := range msgs {
		if c.ws.Write(c.ctx, websocket.MessageText, raw) != nil {
			break
		}
	}
	c.cancel()
}

func (c *conn) send(msg any) {
	raw, _ := json.Marshal(msg)
	c.ws.Write(c.ctx, websocket.MessageText, raw)
}

func (c *conn) handle(raw []byte) {
	m, err := proto.DecodeClient(raw)
	if err != nil {
		c.send(proto.NewError(m.ID, err.Error()))
		return
	}
	if m.Type == "hello" {
		if m.Token != c.s.Token || m.ProtocolVersion != proto.Version {
			c.send(proto.NewError(m.ID, "Rejected"))
			c.ws.Close(websocket.StatusPolicyViolation, "rejected")
			return
		}
		// Subscribing before the snapshot means no update falls in the gap;
		// the channel buffers them until hello.ok and agent.list are out.
		var msgs <-chan []byte
		if !c.authed {
			msgs, c.stop = c.s.Hub.Subscribe()
			c.authed = true
		}
		c.send(proto.NewHelloOK(m.ID, c.s.Hostname))
		c.send(proto.NewAgentList("", c.s.Agents.List()))
		for _, req := range c.s.Broker.Open() {
			c.send(proto.NewPermissionRequest(req))
		}
		if msgs != nil {
			go c.forward(msgs)
		}
		return
	}
	if !c.authed {
		c.send(proto.NewError(m.ID, "Not authenticated"))
		return
	}
	if err := c.dispatch(m); err != nil {
		c.send(proto.NewError(m.ID, err.Error()))
	}
}

func (c *conn) dispatch(m proto.ClientMessage) error {
	switch m.Type {
	case "permission.resolve":
		if !c.s.Broker.Resolve(m.RequestID, m.Decision) {
			return errors.New("Permission request is no longer open")
		}
		c.send(proto.NewAck(m.ID))
		return nil
	case "agent.list":
		c.send(proto.NewAgentList(m.ID, c.s.Agents.List()))
		return nil
	}
	a, err := c.s.Agents.Get(m.AgentID)
	if err != nil {
		return err
	}
	switch m.Type {
	case "agent.timeline":
		since, limit := int64(0), defaultPage
		if m.SinceSeq != nil {
			since = int64(max(0, min(*m.SinceSeq, 1<<53)))
		}
		if m.Limit != nil {
			limit = int(*m.Limit)
		}
		items, older := a.Timeline.Page(since, limit)
		epoch, maxSeq := a.Timeline.State()
		c.send(proto.NewAgentTimeline(m.ID, a.ID(), epoch, items, older, maxSeq))
		return nil
	case "agent.prompt":
		err = a.Driver().Prompt(m.Text)
	case "agent.interrupt":
		err = a.Driver().Interrupt()
	case "agent.compact":
		err = a.Driver().Compact()
	case "agent.close":
		a.Driver().Close()
	}
	if err != nil {
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
