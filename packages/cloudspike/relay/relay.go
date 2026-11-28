// Package relay forwards bytes between a member and a machine that both
// dialled out to it. It never holds a key, so it sees only TLS records.
//
// A machine keeps one line open on /host. A member dials /dial; the relay
// names a session on the machine's line, the machine dials /accept with it,
// and the relay splices the two.
package relay

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"sync"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"
)

const acceptWithin = 10 * time.Second

type Relay struct {
	mu      sync.Mutex
	hosts   map[string]*websocket.Conn
	pending map[string]chan net.Conn
	sent    map[string]*atomic.Int64
	tap     func([]byte)
}

func New() *Relay {
	return &Relay{hosts: map[string]*websocket.Conn{}, pending: map[string]chan net.Conn{}, sent: map[string]*atomic.Int64{}}
}

// Sent is how many bytes the relay forwarded for account, both ways.
func (r *Relay) Sent(account string) int64 { return r.counter(account).Load() }

func (r *Relay) Usage() map[string]int64 {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make(map[string]int64, len(r.sent))
	for a, n := range r.sent {
		out[a] = n.Load()
	}
	return out
}

func (r *Relay) counter(account string) *atomic.Int64 {
	r.mu.Lock()
	defer r.mu.Unlock()
	n := r.sent[account]
	if n == nil {
		n = new(atomic.Int64)
		r.sent[account] = n
	}
	return n
}

func (r *Relay) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	q := req.URL.Query()
	switch req.URL.Path {
	case "/host":
		r.host(w, req, q.Get("account"), q.Get("host"))
	case "/dial":
		r.dial(w, req, q.Get("account"), q.Get("host"))
	case "/accept":
		r.accept(w, req, q.Get("session"))
	default:
		http.NotFound(w, req)
	}
}

func (r *Relay) host(w http.ResponseWriter, req *http.Request, account, host string) {
	if account == "" || host == "" {
		http.Error(w, "account and host required", http.StatusBadRequest)
		return
	}
	c, err := websocket.Accept(w, req, nil)
	if err != nil {
		return
	}
	key := account + "/" + host
	r.mu.Lock()
	old := r.hosts[key]
	r.hosts[key] = c
	r.mu.Unlock()
	if old != nil {
		old.Close(websocket.StatusGoingAway, "replaced")
	}
	<-c.CloseRead(req.Context()).Done()
	r.mu.Lock()
	if r.hosts[key] == c {
		delete(r.hosts, key)
	}
	r.mu.Unlock()
}

func (r *Relay) dial(w http.ResponseWriter, req *http.Request, account, host string) {
	r.mu.Lock()
	line := r.hosts[account+"/"+host]
	r.mu.Unlock()
	if line == nil {
		http.Error(w, "host offline", http.StatusNotFound)
		return
	}
	b := make([]byte, 16)
	rand.Read(b)
	id := hex.EncodeToString(b)
	got := make(chan net.Conn)
	r.mu.Lock()
	r.pending[id] = got
	r.mu.Unlock()
	defer func() {
		r.mu.Lock()
		delete(r.pending, id)
		r.mu.Unlock()
	}()
	ctx, cancel := context.WithTimeout(req.Context(), acceptWithin)
	defer cancel()
	if err := line.Write(ctx, websocket.MessageText, []byte(id)); err != nil {
		http.Error(w, "host unreachable", http.StatusBadGateway)
		return
	}
	var hostConn net.Conn
	select {
	case hostConn = <-got:
	case <-ctx.Done():
		http.Error(w, "host did not answer", http.StatusGatewayTimeout)
		return
	}
	c, err := websocket.Accept(w, req, nil)
	if err != nil {
		hostConn.Close()
		return
	}
	r.splice(websocket.NetConn(req.Context(), c, websocket.MessageBinary), hostConn, r.counter(account))
}

func (r *Relay) accept(w http.ResponseWriter, req *http.Request, id string) {
	r.mu.Lock()
	got := r.pending[id]
	delete(r.pending, id)
	r.mu.Unlock()
	if got == nil {
		http.Error(w, "no such session", http.StatusNotFound)
		return
	}
	c, err := websocket.Accept(w, req, nil)
	if err != nil {
		return
	}
	conn := &closeSignal{Conn: websocket.NetConn(req.Context(), c, websocket.MessageBinary), done: make(chan struct{})}
	select {
	case got <- conn:
	case <-time.After(acceptWithin):
		conn.Close()
		return
	}
	// The splice runs in the dialler's handler; returning here would end this socket.
	<-conn.done
}

type closeSignal struct {
	net.Conn
	once sync.Once
	done chan struct{}
}

func (c *closeSignal) Close() error {
	err := c.Conn.Close()
	c.once.Do(func() { close(c.done) })
	return err
}

func (r *Relay) splice(a, b net.Conn, sent *atomic.Int64) {
	var once sync.Once
	var wg sync.WaitGroup
	cp := func(dst, src net.Conn) {
		defer wg.Done()
		io.Copy(counting{dst, sent, r.tap}, src)
		once.Do(func() {
			a.Close()
			b.Close()
		})
	}
	wg.Add(2)
	go cp(a, b)
	go cp(b, a)
	wg.Wait()
}

type counting struct {
	w   io.Writer
	n   *atomic.Int64
	tap func([]byte)
}

func (c counting) Write(p []byte) (int, error) {
	if c.tap != nil {
		c.tap(p)
	}
	n, err := c.w.Write(p)
	c.n.Add(int64(n))
	return n, err
}

var ErrOffline = errors.New("host offline")

// Dial opens a byte stream to host through the relay at base (ws:// or wss://).
func Dial(ctx context.Context, base, account, host string) (net.Conn, error) {
	c, resp, err := websocket.Dial(ctx, base+"/dial?"+url.Values{"account": {account}, "host": {host}}.Encode(), nil)
	if err != nil {
		if resp != nil && resp.StatusCode == http.StatusNotFound {
			return nil, ErrOffline
		}
		return nil, err
	}
	return websocket.NetConn(context.Background(), c, websocket.MessageBinary), nil
}

// Serve holds host's line to the relay and hands each session to handle. It
// pings, since nothing promises an idle WebSocket lives, and returns when the
// line drops; the caller reconnects.
func Serve(ctx context.Context, base, account, host string, handle func(net.Conn)) error {
	c, _, err := websocket.Dial(ctx, base+"/host?"+url.Values{"account": {account}, "host": {host}}.Encode(), nil)
	if err != nil {
		return err
	}
	defer c.CloseNow()
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	go func() {
		t := time.NewTicker(30 * time.Second)
		defer t.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-t.C:
				pctx, done := context.WithTimeout(ctx, 10*time.Second)
				if c.Ping(pctx) != nil {
					c.CloseNow()
				}
				done()
			}
		}
	}()
	for {
		_, id, err := c.Read(ctx)
		if err != nil {
			return err
		}
		go func() {
			dc, _, err := websocket.Dial(ctx, base+"/accept?"+url.Values{"session": {string(id)}}.Encode(), nil)
			if err != nil {
				return
			}
			handle(websocket.NetConn(context.Background(), dc, websocket.MessageBinary))
		}()
	}
}
