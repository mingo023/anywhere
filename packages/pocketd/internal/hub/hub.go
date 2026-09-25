// Package hub fans server messages out to every connected phone.
package hub

import (
	"encoding/json"
	"sync"
)

const buffer = 1024

type Hub struct {
	mu   sync.Mutex
	subs map[chan []byte]bool
}

func New() *Hub { return &Hub{subs: map[chan []byte]bool{}} }

func (h *Hub) Subscribe() (<-chan []byte, func()) {
	ch := make(chan []byte, buffer)
	h.mu.Lock()
	h.subs[ch] = true
	h.mu.Unlock()
	return ch, func() { h.drop(ch) }
}

func (h *Hub) drop(ch chan []byte) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.subs[ch] {
		delete(h.subs, ch)
		close(ch)
	}
}

// Publish never blocks: a subscriber that fell a whole buffer behind is
// dropped, and its phone reconnects and pages the timeline again.
func (h *Hub) Publish(msg any) {
	raw, err := json.Marshal(msg)
	if err != nil {
		panic(err)
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	for ch := range h.subs {
		select {
		case ch <- raw:
		default:
			delete(h.subs, ch)
			close(ch)
		}
	}
}
