package wsserver

import (
	"net/http"
	"net/netip"
	"sync"
	"time"
)

const (
	maxFailures   = 3
	failureWindow = time.Minute
)

type limiter struct {
	mu    sync.Mutex
	fails map[netip.Addr]window
}

type window struct {
	start time.Time
	n     int
}

// fail records a failure and reports whether it locked the address out.
func (l *limiter) fail(ip netip.Addr, now time.Time) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.fails == nil {
		l.fails = map[netip.Addr]window{}
	}
	for a, w := range l.fails {
		if now.Sub(w.start) >= failureWindow {
			delete(l.fails, a)
		}
	}
	w := l.fails[ip]
	if w.n == 0 {
		w.start = now
	}
	w.n++
	l.fails[ip] = w
	return w.n >= maxFailures
}

func (l *limiter) locked(ip netip.Addr, now time.Time) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	w, ok := l.fails[ip]
	return ok && w.n >= maxFailures && now.Sub(w.start) < failureWindow
}

func remoteIP(r *http.Request) netip.Addr {
	ap, _ := netip.ParseAddrPort(r.RemoteAddr)
	return ap.Addr().Unmap()
}
