package channel

import (
	"crypto/ed25519"
	"errors"
	"io"
	"net"
	"os"
	"testing"
	"time"

	"cloudspike/members"
)

type device struct {
	key ed25519.PrivateKey
	m   members.Member
}

func newDevice(kind members.Kind) device {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return device{priv, members.Member{Key: pub, Kind: kind}}
}

func view(t *testing.T, l members.Log) *members.View {
	t.Helper()
	v, err := members.NewView(l)
	if err != nil {
		t.Fatal(err)
	}
	return v
}

type result struct {
	c   *Conn
	err error
}

// tcpPair is a loopback TCP pair. net.Pipe has no buffer, so a TLS close
// alert would block until the other end reads.
func tcpPair() (net.Conn, net.Conn) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	defer ln.Close()
	b, err := net.Dial("tcp", ln.Addr().String())
	if err != nil {
		panic(err)
	}
	a, err := ln.Accept()
	if err != nil {
		panic(err)
	}
	return a, b
}

func link(host device, hv *members.View, client device, cv *members.View) (server, cl result) {
	a, b := tcpPair()
	done := make(chan result)
	go func() {
		c, err := Server(a, host.key, hv)
		if err != nil {
			a.Close()
		}
		done <- result{c, err}
	}()
	c, err := Client(b, client.key, cv, host.m.Key)
	if err != nil {
		b.Close()
	}
	return <-done, result{c, err}
}

func TestAMemberReachesTheMachineAndBytesPassUnchanged(t *testing.T) {
	mac, host := newDevice(members.Desktop), newDevice(members.Machine)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	s, c := link(host, view(t, l), mac, view(t, l))
	if s.err != nil || c.err != nil {
		t.Fatal(s.err, c.err)
	}
	go c.c.Write([]byte(`{"type":"hello"}`))
	buf := make([]byte, 16)
	if _, err := io.ReadFull(s.c, buf); err != nil || string(buf) != `{"type":"hello"}` {
		t.Fatalf("got %q, %v", buf, err)
	}
	if s.c.Peer.Kind != members.Desktop {
		t.Fatalf("peer kind %q", s.c.Peer.Kind)
	}
}

func TestARemovedMemberCantReachAMachineThatHeard(t *testing.T) {
	mac, host, phone := newDevice(members.Desktop), newDevice(members.Machine), newDevice(members.Phone)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m).Add(mac.key, phone.m)
	s, c := link(host, view(t, l.Remove(mac.key, phone.m.Key)), phone, view(t, l))
	if s.err == nil || c.err == nil {
		t.Fatal("removed phone got a channel")
	}
}

func TestAResetAuthorCantReachAMachineInTheOldList(t *testing.T) {
	mac, host, thief := newDevice(members.Desktop), newDevice(members.Machine), newDevice(members.Desktop)
	old := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	reset := members.Genesis(thief.key, thief.m, nil).Add(thief.key, host.m)
	s, _ := link(host, view(t, old), thief, view(t, reset))
	if !errors.Is(s.err, members.ErrNotMember) {
		t.Fatalf("server err = %v, want ErrNotMember", s.err)
	}
}

func TestAMemberWontTalkToAMachineItDidNotAskFor(t *testing.T) {
	mac, host, other := newDevice(members.Desktop), newDevice(members.Machine), newDevice(members.Machine)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m).Add(mac.key, other.m)
	a, b := tcpPair()
	go func() {
		Server(a, other.key, view(t, l))
		a.Close()
	}()
	if _, err := Client(b, mac.key, view(t, l), host.m.Key); !errors.Is(err, ErrWrongPeer) {
		t.Fatalf("err = %v, want ErrWrongPeer", err)
	}
}

func TestTheNextMemberCarriesAHiddenRemovalAndCutsTheRemovedOne(t *testing.T) {
	mac, host, phone := newDevice(members.Desktop), newDevice(members.Machine), newDevice(members.Phone)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m).Add(mac.key, phone.m)
	hv := view(t, l)
	ps, pc := link(host, hv, phone, view(t, l))
	if ps.err != nil || pc.err != nil {
		t.Fatal(ps.err, pc.err)
	}
	if _, mc := link(host, hv, mac, view(t, l.Remove(mac.key, phone.m.Key))); mc.err != nil {
		t.Fatal(mc.err)
	}
	ps.c.SetReadDeadline(time.Now().Add(2 * time.Second))
	if _, err := ps.c.Read(make([]byte, 1)); err == nil || errors.Is(err, os.ErrDeadlineExceeded) {
		t.Fatalf("phone's channel still open: %v", err)
	}
}

func TestAChannelClosesWhenItsPeerIsReAddedAsAnotherKind(t *testing.T) {
	mac, host := newDevice(members.Desktop), newDevice(members.Machine)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	hv := view(t, l)
	s, c := link(host, hv, mac, view(t, l))
	if s.err != nil || c.err != nil {
		t.Fatal(s.err, c.err)
	}
	demoted := mac.m
	demoted.Kind = members.Phone
	if err := hv.Accept(l.Add(mac.key, demoted)); err != nil {
		t.Fatal(err)
	}
	s.c.SetReadDeadline(time.Now().Add(2 * time.Second))
	if _, err := s.c.Read(make([]byte, 1)); err == nil || errors.Is(err, os.ErrDeadlineExceeded) {
		t.Fatalf("channel still open after the peer became a phone: %v", err)
	}
}

func TestPeerIsTheMemberAsTheMergedLogHasIt(t *testing.T) {
	mac, host := newDevice(members.Desktop), newDevice(members.Machine)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	demoted := mac.m
	demoted.Kind = members.Phone
	s, c := link(host, view(t, l), mac, view(t, l.Add(mac.key, demoted)))
	if s.err != nil || c.err != nil {
		t.Fatal(s.err, c.err)
	}
	if s.c.Peer.Kind != members.Phone {
		t.Fatalf("peer kind = %q, want phone", s.c.Peer.Kind)
	}
}
