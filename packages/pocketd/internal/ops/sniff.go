package ops

import (
	"bufio"
	"errors"
	"net"
	"sync"
	"time"

	"pocketd/internal/peer"
)

const defaultPeekTimeout = 10 * time.Second

// sniffed is a socket conn whose first byte was peeked; reads go through r so
// that byte isn't lost.
type sniffed struct {
	net.Conn
	r   *bufio.Reader
	who peer.Principal
}

func (c *sniffed) Read(b []byte) (int, error) { return c.r.Read(b) }

// chanListener hands conns that open with an HTTP request to an http.Server.
type chanListener struct {
	conns chan net.Conn
	done  chan struct{}
	once  sync.Once
	addr  net.Addr
}

func newChanListener(addr net.Addr) *chanListener {
	return &chanListener{conns: make(chan net.Conn), done: make(chan struct{}), addr: addr}
}

func (l *chanListener) Accept() (net.Conn, error) {
	select {
	case c := <-l.conns:
		return c, nil
	case <-l.done:
		return nil, net.ErrClosed
	}
}

func (l *chanListener) Close() error {
	l.once.Do(func() { close(l.done) })
	return nil
}

func (l *chanListener) Addr() net.Addr { return l.addr }

func (l *chanListener) hand(c net.Conn) {
	select {
	case l.conns <- c:
	case <-l.done:
		c.Close()
	}
}

// A TCP peer is anyone on the host until it sends an owner token, so before
// auth it gets as long and as many bytes as the WebSocket hello does.
var authWindow = 10 * time.Second

const authBytes = 4 << 10

// preauth is a TCP conn that can read authBytes in authWindow until authed.
type preauth struct {
	net.Conn
	left int
}

func newPreauth(c net.Conn) *preauth {
	c.SetReadDeadline(time.Now().Add(authWindow))
	return &preauth{Conn: c, left: authBytes}
}

func (c *preauth) Read(b []byte) (int, error) {
	if c.left < 0 {
		return c.Conn.Read(b)
	}
	if c.left == 0 {
		return 0, errors.New("too much before auth")
	}
	n, err := c.Conn.Read(b[:min(len(b), c.left)])
	c.left -= n
	return n, err
}

func (c *preauth) authed() {
	c.left = -1
	c.SetReadDeadline(time.Time{})
}

// sniff reads the first byte: '{' starts line JSON, 'G' a WebSocket upgrade.
func sniff(c net.Conn, who peer.Principal, timeout time.Duration) (*sniffed, byte, error) {
	r := bufio.NewReader(c)
	c.SetReadDeadline(time.Now().Add(timeout))
	first, err := r.Peek(1)
	c.SetReadDeadline(time.Time{})
	if err != nil {
		return nil, 0, err
	}
	return &sniffed{Conn: c, r: r, who: who}, first[0], nil
}
