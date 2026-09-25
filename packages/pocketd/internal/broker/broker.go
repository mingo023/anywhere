// Package broker holds approvals open until the phone or the desktop answers.
package broker

import (
	"context"
	"maps"
	"slices"
	"strconv"
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
	answer chan string
}

type Broker struct {
	mu   sync.Mutex
	hub  *hub.Hub
	seq  int
	open map[string]*pending
}

func New(h *hub.Hub) *Broker { return &Broker{hub: h, open: map[string]*pending{}} }

// Ask shows the request on every phone and returns "allow" or "deny", or ""
// when the desktop answered first (see Dismiss). key identifies the same
// request as seen by the provider, so Dismiss can find it.
func (b *Broker) Ask(ctx context.Context, agentID, toolName string, detail proto.ToolDetail, key string) string {
	b.mu.Lock()
	b.seq++
	p := &pending{
		seq:    b.seq,
		req:    proto.PermissionRequest{RequestID: "perm-" + strconv.Itoa(b.seq), AgentID: agentID, ToolName: toolName, Detail: detail},
		key:    key,
		answer: make(chan string, 1),
	}
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
func (b *Broker) giveUp(p *pending, decision string) string {
	if _, ok := b.finish(p.req.RequestID, "deny"); !ok {
		return <-p.answer
	}
	return decision
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

func (b *Broker) Resolve(requestID, decision string) bool {
	p, ok := b.finish(requestID, decision)
	if ok {
		p.answer <- decision
	}
	return ok
}

// Dismiss closes the oldest open request with key, which the desktop
// already answered; its Ask returns "".
func (b *Broker) Dismiss(key, decision string) {
	for _, id := range b.match(func(p *pending) bool { return p.key == key }) {
		if p, ok := b.finish(id, decision); ok {
			p.answer <- ""
			return
		}
	}
}

func (b *Broker) DenyAll(agentID string) {
	for _, id := range b.match(func(p *pending) bool { return p.req.AgentID == agentID }) {
		b.Resolve(id, "deny")
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
