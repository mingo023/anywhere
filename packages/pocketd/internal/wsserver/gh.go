package wsserver

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"

	"pocketd/internal/github"
	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// GitHubService serves the gh.v1 verbs; github.Service is the one.
type GitHubService interface {
	Listen(key string, changed func(tree string))
	Watch(key string, trees []string)
	Leave(key string)
	Status(ctx context.Context, tree string) github.Status
	PR(ctx context.Context, tree string) (*proto.PR, github.Problem)
	PRs(project string) []proto.PRItem
	Create(ctx context.Context, tree, title, body, base string, draft bool) (int, string, error)
	Merge(ctx context.Context, tree string, number int, method string) error
	Ready(ctx context.Context, tree string, number int) error
	Reply(ctx context.Context, tree, thread, body string) error
	Resolve(ctx context.Context, tree, thread string, resolved bool) error
}

// ghConn is a conn's state for this area.
type ghConn struct{}

func (c *conn) ghServed() bool { return c.s.GitHub != nil && slices.Contains(c.caps, proto.CapGitHub) }

// helloGitHub has the poller tell this conn when a tree it watches has a new PR result.
func (c *conn) helloGitHub() {
	if c.ghServed() {
		c.s.GitHub.Listen(c.key, func(tree string) { c.send(proto.NewGhChanged(tree)) })
	}
}

func (c *conn) closeGitHub() {
	if c.s.GitHub != nil {
		c.s.GitHub.Leave(c.key)
	}
}

// ghWatch takes a git.watch's trees as the ones whose PRs this conn watches.
// git.watch belongs to git.v1; its handler hands its trees here.
func (c *conn) ghWatch(trees []string) {
	if !c.ghServed() {
		return
	}
	var ok []string
	for _, t := range trees {
		if c.ghTree(t) {
			ok = append(ok, t)
		}
	}
	c.s.GitHub.Watch(c.key, ok)
}

// ghTree: tree is a folder a registered Project holds.
func (c *conn) ghTree(tree string) bool {
	if c.s.Registry == nil || !filepath.IsAbs(tree) || filepath.Clean(tree) != tree {
		return false
	}
	if _, ok := worktree.Containing(c.s.Registry.Load(), tree); !ok {
		return false
	}
	fi, err := os.Stat(tree)
	return err == nil && fi.IsDir()
}

// ghVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) ghVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapGitHub) {
		return proto.ErrMalformed
	}
	if c.s.GitHub == nil {
		return c.notImplemented(m)
	}
	switch m.Type {
	case "gh.prs":
		return c.ghPRs(m)
	case "gh.status":
		if m.Tree == "" {
			go c.ghStatus(m)
			return nil
		}
	}
	if !c.ghTree(m.Tree) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeNotATree, "Not a worktree of a project on this Mac"))
		return nil
	}
	// gh takes seconds; the conn keeps reading meanwhile.
	switch m.Type {
	case "gh.status":
		go c.ghStatus(m)
	case "gh.pr":
		go c.ghPR(m)
	case "gh.pr.create":
		go c.ghPRCreate(m)
	case "gh.pr.merge":
		go c.ghAck(m, func(ctx context.Context) error { return c.s.GitHub.Merge(ctx, m.Tree, m.Number, m.Method) })
	case "gh.pr.ready":
		go c.ghAck(m, func(ctx context.Context) error { return c.s.GitHub.Ready(ctx, m.Tree, m.Number) })
	case "gh.thread.reply":
		go c.ghAck(m, func(ctx context.Context) error { return c.s.GitHub.Reply(ctx, m.Tree, m.Thread, m.Body) })
	case "gh.thread.resolve":
		go c.ghAck(m, func(ctx context.Context) error { return c.s.GitHub.Resolve(ctx, m.Tree, m.Thread, m.Resolved) })
	default:
		return proto.ErrMalformed
	}
	return nil
}

func (c *conn) ghStatus(m proto.ClientMessage) {
	st := c.s.GitHub.Status(c.ctx, m.Tree)
	c.send(proto.NewGhStatus(m.ID, st.State, st.Login))
}

func (c *conn) ghPR(m proto.ClientMessage) {
	pr, problem := c.s.GitHub.PR(c.ctx, m.Tree)
	c.send(proto.NewGhPR(m.ID, m.Tree, string(problem), pr))
}

func (c *conn) ghPRs(m proto.ClientMessage) error {
	if c.s.Registry == nil || !slices.Contains(c.s.Registry.Load().Projects, m.Project) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownProject, "Unknown project"))
		return nil
	}
	go func() { c.send(proto.NewGhPRs(m.ID, c.s.GitHub.PRs(m.Project))) }()
	return nil
}

func (c *conn) ghPRCreate(m proto.ClientMessage) {
	number, url, err := c.s.GitHub.Create(c.ctx, m.Tree, m.Title, m.Body, m.Base, m.Draft)
	if err != nil {
		c.send(ghError(m.ID, err))
		return
	}
	c.send(proto.NewGhPRCreated(m.ID, number, url))
}

func (c *conn) ghAck(m proto.ClientMessage, act func(context.Context) error) {
	if err := act(c.ctx); err != nil {
		c.send(ghError(m.ID, err))
		return
	}
	c.send(proto.NewAck(m.ID))
}

func ghError(id string, err error) proto.Error {
	if e, ok := errors.AsType[*github.Error](err); ok {
		return proto.NewCodedError(id, e.Code, e.Message, e.Detail)
	}
	return proto.NewErrorCode(id, proto.CodeGhFailed, err.Error())
}
