// Package broker holds approvals open until the phone or the desktop answers.
package broker

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"maps"
	"slices"
	"sync"
	"time"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

var Timeout = 10 * time.Minute

type pending struct {
	seq    int
	req    proto.PermissionRequest
	key    string
	answer chan Answer
}

// Answer is the phone's reply. Option is one of the request's Options;
// Message is feedback sent with a deny.
type Answer struct {
	Decision, Option, Message string
}

type Broker struct {
	mu   sync.Mutex
	hub  *hub.Hub
	seq  int
	open map[string]*pending
}

// newRequestID is random so a phone can't answer a request it was never shown.
func newRequestID() string {
	b := make([]byte, 16)
	rand.Read(b)
	return hex.EncodeToString(b)
}

func New(h *hub.Hub) *Broker { return &Broker{hub: h, open: map[string]*pending{}} }

// Ask shows req on every phone and returns its answer, whose Decision is
// "allow" or "deny", or "" when the desktop answered first (see Dismiss).
// key identifies the same request as seen by the provider, so Dismiss can
// find it. Ask assigns req.RequestID.
func (b *Broker) Ask(ctx context.Context, req proto.PermissionRequest, key string) Answer {
	b.mu.Lock()
	b.seq++
	req.RequestID = newRequestID()
	p := &pending{seq: b.seq, req: req, key: key, answer: make(chan Answer, 1)}
	b.open[p.req.RequestID] = p
	// Publishing under the lock keeps a phone from seeing or resolving the
	// request before every other phone has been told about it.
	b.hub.Publish(proto.NewPermissionRequest(p.req))
	b.mu.Unlock()

	timer := time.NewTimer(Timeout)
	defer timer.Stop()
	select {
	case d := <-p.answer:
		return d
	case <-ctx.Done():
		return b.giveUp(p, "")
	case <-timer.C:
		return b.giveUp(p, "deny")
	}
}

// giveUp denies p unless an answer already won the race to close it.
func (b *Broker) giveUp(p *pending, decision string) Answer {
	if _, ok := b.finish(p.req.RequestID, "deny"); !ok {
		return <-p.answer
	}
	return Answer{Decision: decision}
}

// finish closes the request for every phone; false if it was already closed.
func (b *Broker) finish(requestID, decision string) (*pending, bool) {
	b.mu.Lock()
	p, ok := b.open[requestID]
	delete(b.open, requestID)
	b.mu.Unlock()
	if ok {
		b.hub.Publish(proto.NewPermissionResolved(requestID, decision))
	}
	return p, ok
}

func (b *Broker) Resolve(requestID string, a Answer) bool {
	p, ok := b.finish(requestID, a.Decision)
	if ok {
		p.answer <- a
	}
	return ok
}

// Dismiss closes the oldest open request with key, which the desktop
// already answered; its Ask returns "".
func (b *Broker) Dismiss(key, decision string) {
	for _, id := range b.match(func(p *pending) bool { return p.key == key }) {
		if p, ok := b.finish(id, decision); ok {
			p.answer <- Answer{}
			return
		}
	}
}

func (b *Broker) DenyAll(agentID string) {
	for _, id := range b.match(func(p *pending) bool { return p.req.AgentID == agentID }) {
		b.Resolve(id, Answer{Decision: "deny"})
	}
}

func (b *Broker) match(ok func(*pending) bool) []string {
	var ids []string
	for _, p := range b.sorted() {
		if ok(p) {
			ids = append(ids, p.req.RequestID)
		}
	}
	return ids
}

func (b *Broker) Open() []proto.PermissionRequest {
	out := []proto.PermissionRequest{}
	for _, p := range b.sorted() {
		out = append(out, p.req)
	}
	return out
}

func (b *Broker) sorted() []*pending {
	b.mu.Lock()
	all := slices.Collect(maps.Values(b.open))
	b.mu.Unlock()
	slices.SortFunc(all, func(x, y *pending) int { return x.seq - y.seq })
	return all
}
