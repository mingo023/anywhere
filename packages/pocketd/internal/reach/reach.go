// Package reach decides where pocketd listens for phones: loopback always,
// plus this Mac's Tailscale addresses, rechecked as Tailscale comes and goes.
package reach

import (
	"fmt"
	"net"
	"net/http"
	"net/netip"
	"os"
	"os/exec"
	"slices"
	"strings"
	"sync"
	"time"
)

const recheck = 30 * time.Second

var (
	cgnat    = netip.MustParsePrefix("100.64.0.0/10")
	tailnet6 = netip.MustParsePrefix("fd7a:115c:a1e0::/48")
)

// IsTailnet reports whether ip on iface is a Tailscale address. 100.64.0.0/10
// counts only on utun interfaces: elsewhere it is carrier-grade NAT.
func IsTailnet(ip netip.Addr, iface string) bool {
	ip = ip.Unmap()
	return tailnet6.Contains(ip) || cgnat.Contains(ip) && strings.HasPrefix(iface, "utun")
}

type Iface struct {
	Name  string
	Addrs []netip.Addr
}

func TailnetAddrs(ifaces []Iface) []netip.Addr {
	var out []netip.Addr
	for _, f := range ifaces {
		for _, a := range f.Addrs {
			if a = a.Unmap(); IsTailnet(a, f.Name) && !slices.Contains(out, a) {
				out = append(out, a)
			}
		}
	}
	slices.SortFunc(out, netip.Addr.Compare)
	return out
}

func systemIfaces() []Iface {
	nics, err := net.Interfaces()
	if err != nil {
		return nil
	}
	var out []Iface
	for _, nic := range nics {
		if nic.Flags&net.FlagUp == 0 {
			continue
		}
		f := Iface{Name: nic.Name}
		addrs, _ := nic.Addrs()
		for _, a := range addrs {
			if n, ok := a.(*net.IPNet); ok {
				if ip, ok := netip.AddrFromSlice(n.IP); ok {
					f.Addrs = append(f.Addrs, ip.Unmap())
				}
			}
		}
		out = append(out, f)
	}
	return out
}

// cliAddrs asks the tailscale CLI. It prints addresses even while Tailscale is
// stopped, so callers keep one only if binding it succeeds.
func cliAddrs() []netip.Addr {
	out, err := exec.Command("tailscale", "ip").Output()
	if err != nil {
		return nil
	}
	var addrs []netip.Addr
	for _, f := range strings.Fields(string(out)) {
		if a, err := netip.ParseAddr(f); err == nil {
			addrs = append(addrs, a)
		}
	}
	return addrs
}

type Listener struct {
	port    int
	auto    bool
	handler http.Handler
	ifaces  func() []Iface
	cli     func() []netip.Addr
	listen  func(network, addr string) (net.Listener, error)
	done    chan struct{}

	mu       sync.Mutex
	loopback net.Listener
	tailnet  map[netip.Addr]net.Listener
	closed   bool
}

// Listen serves h on 127.0.0.1:port and, in mode "auto", on every tailnet
// address, rechecked every 30 s. Requests whose Host isn't one of those
// addresses, localhost or a *.ts.net name get 403 before h sees them.
func Listen(port int, mode string, h http.Handler) (*Listener, error) {
	return listen(port, mode, h, systemIfaces, cliAddrs, net.Listen)
}

func listen(port int, mode string, h http.Handler, ifaces func() []Iface, cli func() []netip.Addr, listen func(string, string) (net.Listener, error)) (*Listener, error) {
	l := &Listener{port: port, auto: mode == "auto", handler: h, ifaces: ifaces, cli: cli, listen: listen, done: make(chan struct{}), tailnet: map[netip.Addr]net.Listener{}}
	ln, err := listen("tcp", netip.AddrPortFrom(netip.AddrFrom4([4]byte{127, 0, 0, 1}), uint16(port)).String())
	if err != nil {
		return nil, err
	}
	l.loopback = ln
	go http.Serve(ln, l.gate())
	if l.auto {
		l.Refresh()
		go l.poll()
	}
	return l, nil
}

func (l *Listener) poll() {
	t := time.NewTicker(recheck)
	defer t.Stop()
	for {
		select {
		case <-l.done:
			return
		case <-t.C:
			l.Refresh()
		}
	}
}

func (l *Listener) gate() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !l.AllowHost(r.Host) {
			http.Error(w, "Forbidden", http.StatusForbidden)
			return
		}
		l.handler.ServeHTTP(w, r)
	})
}

// Refresh binds tailnet addresses that appeared and closes those that went.
// Connections already accepted on a closed address live until they drop.
func (l *Listener) Refresh() {
	if !l.auto {
		return
	}
	want := TailnetAddrs(l.ifaces())
	fromCLI := len(want) == 0
	if fromCLI {
		for _, a := range l.cli() {
			if a = a.Unmap(); cgnat.Contains(a) || tailnet6.Contains(a) {
				want = append(want, a)
			}
		}
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return
	}
	for a, ln := range l.tailnet {
		if !slices.Contains(want, a) {
			ln.Close()
			delete(l.tailnet, a)
		}
	}
	for _, a := range want {
		if l.tailnet[a] != nil {
			continue
		}
		ln, err := l.listen("tcp", netip.AddrPortFrom(a, uint16(l.port)).String())
		if err != nil {
			if !fromCLI {
				fmt.Fprintf(os.Stderr, "pocketd: listen %s: %v\n", a, err)
			}
			continue
		}
		l.tailnet[a] = ln
		go http.Serve(ln, l.gate())
	}
}

// Addrs lists every bound address, sorted, loopback included.
func (l *Listener) Addrs() []netip.AddrPort {
	l.mu.Lock()
	defer l.mu.Unlock()
	out := []netip.AddrPort{netip.AddrPortFrom(netip.AddrFrom4([4]byte{127, 0, 0, 1}), uint16(l.port))}
	for a := range l.tailnet {
		out = append(out, netip.AddrPortFrom(a, uint16(l.port)))
	}
	slices.SortFunc(out, netip.AddrPort.Compare)
	return out
}

func (l *Listener) Tailnet() bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	return len(l.tailnet) > 0
}

// PairHost is the address a pairing link carries: the first tailnet IPv4.
func (l *Listener) PairHost() (string, bool) {
	for _, a := range l.Addrs() {
		if a.Addr().Is4() && !a.Addr().IsLoopback() {
			return a.String(), true
		}
	}
	return "", false
}

// AllowHost accepts a Host header naming loopback, localhost, a bound tailnet
// address or a *.ts.net name. Anything else is DNS rebinding or a stray proxy.
func (l *Listener) AllowHost(hostport string) bool {
	host, _, err := net.SplitHostPort(hostport)
	if err != nil {
		host = strings.Trim(hostport, "[]")
	}
	host = strings.TrimSuffix(strings.ToLower(host), ".")
	if host == "localhost" || strings.HasSuffix(host, ".ts.net") {
		return true
	}
	a, err := netip.ParseAddr(host)
	if err != nil {
		return false
	}
	if a = a.Unmap(); a.IsLoopback() {
		return true
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.tailnet[a] != nil
}

func (l *Listener) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return nil
	}
	l.closed = true
	close(l.done)
	for _, ln := range l.tailnet {
		ln.Close()
	}
	return l.loopback.Close()
}
