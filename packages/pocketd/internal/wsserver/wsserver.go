// Package wsserver speaks the phone protocol (packages/protocol) over WebSocket.
package wsserver

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/netip"
	"slices"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/devices"
	"pocketd/internal/events"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/pairing"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

const (
	defaultPage         = 200
	defaultHelloTimeout = 10 * time.Second
	defaultPingInterval = 20 * time.Second
	defaultPingTimeout  = 10 * time.Second

	statusVersionMismatch websocket.StatusCode = 4426
	statusUnpaired        websocket.StatusCode = 4401

	maxPreauth       = 16
	preauthReadLimit = 4 << 10
	authedReadLimit  = 1 << 20
)

var updateCopy = map[string]string{
	proto.CodeClientTooOld: "Update Pocket on this phone",
	proto.CodeServerTooOld: "Update Pocket on your Mac",
}

type Server struct {
	Devices  *devices.Store
	Pairing  *pairing.Manager
	Hostname string
	Agents   *agent.Registry
	Broker   *broker.Broker
	Hub      *hub.Hub
	Monitor  *host.Monitor
	Events   *events.Log
	// HelloTimeout closes connections still unauthenticated after it; zero means defaultHelloTimeout.
	HelloTimeout time.Duration
	// Host is the tailnet ip:port a phone pairs with; false when Tailscale is off.
	Host    func() (string, bool)
	MacName string
	// AskOpen reports whether the agent in a Terminal waits on the user.
	AskOpen func(terminalID string) bool

	pingInterval, pingTimeout time.Duration
	conns                     atomic.Int64
	preauth                   atomic.Int64
	failures                  limiter

	mu       sync.Mutex
	byDevice map[string]map[*conn]bool
}

type conn struct {
	s        *Server
	key      string
	ws       *websocket.Conn
	ip       netip.Addr
	ctx      context.Context
	cancel   func()
	authed   bool
	onAuth   func()
	device   string
	who      peer.Principal
	offer    *openOffer
	stop     func()
	stopHost func()
}

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	ip := remoteIP(r)
	// Socket conns all have the zero address, so per-address limits would let
	// one PTY peer lock the owner out. The hello timeout still bounds them.
	_, local := peer.From(r.Context())
	if !local && s.failures.locked(ip, time.Now()) {
		http.Error(w, "Too many attempts", http.StatusTooManyRequests)
		return
	}
	authed := func() {}
	if !local {
		if s.preauth.Add(1) > maxPreauth {
			s.preauth.Add(-1)
			http.Error(w, "Busy", http.StatusServiceUnavailable)
			return
		}
		authed = sync.OnceFunc(func() { s.preauth.Add(-1) })
	}
	defer authed()
	ws, err := websocket.Accept(w, r, nil)
	if err != nil {
		return
	}
	ws.SetReadLimit(preauthReadLimit)
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()
	c := &conn{s: s, key: strconv.FormatInt(s.conns.Add(1), 10), ws: ws, ip: ip, ctx: ctx, cancel: cancel, onAuth: authed}
	defer s.Agents.DropView(c.key)
	defer s.forget(c)
	defer func() {
		if c.stop != nil {
			c.stop()
		}
		if c.stopHost != nil {
			c.stopHost()
		}
	}()
	go c.keepalive()
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

// keepalive closes a connection whose peer stopped answering, so a sleeping phone's view doesn't mark turns seen.
func (c *conn) keepalive() {
	for {
		select {
		case <-c.ctx.Done():
			return
		case <-time.After(cmp.Or(c.s.pingInterval, defaultPingInterval)):
		}
		ctx, cancel := context.WithTimeout(c.ctx, cmp.Or(c.s.pingTimeout, defaultPingTimeout))
		err := c.ws.Ping(ctx)
		cancel()
		if err != nil {
			c.ws.CloseNow()
			return
		}
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
		version, caps, code := proto.Negotiate(m.Versions(), m.Caps, proto.ServerCaps)
		if code != "" {
			c.tooOld(m.ID, code)
			return
		}
		who, local := peer.From(c.ctx)
		if !local {
			d, ok := c.s.Devices.Lookup(m.Token)
			if !ok {
				c.reject(proto.NewErrorCode(m.ID, proto.CodeNotPaired, "Not paired"), statusUnpaired, proto.CodeNotPaired)
				return
			}
			if c.authed && d.ID != c.device {
				c.reject(proto.NewErrorCode(m.ID, proto.CodeNotPaired, "Not paired"), statusUnpaired, proto.CodeNotPaired)
				return
			}
			if !c.authed {
				c.device = d.ID
				c.s.remember(c)
				// A revoke between Lookup and remember found nothing to close.
				if !c.s.Devices.Has(d.ID) {
					c.ws.Close(statusUnpaired, "revoked")
					return
				}
			}
			c.s.Devices.Seen(d.ID, c.ip.String(), time.Now())
			who = peer.FromDevice(d)
		}
		c.who = who
		// Subscribing before the snapshot means no update falls in the gap;
		// the channel buffers them until hello.ok and agent.list are out.
		var msgs <-chan []byte
		if !c.authed {
			msgs, c.stop = c.s.Hub.Subscribe()
			c.authed = true
			c.onAuth()
			c.ws.SetReadLimit(authedReadLimit)
		}
		ok := proto.NewHelloOK(m.ID, c.s.Hostname, version, caps)
		if slices.Contains(caps, proto.CapScopes) {
			ok.Scopes = c.who.Names()
		}
		hostMsgs := c.withHost(&ok)
		c.send(ok)
		c.send(proto.NewAgentList("", c.s.Agents.List()))
		for _, req := range c.s.Broker.Open() {
			c.send(proto.NewPermissionRequest(req))
		}
		if msgs != nil {
			go c.forward(msgs)
		}
		if hostMsgs != nil {
			go c.forward(hostMsgs)
		}
		return
	}
	if m.Type == "pair" {
		c.pair(m)
		return
	}
	if !c.authed {
		c.send(proto.NewError(m.ID, "Not authenticated"))
		return
	}
	if err := c.dispatch(m); err != nil {
		c.send(errorReply(m.ID, err))
	}
}

// withHost fills hello.ok.host for a phone that negotiated host.v1 and, on
// its first hello, subscribes it to host.changed before the state is read.
func (c *conn) withHost(reply *proto.HelloOK) <-chan []byte {
	if c.s.Monitor == nil || !slices.Contains(reply.Caps, proto.CapHost) {
		return nil
	}
	var msgs <-chan []byte
	if c.stopHost == nil {
		msgs, c.stopHost = c.s.Monitor.Subscribe()
	}
	state := c.s.Monitor.State()
	reply.Host = &state
	return msgs
}

func errorReply(id string, err error) proto.Error {
	if code := peer.CodeOf(err); code != "" {
		return proto.NewErrorCode(id, code, err.Error())
	}
	return proto.NewError(id, err.Error())
}

func (s *Server) remember(c *conn) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.byDevice == nil {
		s.byDevice = map[string]map[*conn]bool{}
	}
	if s.byDevice[c.device] == nil {
		s.byDevice[c.device] = map[*conn]bool{}
	}
	s.byDevice[c.device][c] = true
}

func (s *Server) forget(c *conn) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.byDevice[c.device], c)
	if len(s.byDevice[c.device]) == 0 {
		delete(s.byDevice, c.device)
	}
}

// CloseDevice closes every live socket of a device with 4401 and reason.
func (s *Server) CloseDevice(id, reason string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for c := range s.byDevice[id] {
		go c.ws.Close(statusUnpaired, reason)
	}
}

// reject answers a failed auth attempt and closes with status, or stays open
// when status is 0. The third failure from one address in a minute closes as
// rate_limited instead and locks the address out.
func (c *conn) reject(msg proto.Error, status websocket.StatusCode, reason string) {
	if _, local := peer.From(c.ctx); !local && c.s.failures.fail(c.ip, time.Now()) {
		msg, status, reason = proto.NewErrorCode(msg.ID, proto.CodeRateLimited, "Too many attempts"), websocket.StatusPolicyViolation, proto.CodeRateLimited
	}
	c.send(msg)
	if status != 0 {
		c.ws.Close(status, reason)
	}
}

func (c *conn) tooOld(id, code string) {
	c.send(proto.NewErrorCode(id, code, updateCopy[code]))
	c.ws.Close(statusVersionMismatch, code)
}

func (c *conn) asker(requestID string) string {
	for _, req := range c.s.Broker.Open() {
		if req.RequestID == requestID {
			return req.AgentID
		}
	}
	return ""
}

// emitSeen records Done → Seen; it runs before the view or seen clears Done.
func (c *conn) emitSeen(ids []string) {
	for _, id := range ids {
		if a, err := c.s.Agents.Get(id); err == nil && a.Summary().Status == "done" {
			c.s.Events.Emit(events.Event{Kind: "seen", Agent: id, Principal: c.principal()})
		}
	}
}

func (c *conn) principal() string {
	switch c.who.Kind {
	case peer.PTY:
		return "pty"
	case peer.Owner:
		return "owner"
	}
	return "device"
}

// asking names agentID's Terminal while it waits on an ask. Only a PTY
// peer's verbs are guarded, so others skip the lookup.
func (c *conn) asking(agentID string) string {
	if c.who.Kind != peer.PTY || c.s.AskOpen == nil {
		return ""
	}
	a, err := c.s.Agents.Get(agentID)
	if err != nil {
		return ""
	}
	if id := a.Summary().TerminalID; id != "" && c.s.AskOpen(id) {
		return id
	}
	return ""
}

func (c *conn) dispatch(m proto.ClientMessage) error {
	if r := c.who.Check("ws:"+m.Type, c.asking(m.AgentID), m.Text); r != nil {
		return r
	}
	switch m.Type {
	case "pair.begin":
		return c.pairBegin(m.ID)
	case "permission.resolve":
		agentID := c.asker(m.RequestID)
		if !c.s.Broker.Resolve(m.RequestID, broker.Answer{Decision: m.Decision, Option: m.Option, Message: m.Message}) {
			return errors.New("Permission request is no longer open")
		}
		c.s.Events.Emit(events.Event{Kind: "answer", Agent: agentID, Principal: c.principal(), Decision: m.Decision})
		c.send(proto.NewAck(m.ID))
		return nil
	case "agent.list":
		c.send(proto.NewAgentList(m.ID, c.s.Agents.List()))
		return nil
	case "agent.view":
		c.emitSeen(m.AgentIDs)
		c.s.Agents.SetView(c.key, m.AgentIDs)
		c.send(proto.NewAck(m.ID))
		return nil
	case "agent.seen":
		c.emitSeen(m.AgentIDs)
		c.s.Agents.MarkSeen(m.AgentIDs)
		c.send(proto.NewAck(m.ID))
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
