package wsserver

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/devices"
	"pocketd/internal/pairing"
	"pocketd/internal/peer"
)

func pairMsg(code string) string {
	return `{"type":"pair","id":"p","code":"` + code + `","name":"  Work phone ","platform":"ios","protocol":{"min":3,"max":3}}`
}

func TestAPairCodeBuysATokenThatSaysHello(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) { s = srv })
	offer, done, _ := s.Pairing.Begin("h:1", "Mac")
	p.send(pairMsg(offer.Code))
	m := p.recv()
	if m["type"] != "pair.ok" || m["id"] != "p" || len(m["token"].(string)) != 43 {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != websocket.StatusNormalClosure {
		t.Fatalf("%v", ce)
	}
	if r := <-done; r.DeviceID != m["deviceId"] || r.Name != "Work phone" {
		t.Fatalf("%+v", r)
	}
	if d := s.Devices.List()[1]; d.Name != "Work phone" || d.Platform != "ios" {
		t.Fatalf("%+v", d)
	}
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	p.ws = ws
	p.send(`{"type":"hello","id":"h","token":"` + m["token"].(string) + `","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["type"] != "hello.ok" {
		t.Fatalf("%v", m)
	}
}

func TestAWrongCodeLeavesTheSocketOpenForTheRightOne(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) { s = srv })
	offer, _, _ := s.Pairing.Begin("h:1", "Mac")
	p.send(pairMsg("AAAAAAAAAAAAAAAAAAAAAA"))
	if m := p.recv(); m["type"] != "error" || m["code"] != "pair_invalid" || m["message"] != pairing.ErrInvalid.Error() {
		t.Fatalf("%v", m)
	}
	p.send(pairMsg(offer.Code))
	if m := p.recv(); m["type"] != "pair.ok" {
		t.Fatalf("%v", m)
	}
}

func TestThreeBadCodesLockTheAddressOut(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) { s = srv })
	s.Pairing.Begin("h:1", "Mac")
	for range 2 {
		p.send(pairMsg("wrong"))
		if m := p.recv(); m["code"] != "pair_invalid" {
			t.Fatalf("%v", m)
		}
	}
	p.send(pairMsg("wrong"))
	if m := p.recv(); m["code"] != "rate_limited" {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != websocket.StatusPolicyViolation {
		t.Fatalf("%v", ce)
	}
}

func TestAFailedSaveDoesntLockThePhoneOut(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) {
		srv.Devices, _ = devices.Open(filepath.Join(t.TempDir(), "missing", "devices.json"), "tok")
		s = srv
	})
	for range 3 {
		offer, _, _ := s.Pairing.Begin("h:1", "Mac")
		p.send(pairMsg(offer.Code))
		if m := p.recv(); m["type"] != "error" || m["code"] != nil || m["message"] != "Pairing failed. Make a new code on your Mac." {
			t.Fatalf("%v", m)
		}
	}
	p.hello()
}

func TestPairAfterHelloIsRefused(t *testing.T) {
	_, _, p := setup(t)
	p.hello()
	p.send(pairMsg("x"))
	if m := p.recv(); m["type"] != "error" || m["message"] != "Already authenticated" {
		t.Fatalf("%v", m)
	}
}

func TestAnOutdatedPhoneCantPair(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"pair","id":"p","code":"c","name":"n","platform":"android","protocol":{"min":1,"max":2}}`)
	if m := p.recv(); m["code"] != "client_too_old" {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != 4426 || ce.Reason != "client_too_old" {
		t.Fatalf("%v", ce)
	}
}

func TestPairDoneReachesOnlyTheConnThatBeganPairing(t *testing.T) {
	var s *Server
	_, _, began := local(t, peer.OwnerOf(1), func(srv *Server) {
		srv.Host = func() (string, bool) { return "100.64.0.1:4517", true }
		s = srv
	})
	ws, _, err := began.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	other := &phone{t: t, ws: ws, url: began.url}
	helloAs(began)
	helloAs(other)
	began.send(`{"type":"pair.begin","id":"b"}`)
	offer := began.recv()
	if offer["type"] != "pair.offer" || offer["id"] != "b" {
		t.Fatalf("%v", offer)
	}
	s.Pairing.Redeem(offer["code"].(string), func() (devices.Device, string, error) {
		return s.Devices.Add("iPhone", "ios", devices.PhoneScopes)
	})
	if m := began.recv(); m["type"] != "pair.done" || m["name"] != "iPhone" {
		t.Fatalf("%v", m)
	}
	other.send(`{"type":"agent.list","id":"l"}`)
	if m := other.recv(); m["type"] != "agent.list" {
		t.Fatalf("other conn got %v", m)
	}
}

func TestPairingWithoutTailscaleIsTailnetOff(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1))
	helloAs(p)
	p.send(`{"type":"pair.begin","id":"b"}`)
	if m := p.recv(); m["code"] != "tailnet_off" || m["id"] != "b" {
		t.Fatalf("%v", m)
	}
}

func TestAPhoneCantBeginPairing(t *testing.T) {
	_, _, p := setup(t, func(srv *Server) { srv.Host = func() (string, bool) { return "100.64.0.1:4517", true } })
	p.hello()
	p.send(`{"type":"pair.begin","id":"b"}`)
	if m := p.recv(); m["code"] != "scope_denied" {
		t.Fatalf("%v", m)
	}
}

func TestASupersededCodeTellsTheDesktop(t *testing.T) {
	var s *Server
	_, _, p := local(t, peer.OwnerOf(1), func(srv *Server) {
		srv.Host = func() (string, bool) { return "100.64.0.1:4517", true }
		s = srv
	})
	helloAs(p)
	p.send(`{"type":"pair.begin","id":"b"}`)
	p.recv()
	s.Pairing.Begin("100.64.0.1:4517", "Mac")
	if m := p.recv(); m["type"] != "error" || m["id"] != "b" || m["code"] != "pair_expired" || m["message"] != "Code expired." {
		t.Fatalf("%v", m)
	}
}

func TestBeginningAgainOnOneConnSendsOnlyTheNewOffer(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1), func(srv *Server) {
		srv.Host = func() (string, bool) { return "100.64.0.1:4517", true }
	})
	helloAs(p)
	p.send(`{"type":"pair.begin","id":"b1"}`)
	first := p.recv()
	p.send(`{"type":"pair.begin","id":"b2"}`)
	if m := p.recv(); m["type"] != "pair.offer" || m["id"] != "b2" || m["code"] == first["code"] {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.recv(); m["type"] != "agent.list" {
		t.Fatalf("after the new offer: %v", m)
	}
}

func TestACodeThatRunsOutTellsTheDesktop(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1), func(srv *Server) {
		srv.Host = func() (string, bool) { return "100.64.0.1:4517", true }
		srv.Pairing = pairing.New(func() time.Time { return time.Now().Add(100*time.Millisecond - 5*time.Minute) })
	})
	helloAs(p)
	p.send(`{"type":"pair.begin","id":"b"}`)
	p.recv()
	if m := p.recv(); m["type"] != "error" || m["id"] != "b" || m["code"] != "pair_expired" {
		t.Fatalf("%v", m)
	}
}
