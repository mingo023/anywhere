package ops

import (
	"log"
	"net"
	"os"
	"sync"
	"time"
)

var keepEvery = 2 * time.Second

// keptListener listens on a socket path and binds it again when the file is
// removed. A listener whose file is gone still runs but nothing can dial it,
// and every hook then waits out its timeout.
type keptListener struct {
	path  string
	every time.Duration
	conns chan net.Conn
	done  chan struct{}
	once  sync.Once

	mu  sync.Mutex
	cur *net.UnixListener
}

func keep(path string, ln *net.UnixListener) *keptListener {
	k := &keptListener{path: path, every: keepEvery, conns: make(chan net.Conn), done: make(chan struct{})}
	k.use(ln)
	go k.watch()
	return k
}

func (k *keptListener) use(ln *net.UnixListener) {
	k.mu.Lock()
	k.cur = ln
	k.mu.Unlock()
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			select {
			case k.conns <- c:
			case <-k.done:
				c.Close()
				return
			}
		}
	}()
}

func (k *keptListener) watch() {
	for {
		select {
		case <-k.done:
			return
		case <-time.After(k.every):
		}
		if _, err := os.Lstat(k.path); err == nil {
			continue
		}
		ln, err := net.ListenUnix("unix", &net.UnixAddr{Name: k.path, Net: "unix"})
		if err == nil {
			err = os.Chmod(k.path, 0o600)
		}
		if err != nil {
			log.Printf("ops: rebinding %s: %v", k.path, err)
			continue
		}
		log.Printf("ops: %s was removed; listening on it again", k.path)
		k.mu.Lock()
		old := k.cur
		k.mu.Unlock()
		// Closing would unlink the file just bound at the same path.
		old.SetUnlinkOnClose(false)
		old.Close()
		k.use(ln)
	}
}

func (k *keptListener) Accept() (net.Conn, error) {
	select {
	case c := <-k.conns:
		return c, nil
	case <-k.done:
		return nil, net.ErrClosed
	}
}

func (k *keptListener) Close() error {
	k.once.Do(func() { close(k.done) })
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.cur.Close()
}

func (k *keptListener) Addr() net.Addr {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.cur.Addr()
}
