package reach

import (
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"reflect"
	"sync"
	"testing"
)

func addrs(ss ...string) []netip.Addr {
	var out []netip.Addr
	for _, s := range ss {
		out = append(out, netip.MustParseAddr(s))
	}
	return out
}

func TestTailnetAddrsComeOnlyFromTailscaleInterfaces(t *testing.T) {
	got := TailnetAddrs([]Iface{
		{"en0", addrs("192.168.1.5", "100.70.0.1", "fd7a:115c:a1e0::2")},
		{"utun4", addrs("100.77.122.82", "fe80::1", "0.0.0.0")},
		{"utun5", addrs("100.77.122.82")},
		{"lo0", addrs("127.0.0.1", "::1")},
	})
	if want := addrs("100.77.122.82", "fd7a:115c:a1e0::2"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

type fakeNet struct {
	mu     sync.Mutex
	ifaces []Iface
	cli    []netip.Addr
	refuse map[string]bool
	opened map[string]net.Listener
}

func (f *fakeNet) listen(_, addr string) (net.Listener, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.refuse[addr] {
		return nil, errors.New("can't assign requested address")
	}
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	f.opened[addr] = ln
	return ln, err
}

func start(t *testing.T, f *fakeNet, mode string) *Listener {
	t.Helper()
	f.opened = map[string]net.Listener{}
	l, err := listen(4517, mode, http.NotFoundHandler(), func() []Iface { return f.ifaces }, func() []netip.Addr { return f.cli }, f.listen)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { l.Close() })
	return l
}

func addrStrings(l *Listener) []string {
	var out []string
	for _, a := range l.Addrs() {
		out = append(out, a.String())
	}
	return out
}

func TestRefreshFollowsTheTailnetAddress(t *testing.T) {
	f := &fakeNet{ifaces: []Iface{{"utun4", addrs("100.77.122.82")}}}
	l := start(t, f, "auto")
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"100.77.122.82:4517", "127.0.0.1:4517"}) || !l.Tailnet() {
		t.Fatalf("%v", got)
	}
	f.ifaces = []Iface{{"utun4", addrs("100.77.122.99")}}
	l.Refresh()
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"100.77.122.99:4517", "127.0.0.1:4517"}) {
		t.Fatalf("%v", got)
	}
	if c, err := net.Dial("tcp", f.opened["100.77.122.82:4517"].Addr().String()); err == nil {
		c.Close()
		t.Fatal("the old tailnet listener is still open")
	}
	f.ifaces = nil
	l.Refresh()
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"127.0.0.1:4517"}) || l.Tailnet() {
		t.Fatalf("%v", got)
	}
}

func TestLoopbackModeNeverBindsTheTailnet(t *testing.T) {
	f := &fakeNet{ifaces: []Iface{{"utun4", addrs("100.77.122.82")}}}
	l := start(t, f, "loopback")
	l.Refresh()
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"127.0.0.1:4517"}) {
		t.Fatalf("%v", got)
	}
}

func TestACLIAddressCountsOnlyIfItBinds(t *testing.T) {
	f := &fakeNet{cli: addrs("100.77.122.82"), refuse: map[string]bool{"100.77.122.82:4517": true}}
	if l := start(t, f, "auto"); l.Tailnet() {
		t.Fatal("bound a stale CLI address")
	}
	f = &fakeNet{cli: addrs("100.77.122.82", "192.168.1.5")}
	if got := addrStrings(start(t, f, "auto")); !reflect.DeepEqual(got, []string{"100.77.122.82:4517", "127.0.0.1:4517"}) {
		t.Fatalf("%v", got)
	}
}

func TestPairHostIsTheFirstTailnetIPv4(t *testing.T) {
	f := &fakeNet{ifaces: []Iface{{"utun4", addrs("fd7a:115c:a1e0::2", "100.77.122.82")}}}
	if host, ok := start(t, f, "auto").PairHost(); !ok || host != "100.77.122.82:4517" {
		t.Fatalf("%q %v", host, ok)
	}
	if _, ok := start(t, &fakeNet{}, "auto").PairHost(); ok {
		t.Fatal("pair host without a tailnet")
	}
}

func TestOnlyLocalAndTailnetHostsGetThrough(t *testing.T) {
	l := start(t, &fakeNet{ifaces: []Iface{{"utun4", addrs("100.77.122.82", "fd7a:115c:a1e0::2")}}}, "auto")
	for host, want := range map[string]bool{
		"127.0.0.1:4517":            true,
		"localhost:4517":            true,
		"[::1]:4517":                true,
		"100.77.122.82:4517":        true,
		"[fd7a:115c:a1e0::2]:4517":  true,
		"mac.tail1234.ts.net:4517":  true,
		"MAC.tail1234.ts.net.:4517": true,
		"100.77.122.83:4517":        false,
		"evil.example:4517":         false,
		"ts.net:4517":               false,
		"192.168.1.5:4517":          false,
		"":                          false,
	} {
		if got := l.AllowHost(host); got != want {
			t.Errorf("%q: got %v", host, got)
		}
		rec := httptest.NewRecorder()
		r := httptest.NewRequest("GET", "/", nil)
		r.Host = host
		l.gate().ServeHTTP(rec, r)
		if blocked := rec.Code == http.StatusForbidden; blocked == want {
			t.Errorf("%q: status %d", host, rec.Code)
		}
	}
}

func TestLoopbackModeClosesTheTailnetAndAutoBindsItAgain(t *testing.T) {
	f := &fakeNet{ifaces: []Iface{{"utun4", addrs("100.77.122.82")}}}
	l := start(t, f, "auto")
	l.SetMode("loopback")
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"127.0.0.1:4517"}) || l.Tailnet() || l.Mode() != "loopback" {
		t.Fatalf("%v", got)
	}
	l.Refresh()
	if l.Tailnet() {
		t.Fatal("a recheck bound the tailnet in loopback mode")
	}
	l.SetMode("auto")
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"100.77.122.82:4517", "127.0.0.1:4517"}) {
		t.Fatalf("%v", got)
	}
}

func TestMovingToAPortThatWontBindKeepsTheOldListener(t *testing.T) {
	f := &fakeNet{ifaces: []Iface{{"utun4", addrs("100.77.122.82")}}, refuse: map[string]bool{"127.0.0.1:4600": true}}
	l := start(t, f, "auto")
	if _, err := l.Move(4600); err == nil {
		t.Fatal("moved to a port that won't bind")
	}
	if got := addrStrings(l); !reflect.DeepEqual(got, []string{"100.77.122.82:4517", "127.0.0.1:4517"}) {
		t.Fatalf("%v", got)
	}
	next, err := l.Move(4601)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { next.Close() })
	if got := addrStrings(next); !reflect.DeepEqual(got, []string{"100.77.122.82:4601", "127.0.0.1:4601"}) || next.Port() != 4601 {
		t.Fatalf("%v", got)
	}
	if c, err := net.Dial("tcp", f.opened["127.0.0.1:4517"].Addr().String()); err == nil {
		c.Close()
		t.Fatal("the old loopback listener is still open")
	}
}
