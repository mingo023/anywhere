package wsserver

import (
	"slices"

	"pocketd/internal/proto"
)

// GitService serves the git.v1 verbs.
type GitService interface{}

// gitConn is a conn's state for this area.
type gitConn struct{}

func (c *conn) helloGit() {}

func (c *conn) closeGit() {}

// gitVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) gitVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapGit) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "git.status":
		return c.gitStatus(m)
	case "git.watch":
		return c.gitWatch(m)
	case "git.diff":
		return c.gitDiff(m)
	case "git.log":
		return c.gitLog(m)
	case "git.commit":
		return c.gitCommit(m)
	case "git.stage":
		return c.gitStage(m)
	case "git.discard":
		return c.gitDiscard(m)
	case "git.commit.create":
		return c.gitCommitCreate(m)
	case "git.push":
		return c.gitPush(m)
	case "git.pull":
		return c.gitPull(m)
	}
	return proto.ErrMalformed
}

func (c *conn) gitStatus(m proto.ClientMessage) error       { return c.notImplemented(m) }
func (c *conn) gitWatch(m proto.ClientMessage) error        { return c.notImplemented(m) }
func (c *conn) gitDiff(m proto.ClientMessage) error         { return c.notImplemented(m) }
func (c *conn) gitLog(m proto.ClientMessage) error          { return c.notImplemented(m) }
func (c *conn) gitCommit(m proto.ClientMessage) error       { return c.notImplemented(m) }
func (c *conn) gitStage(m proto.ClientMessage) error        { return c.notImplemented(m) }
func (c *conn) gitDiscard(m proto.ClientMessage) error      { return c.notImplemented(m) }
func (c *conn) gitCommitCreate(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) gitPush(m proto.ClientMessage) error         { return c.notImplemented(m) }
func (c *conn) gitPull(m proto.ClientMessage) error         { return c.notImplemented(m) }
