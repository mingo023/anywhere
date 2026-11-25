package wsserver

import (
	"cmp"
	"context"
	"encoding/json"
	"log"
	"net"
	"net/http"
	"runtime/debug"
	"time"
)

const (
	frameQueue         = 1024
	defaultSlowHandler = 2 * time.Second
)

type inbound struct {
	raw []byte
	// handled closes once raw is handled; set only before auth, where the next read waits for it.
	handled chan struct{}
}

// overSocket says r came in over the unix socket. A peer that dies closes it,
// so it needs no ping; TCP peers, owner-token ones included, do.
func overSocket(r *http.Request) bool {
	a, ok := r.Context().Value(http.LocalAddrContextKey).(net.Addr)
	return ok && a.Network() == "unix"
}

// serve reads c's frames on this goroutine and handles them in order on
// another, so a slow handler doesn't stop the read that takes a pong. A full
// queue stops the read; it never closes the conn.
func (c *conn) serve(helloCtx context.Context) {
	started := time.Now()
	frames := make(chan inbound, frameQueue)
	done := make(chan struct{})
	go func() {
		defer close(done)
		c.handleFrames(frames)
	}()
	reason := c.readFrames(helloCtx, frames, done)
	close(frames)
	<-done
	log.Printf("ws closed: client=%q lived=%s reason=%s", c.clientID, time.Since(started).Round(time.Millisecond), reason)
}

func (c *conn) readFrames(helloCtx context.Context, frames chan<- inbound, done <-chan struct{}) string {
	for {
		readCtx, before := c.ctx, !c.ready.Load()
		if before {
			// Until auth, the 4 KB read limit and the hello timeout hold for each frame, so the next read waits for this one's handler.
			readCtx = helloCtx
		}
		_, raw, err := c.ws.Read(readCtx)
		if err != nil {
			c.ws.CloseNow()
			if why := c.closing.Load(); why != nil {
				return *why
			}
			return err.Error()
		}
		f := inbound{raw: raw}
		if before {
			f.handled = make(chan struct{})
		}
		select {
		case frames <- f:
		case <-done:
			return "handler stopped"
		case <-c.ctx.Done():
			c.ws.CloseNow()
			return "conn ended"
		}
		if before {
			select {
			case <-f.handled:
			case <-done:
				return "handler stopped"
			case <-c.ctx.Done():
				c.ws.CloseNow()
				return "conn ended"
			}
		}
	}
}

func (c *conn) handleFrames(frames <-chan inbound) {
	defer func() {
		// net/http would have recovered this on the goroutine that handled it before.
		if r := recover(); r != nil {
			log.Printf("ws handler panic: %v\n%s", r, debug.Stack())
			c.ws.CloseNow()
		}
	}()
	slow := cmp.Or(c.s.slowHandler, defaultSlowHandler)
	for f := range frames {
		start := time.Now()
		c.handle(f.raw)
		if c.authed {
			c.ready.Store(true)
		}
		if f.handled != nil {
			close(f.handled)
		}
		if d := time.Since(start); d > slow {
			var head struct {
				Type string `json:"type"`
			}
			json.Unmarshal(f.raw, &head)
			log.Printf("ws handler slow: verb=%s took=%s", head.Type, d.Round(time.Millisecond))
		}
	}
}
