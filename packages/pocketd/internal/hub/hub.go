// Package hub fans server messages out to every connected phone.
package hub

import (
	"encoding/json"
	"sync"
)

const buffer = 1024

type Hub struct {
	mu sync.Mutex
	// subs maps each subscriber to whether it takes sub-agents' items.
	subs map[chan []byte]bool
}

func New() *Hub { return &Hub{subs: map[chan []byte]bool{}} }

func (h *Hub) Subscribe() (<-chan []byte, func()) { return h.subscribe(false) }

// SubscribeSubagents also takes what PublishSubagent sends.
func (h *Hub) SubscribeSubagents() (<-chan []byte, func()) { return h.subscribe(true) }

func (h *Hub) subscribe(subagents bool) (<-chan []byte, func()) {
	ch := make(chan []byte, buffer)
	h.mu.Lock()
	h.subs[ch] = subagents
	h.mu.Unlock()
	return ch, func() { h.drop(ch) }
}

func (h *Hub) drop(ch chan []byte) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if _, ok := h.subs[ch]; ok {
		delete(h.subs, ch)
		close(ch)
	}
}

// Publish never blocks: a subscriber that fell a whole buffer behind is
// dropped, and its phone reconnects and pages the timeline again.
func (h *Hub) Publish(msg any) { h.publish(msg, false) }

// PublishSubagent sends a sub-agent's item only to SubscribeSubagents, so
// clients that predate them don't show its work as the agent's own.
func (h *Hub) PublishSubagent(msg any) { h.publish(msg, true) }

func (h *Hub) publish(msg any, subagent bool) {
	raw, err := json.Marshal(msg)
	if err != nil {
		panic(err)
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	for ch, subagents := range h.subs {
		if subagent && !subagents {
			continue
		}
		select {
		case ch <- raw:
		default:
			delete(h.subs, ch)
			close(ch)
		}
	}
}
