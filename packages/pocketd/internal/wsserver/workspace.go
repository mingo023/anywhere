package wsserver

import (
	"slices"

	"pocketd/internal/proto"
)

// WorkspaceService serves the workspace.v1 verbs.
type WorkspaceService interface{}

// workspaceConn is a conn's state for this area.
type workspaceConn struct{}

func (c *conn) helloWorkspace() {}

func (c *conn) closeWorkspace() {}

// workspaceVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) workspaceVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapWorkspace) {
		return proto.ErrMalformed
	}
	switch m.Type {
	case "worktree.create":
		return c.worktreeCreate(m)
	case "worktree.delete":
		return c.worktreeDelete(m)
	case "branch.list":
		return c.branchList(m)
	case "account.list":
		return c.accountList(m)
	case "agent.account":
		return c.agentAccount(m)
	}
	return proto.ErrMalformed
}

func (c *conn) worktreeCreate(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) worktreeDelete(m proto.ClientMessage) error { return c.notImplemented(m) }
func (c *conn) branchList(m proto.ClientMessage) error     { return c.notImplemented(m) }
func (c *conn) accountList(m proto.ClientMessage) error    { return c.notImplemented(m) }
func (c *conn) agentAccount(m proto.ClientMessage) error   { return c.notImplemented(m) }
