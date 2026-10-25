package host

import (
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/proto"
)

func TestTheStateFollowsWhetherPhonesMayUseTerminals(t *testing.T) {
	m := NewMonitor()
	s := config.NewSettings(t.TempDir())
	FollowTerminals(m, s)
	if !m.State().Terminals {
		t.Fatal("terminals start off")
	}
	msgs, stop := m.Subscribe()
	defer stop()
	if err := s.Set("phone.terminals", "off"); err != nil {
		t.Fatal(err)
	}
	if got, want := string(<-msgs), `{"type":"host.changed","host":{"tailnet":false,"keepingAwake":false,"terminals":false}}`; got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
	s.Set("awake.enabled", "false")
	s.Set("phone.terminals", "on")
	if got := string(<-msgs); got != `{"type":"host.changed","host":{"tailnet":false,"keepingAwake":false,"terminals":true}}` {
		t.Fatalf("got %s", got)
	}
}

func TestSetPublishesOnlyOnChange(t *testing.T) {
	m := NewMonitor()
	msgs, stop := m.Subscribe()
	defer stop()
	m.Set(func(h *proto.HostState) { h.Tailnet = true })
	m.Set(func(h *proto.HostState) { h.Tailnet = true })
	m.Set(func(h *proto.HostState) { h.KeepingAwake = true })
	want := []string{
		`{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false,"terminals":false}}`,
		`{"type":"host.changed","host":{"tailnet":true,"keepingAwake":true,"terminals":false}}`,
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
