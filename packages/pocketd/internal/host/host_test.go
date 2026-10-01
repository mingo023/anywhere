package host

import (
	"testing"

	"pocketd/internal/proto"
)

func TestSetPublishesOnlyOnChange(t *testing.T) {
	m := NewMonitor()
	msgs, stop := m.Subscribe()
	defer stop()
	m.Set(func(h *proto.HostState) { h.Tailnet = true })
	m.Set(func(h *proto.HostState) { h.Tailnet = true })
	m.Set(func(h *proto.HostState) { h.KeepingAwake = true })
	want := []string{
		`{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false}}`,
		`{"type":"host.changed","host":{"tailnet":true,"keepingAwake":true}}`,
	}
	for _, w := range want {
		if got := string(<-msgs); got != w {
			t.Fatalf("got %s, want %s", got, w)
		}
	}
	select {
	case raw := <-msgs:
		t.Fatalf("unexpected %s", raw)
	default:
	}
	if m.State() != (proto.HostState{Tailnet: true, KeepingAwake: true}) {
		t.Fatalf("state %+v", m.State())
	}
}
