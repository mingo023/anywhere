package ops

import (
	"strings"
	"testing"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/pairing"
)

func withPairing(t *testing.T, host string) *Server {
	t.Helper()
	srv, _ := withDevices(t)
	srv.Pairing = pairing.New(time.Now)
	srv.MacName = "Mac mini"
	srv.Host = func() (string, bool) { return host, host != "" }
	return srv
}

func TestPairBeginHandsOutACodeAndReportsThePhone(t *testing.T) {
	srv := withPairing(t, "100.77.122.82:4517")
	c := start(t, srv)
	c.Send(Msg{Op: "pair.begin"})
	offer := recv(t, c, "pair.begin").Pair
	if offer == nil || len(offer.Code) != 22 {
		t.Fatalf("%+v", offer)
	}
	d, _, err := srv.Pairing.Redeem(offer.Code, func() (devices.Device, string, error) {
		return srv.Devices.Add("iPhone", "ios", devices.PhoneScopes)
	})
	if err != nil {
		t.Fatal(err)
	}
	if m := recv(t, c, "pair.ok"); m.ID != d.ID || m.Text != "iPhone" {
		t.Fatalf("%+v", m)
	}
}

func TestAnExplicitHostWinsOverTheTailnet(t *testing.T) {
	c := start(t, withPairing(t, ""))
	c.Send(Msg{Op: "pair.begin", Text: "127.0.0.1:4517"})
	if offer := recv(t, c, "pair.begin").Pair; offer == nil || !strings.Contains(offer.URL, "h=127.0.0.1%3A4517") {
		t.Fatalf("%+v", offer)
	}
}

func TestPairBeginNeedsTheTailnet(t *testing.T) {
	c := start(t, withPairing(t, ""))
	c.Send(Msg{Op: "pair.begin"})
	if m := recv(t, c, "error"); m.ErrorCode != "tailnet_off" || m.Error != "Tailscale isn't running. Phones can't reach this Mac." {
		t.Fatalf("%+v", m)
	}
}
