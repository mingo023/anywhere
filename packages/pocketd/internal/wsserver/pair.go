package wsserver

import (
	"context"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/devices"
	"pocketd/internal/pairing"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

var pairEnded = map[string]string{
	"pair_expired": "Code expired.",
	"pair_locked":  "Too many wrong codes. Try again in a minute.",
	"pair_failed":  "Pairing failed.",
}

// pair trades a one-time code for a device token, then closes: the phone
// reconnects and says hello with the token.
func (c *conn) pair(m proto.ClientMessage) {
	if c.authed {
		c.send(proto.NewError(m.ID, "Already authenticated"))
		return
	}
	if m.Protocol != nil {
		if _, _, code := proto.Negotiate(*m.Protocol, nil, nil); code != "" {
			c.tooOld(m.ID, code)
			return
		}
	}
	d, token, err := c.s.Pairing.Redeem(m.Code, func() (devices.Device, string, error) {
		return c.s.Devices.Add(m.Name, m.Platform, devices.PhoneScopes)
	})
	if err != nil {
		code := pairing.Code(err)
		if code == "" {
			c.send(proto.NewError(m.ID, "Pairing failed. Make a new code on your Mac."))
			return
		}
		c.reject(proto.NewErrorCode(m.ID, code, err.Error()), 0, "")
		return
	}
	c.send(proto.NewPairOK(m.ID, d.ID, token))
	c.ws.Close(websocket.StatusNormalClosure, "")
}

// openOffer is the code this conn's pairBegin opened. stop ends its watcher,
// which closes gone on the way out.
type openOffer struct {
	code string
	stop func()
	gone chan struct{}
}

// pairBegin opens a code for the owner's desktop. pair.done, or an error with
// the begin's id, follows on this conn; the code ends at its expiry or when the
// conn closes. It first voids this conn's previous code, which Begin would end
// with pair_expired, so nothing about the old code can follow the new offer.
func (c *conn) pairBegin(id string) error {
	c.endOffer()
	host, ok := "", false
	if c.s.Host != nil {
		host, ok = c.s.Host()
	}
	if !ok {
		return &peer.Refusal{Code: "tailnet_off", Message: "Tailscale isn't running. Phones can't reach this Mac."}
	}
	offer, done, err := c.s.Pairing.Begin(host, c.s.MacName)
	if err != nil {
		return &peer.Refusal{Code: pairing.Code(err), Message: err.Error()}
	}
	ctx, stop := context.WithCancel(c.ctx)
	c.offer = &openOffer{code: offer.Code, stop: stop, gone: make(chan struct{})}
	c.send(proto.NewPairOffer(id, offer.URL, offer.Code, offer.ExpiresAt))
	go func(gone chan struct{}) {
		defer close(gone)
		expiry := time.NewTimer(time.Until(time.UnixMilli(offer.ExpiresAt)))
		defer expiry.Stop()
		select {
		case r := <-done:
			c.pairResult(id, r)
		case <-expiry.C:
			if c.s.Pairing.Cancel(offer.Code) {
				c.pairResult(id, pairing.Result{Code: "pair_expired"})
			} else {
				c.pairResult(id, <-done)
			}
		case <-ctx.Done():
			c.s.Pairing.Cancel(offer.Code)
		}
	}(c.offer.gone)
	return nil
}

// endOffer voids this conn's open code, if any, and waits for its watcher.
// Cancel sends nothing, so a code it voids ends without a word.
func (c *conn) endOffer() {
	if c.offer == nil {
		return
	}
	c.s.Pairing.Cancel(c.offer.code)
	c.offer.stop()
	<-c.offer.gone
	c.offer = nil
}

func (c *conn) pairResult(id string, r pairing.Result) {
	if r.Code != "" {
		c.send(proto.NewErrorCode(id, r.Code, pairEnded[r.Code]))
		return
	}
	c.send(proto.NewPairDone(r.DeviceID, r.Name))
}
