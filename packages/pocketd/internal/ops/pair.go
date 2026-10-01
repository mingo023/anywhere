package ops

import (
	"context"
	"time"

	"pocketd/internal/pairing"
)

// pairBegin opens a code for this connection. It reports pair.ok or
// pair.expired later; closing the connection voids the code.
func (s *Server) pairBegin(ctx context.Context, c *Conn, m Msg) {
	host, ok := m.Text, m.Text != ""
	if !ok {
		host, ok = s.Host()
	}
	if !ok {
		c.Send(Msg{Ev: "error", Error: "Tailscale isn't running. Phones can't reach this Mac.", ErrorCode: "tailnet_off"})
		return
	}
	offer, done, err := s.Pairing.Begin(host, s.MacName)
	if err != nil {
		c.Send(Msg{Ev: "error", Error: err.Error(), ErrorCode: pairing.Code(err)})
		return
	}
	c.Send(Msg{Ev: "pair.begin", Pair: &offer})
	go func() {
		expiry := time.NewTimer(time.Until(time.UnixMilli(offer.ExpiresAt)))
		defer expiry.Stop()
		select {
		case r := <-done:
			c.Send(pairResult(r))
		case <-expiry.C:
			if s.Pairing.Cancel(offer.Code) {
				c.Send(Msg{Ev: "pair.expired", ErrorCode: "pair_expired"})
			} else {
				c.Send(pairResult(<-done))
			}
		case <-ctx.Done():
			s.Pairing.Cancel(offer.Code)
		}
	}()
}

func pairResult(r pairing.Result) Msg {
	if r.Code != "" {
		return Msg{Ev: "pair.expired", ErrorCode: r.Code}
	}
	return Msg{Ev: "pair.ok", ID: r.DeviceID, Text: r.Name}
}
