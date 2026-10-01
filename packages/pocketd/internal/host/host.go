// Package host holds what the phone shows about the Mac itself.
package host

import (
	"sync"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

type Monitor struct {
	mu    sync.Mutex
	state proto.HostState
	hub   *hub.Hub
}

func NewMonitor() *Monitor { return &Monitor{hub: hub.New()} }

func (m *Monitor) State() proto.HostState {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.state
}

// Set applies f and publishes host.changed if the state moved.
func (m *Monitor) Set(f func(*proto.HostState)) {
	m.mu.Lock()
	defer m.mu.Unlock()
	next := m.state
	f(&next)
	if next == m.state {
		return
	}
	m.state = next
	m.hub.Publish(proto.NewHostChanged(next))
}

func (m *Monitor) Subscribe() (<-chan []byte, func()) { return m.hub.Subscribe() }
