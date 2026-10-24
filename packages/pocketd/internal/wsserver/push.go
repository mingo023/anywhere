package wsserver

import (
	"context"
	"encoding/base64"
	"errors"
	"slices"
	"sync/atomic"
	"time"

	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/push"
)

// PushService serves the push.v1 verbs; push.Notifier is one.
type PushService interface {
	Register(device string, r push.Registration) error
	Activity(device, activityID string, h *proto.PushHandle, ended bool) error
	Prefs(device string, p proto.PushPrefs) error
	Forget(device string) error
	Test(ctx context.Context, device string) error
}

// pushConn is a conn's state for this area. A phone's app is in front from
// its hello until it sends agent.view([]), as it does on going to the
// background, and again from its next non-empty view or hello.
type pushConn struct {
	background atomic.Bool
}

func (c *conn) helloPush() { c.push.background.Store(false) }

func (c *conn) closePush() {}

// viewed notes whether the phone still shows anything; see pushConn.
func (c *conn) viewed(ids []string) { c.push.background.Store(len(ids) == 0) }

// Foreground is whether device has a connection with the app in front.
func (s *Server) Foreground(device string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	for c := range s.byDevice[device] {
		if !c.push.background.Load() {
			return true
		}
	}
	return false
}

var errNotAPhone = errors.New("Only a paired phone registers for pushes")

// phoneID is the paired device on this conn, or "".
func (c *conn) phoneID() string {
	if c.who.Kind == peer.Device {
		return c.who.Device
	}
	return c.device
}

// pushVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) pushVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapPush) {
		return proto.ErrMalformed
	}
	if c.s.Push == nil {
		return c.notImplemented(m)
	}
	device := c.phoneID()
	if device == "" {
		return errNotAPhone
	}
	var err error
	switch m.Type {
	case "push.register":
		key, _ := base64.StdEncoding.DecodeString(m.Key)
		err = c.s.Push.Register(device, push.Registration{Key: key, Env: m.Env, Alert: m.Alert, Start: m.Start, Prefs: *m.Prefs, Frequent: m.Frequent})
	case "push.activity":
		err = c.s.Push.Activity(device, m.ActivityID, m.Handle, m.Ended)
	case "push.prefs":
		err = c.s.Push.Prefs(device, *m.Prefs)
	case "push.unregister":
		err = c.s.Push.Forget(device)
	case "push.test":
		// The relay takes a round trip to APNs; the conn's other messages don't wait.
		go func() {
			ctx, cancel := context.WithTimeout(c.ctx, 30*time.Second)
			defer cancel()
			if err := c.s.Push.Test(ctx, device); err != nil {
				c.send(errorReply(m.ID, err))
				return
			}
			c.send(proto.NewAck(m.ID))
		}()
		return nil
	default:
		return proto.ErrMalformed
	}
	if err != nil {
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
