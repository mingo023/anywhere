package wsserver

import (
	"slices"

	"pocketd/internal/proto"
)

// GitHubService serves the gh.v1 verbs.
type GitHubService interface{}

// ghConn is a conn's state for this area.
type ghConn struct{}

func (c *conn) helloGitHub() {}

func (c *conn) closeGitHub() {}

// ghVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) ghVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapGitHub) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "gh.status":
		return c.ghStatus(m)
	case "gh.pr":
		return c.ghPR(m)
	case "gh.prs":
		return c.ghPRs(m)
	case "gh.pr.create":
		return c.ghPRCreate(m)
	case "gh.pr.merge":
		return c.ghPRMerge(m)
	case "gh.pr.ready":
		return c.ghPRReady(m)
	case "gh.thread.reply":
		return c.ghThreadReply(m)
	case "gh.thread.resolve":
		return c.ghThreadResolve(m)
	}
	return proto.ErrMalformed
}

func (c *conn) ghStatus(m proto.ClientMessage) error        { return c.notImplemented(m) }
func (c *conn) ghPR(m proto.ClientMessage) error            { return c.notImplemented(m) }
func (c *conn) ghPRs(m proto.ClientMessage) error           { return c.notImplemented(m) }
func (c *conn) ghPRCreate(m proto.ClientMessage) error      { return c.notImplemented(m) }
func (c *conn) ghPRMerge(m proto.ClientMessage) error       { return c.notImplemented(m) }
func (c *conn) ghPRReady(m proto.ClientMessage) error       { return c.notImplemented(m) }
func (c *conn) ghThreadReply(m proto.ClientMessage) error   { return c.notImplemented(m) }
func (c *conn) ghThreadResolve(m proto.ClientMessage) error { return c.notImplemented(m) }
