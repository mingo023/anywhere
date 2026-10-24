package wsserver

import (
	"slices"

	"pocketd/internal/proto"
)

// FileService serves the fs.v1 verbs.
type FileService interface{}

// filesConn is a conn's state for this area.
type filesConn struct{}

func (c *conn) helloFiles() {}

func (c *conn) closeFiles() {}

// filesVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) filesVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapFiles) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "fs.list":
		return c.fsList(m)
	case "fs.read":
		return c.fsRead(m)
	case "fs.search":
		return c.fsSearch(m)
	case "attach.put":
		return c.attachPut(m)
	}
	return proto.ErrMalformed
}

func (c *conn) fsList(m proto.ClientMessage) error    { return c.notImplemented(m) }
func (c *conn) fsRead(m proto.ClientMessage) error    { return c.notImplemented(m) }
func (c *conn) fsSearch(m proto.ClientMessage) error  { return c.notImplemented(m) }
func (c *conn) attachPut(m proto.ClientMessage) error { return c.notImplemented(m) }
