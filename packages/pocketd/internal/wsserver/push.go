package wsserver

import (
	"slices"

	"pocketd/internal/proto"
)

// PushService serves the push.v1 verbs.
type PushService interface{}

// pushConn is a conn's state for this area.
type pushConn struct{}

func (c *conn) helloPush() {}

func (c *conn) closePush() {}

// pushVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) pushVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapPush) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "push.register":
		return c.pushRegister(m)
	case "push.activity":
		return c.pushActivity(m)
	case "push.prefs":
		return c.pushPrefs(m)
	case "push.unregister":
		return c.pushUnregister(m)
	case "push.test":
		return c.pushTest(m)
	}
	return proto.ErrMalformed
}

func (c *conn) pushRegister(m proto.ClientMessage) error   { return c.notImplemented(m) }
func (c *conn) pushActivity(m proto.ClientMessage) error   { return c.notImplemented(m) }
func (c *conn) pushPrefs(m proto.ClientMessage) error      { return c.notImplemented(m) }
func (c *conn) pushUnregister(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) pushTest(m proto.ClientMessage) error       { return c.notImplemented(m) }
