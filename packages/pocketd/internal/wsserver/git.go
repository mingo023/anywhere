package wsserver

import (
	"errors"
	"path/filepath"
	"slices"
	"sync"

	"pocketd/internal/gitview"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// GitService serves the git.v1 verbs; gitview.Service is the real one.
type GitService interface {
	Status(tree string, cursor, limit int) (proto.GitStatus, error)
	Diff(tree, path string, at proto.DiffAt, o gitview.DiffOptions) (proto.GitDiff, error)
	Log(tree string, all bool, skip, limit int) (proto.LogTips, []proto.LogCommit, bool, error)
	Commit(tree, sha string) (proto.GitCommit, error)
	Stage(tree string, paths []string, staged bool) error
	Discard(tree string, paths []string) error
	CreateCommit(tree, message string, amend, all bool) (string, error)
	Push(tree string, lease bool) error
	Pull(tree string) error
	Watch(trees []string, changed func(tree, rev string)) (stop func())
}

// gitConn is a conn's watch set.
type gitConn struct {
	mu   sync.Mutex
	stop func()
}

func (c *conn) helloGit() {}

func (c *conn) closeGit() { c.watchGit(nil) }

// gitVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) gitVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapGit) {
		return proto.ErrMalformed
	}
	if c.s.Git == nil {
		return c.notImplemented(m)
	}
	if m.Type == "git.watch" {
		return c.gitWatch(m)
	}
	if err := c.gitTree(m.Tree); err != nil {
		return err
	}
	paths := m.Paths
	if m.Path != "" {
		paths = append(slices.Clip(paths), m.Path)
	}
	if err := gitview.InTree(m.Tree, paths...); err != nil {
		return &peer.Refusal{Code: proto.CodeOutsideTree, Message: err.Error()}
	}
	// git takes a while on a big repo; the conn's other messages don't wait.
	go func() {
		if err := c.gitServe(m); err != nil {
			c.send(gitError(m.ID, err))
		}
	}()
	return nil
}

func (c *conn) gitServe(m proto.ClientMessage) error {
	g := c.s.Git
	switch m.Type {
	case "git.status":
		limit := 0
		if m.Limit != nil {
			limit = int(*m.Limit)
		}
		st, err := g.Status(m.Tree, m.Cursor, limit)
		if err != nil {
			return err
		}
		st.ID = m.ID
		c.send(st)
	case "git.diff":
		d, err := g.Diff(m.Tree, m.Path, *m.At, gitview.DiffOptions{Context: m.Context, IgnoreWhitespace: m.IgnoreWhitespace, MaxLines: m.MaxLines})
		if err != nil {
			return err
		}
		d.ID = m.ID
		c.send(d)
	case "git.log":
		limit := 0
		if m.Limit != nil {
			limit = int(*m.Limit)
		}
		tips, commits, more, err := g.Log(m.Tree, m.All, m.Skip, limit)
		if err != nil {
			return err
		}
		c.send(proto.NewGitLog(m.ID, tips, commits, more))
	case "git.commit":
		cm, err := g.Commit(m.Tree, m.Sha)
		if err != nil {
			return err
		}
		cm.ID = m.ID
		c.send(cm)
	case "git.commit.create":
		sha, err := g.CreateCommit(m.Tree, m.Message, m.Amend, m.All)
		if err != nil {
			return err
		}
		c.send(proto.NewGitCommitted(m.ID, sha))
	default:
		var err error
		switch m.Type {
		case "git.stage":
			err = g.Stage(m.Tree, m.Paths, m.Staged)
		case "git.discard":
			err = g.Discard(m.Tree, m.Paths)
		case "git.push":
			err = g.Push(m.Tree, m.Lease)
		case "git.pull":
			err = g.Pull(m.Tree)
		}
		if err != nil {
			return err
		}
		c.send(proto.NewAck(m.ID))
	}
	return nil
}

// gitWatch replaces this conn's watch set; every tree must be one.
func (c *conn) gitWatch(m proto.ClientMessage) error {
	for _, t := range m.Trees {
		if err := c.gitTree(t); err != nil {
			return err
		}
	}
	c.watchGit(m.Trees)
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) watchGit(trees []string) {
	c.git.mu.Lock()
	defer c.git.mu.Unlock()
	if c.git.stop != nil {
		c.git.stop()
		c.git.stop = nil
	}
	if len(trees) > 0 && c.s.Git != nil {
		c.git.stop = c.s.Git.Watch(trees, func(tree, rev string) { c.send(proto.NewGitChanged(tree, rev)) })
	}
}

// gitTree refuses anything but a clean absolute path inside a registered project.
func (c *conn) gitTree(tree string) error {
	refuse := &peer.Refusal{Code: proto.CodeNotATree, Message: "Not a project, worktree or Local: " + tree}
	if c.s.Registry == nil || !filepath.IsAbs(tree) || filepath.Clean(tree) != tree {
		return refuse
	}
	if _, ok := worktree.Containing(c.s.Registry.Load(), tree); !ok {
		return refuse
	}
	return nil
}

func gitError(id string, err error) proto.Error {
	if f, ok := errors.AsType[*gitview.Failed](err); ok {
		return proto.NewCodedError(id, proto.CodeGitFailed, f.Message, f.Detail)
	}
	if errors.Is(err, gitview.ErrOutsideTree) {
		return proto.NewErrorCode(id, proto.CodeOutsideTree, err.Error())
	}
	return errorReply(id, err)
}
