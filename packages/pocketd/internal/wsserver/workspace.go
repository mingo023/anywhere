package wsserver

import (
	"context"
	"errors"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/workspace"
	"pocketd/internal/worktree"
)

// WorkspaceService serves the workspace.v1 verbs. Worktrees are made by
// Server.Launch and removed from Server.Registry's projects.
type WorkspaceService struct {
	// Projects is the cached project list pushed to workspace.v1 conns; nil serves project.list from git as before.
	Projects *workspace.Projects
	// Accounts answers account.list; nil lists none.
	Accounts *workspace.AccountList
	// Switch moves a chat-only agent to another account once its turn ends; nil switches none.
	Switch func(agentID, account string) error
}

// closeWait bounds how long worktree.delete waits on a closed terminal's
// processes before git removes their folder.
const closeWait = 5 * time.Second

// usageWait bounds the usage fetch account.list may wait on.
const usageWait = 5 * time.Second

// workspaceConn is a conn's state for this area.
type workspaceConn struct {
	stop func()
}

// helloWorkspace subscribes a workspace.v1 conn to project.list once, then
// sends the list as it stands, after every hello.
func (c *conn) helloWorkspace() {
	p := c.s.Workspace.Projects
	if p == nil || !slices.Contains(c.caps, proto.CapWorkspace) {
		return
	}
	if c.workspace.stop == nil {
		var msgs <-chan []byte
		msgs, c.workspace.stop = p.Subscribe()
		go c.forward(msgs)
	}
	if list, ok := p.List(); ok {
		c.send(proto.NewProjectList("", list))
	}
}

func (c *conn) closeWorkspace() {
	if c.workspace.stop != nil {
		c.workspace.stop()
	}
}

// projectList answers from the cache for a workspace.v1 conn. Anyone else
// gets it fresh off the read loop, through the cache's one-at-a-time Scan so
// a burst of asks can't run git for every project at once.
func (c *conn) projectList(m proto.ClientMessage) {
	p := c.s.Workspace.Projects
	if p == nil {
		go c.send(proto.NewProjectList(m.ID, c.s.Projects()))
		return
	}
	if list, ok := p.List(); ok && slices.Contains(c.caps, proto.CapWorkspace) {
		c.send(proto.NewProjectList(m.ID, list))
		return
	}
	go func() {
		p.Scan()
		list, _ := p.List()
		c.send(proto.NewProjectList(m.ID, list))
	}()
}

// workspaceScope names the scope local.* and worktree.rename are checked
// against: a phone that speaks workspace.v1 manages Locals and names
// worktrees with spawn, as it makes them; anyone else needs owner.
func (c *conn) workspaceScope(verb string) string {
	if (strings.HasPrefix(verb, "ws:local.") || verb == "ws:worktree.rename") && slices.Contains(c.caps, proto.CapWorkspace) {
		return verb + ".workspace"
	}
	return verb
}

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

// worktreeCreate makes the worktree off the read loop; fetching and copying
// can take a while.
func (c *conn) worktreeCreate(m proto.ClientMessage) error {
	if c.s.Launch == nil {
		return errors.New("This pocketd can't make worktrees")
	}
	w := launchWho(c.who)
	n := launch.NewWorktree{Project: m.Project, Name: m.Name, Base: m.Base, Branch: m.Branch, PR: m.Number, Setup: m.Setup}
	go func() {
		made, fl := c.s.Launch.Worktree(w, n, func(step, note string) {
			c.send(proto.NewWorktreeProgress(m.ID, m.RequestID, step, note))
		})
		if fl != nil {
			c.send(proto.NewCodedError(m.ID, fl.Code, fl.Message, fl.Detail))
			return
		}
		// This phone has the new row before it hears the worktree exists; the
		// hub's push to it may come after.
		if p := c.s.Workspace.Projects; p != nil {
			p.Scan()
			list, _ := p.List()
			c.send(proto.NewProjectList("", list))
		}
		c.send(proto.NewWorktreeCreated(m.ID, m.RequestID, made.Path, made.Branch))
	}()
	return nil
}

func (c *conn) worktreeDelete(m proto.ClientMessage) error {
	if c.s.Registry == nil {
		return errors.New("This pocketd can't remove worktrees")
	}
	go func() {
		if err := c.removeWorktree(m.Path, m.DeleteBranch); err != nil {
			c.send(workspaceError(m.ID, err))
			return
		}
		c.send(proto.NewAck(m.ID))
	}()
	return nil
}

// removeWorktree closes the terminals opened in the worktree at path, removes
// its folder and its name, and its branch when asked. The main worktree is
// refused before anything closes.
func (c *conn) removeWorktree(path string, deleteBranch bool) error {
	f := c.s.Registry.Load()
	path = filepath.Clean(path)
	project, ok := worktree.Containing(f, path)
	if !ok || !filepath.IsAbs(path) {
		return worktree.ErrUnknownWorktree
	}
	trees, err := worktree.List(project)
	if err != nil {
		return worktree.ErrUnknownProject
	}
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	i := slices.IndexFunc(trees, func(w worktree.Worktree) bool { return w.Path == path })
	switch {
	case i < 0:
		return worktree.ErrUnknownWorktree
	case trees[i].Main:
		return worktree.ErrMain
	}
	if c.s.Terminals != nil {
		var closing []<-chan struct{}
		for _, t := range c.s.Terminals.All() {
			if info := t.Info(); info.Local == "" && inside(info.Cwd, path) {
				t.Close()
				closing = append(closing, t.Done())
			}
		}
		deadline := time.After(closeWait)
		for _, done := range closing {
			select {
			case <-done:
			case <-deadline:
			}
		}
	}
	if err := worktree.Remove(f, project, path); err != nil {
		return err
	}
	if c.s.Names != nil {
		c.s.Names.Forget(path)
	}
	if p := c.s.Workspace.Projects; p != nil {
		defer p.Scan()
	}
	if branch := trees[i].Branch; deleteBranch && branch != "" {
		if out, err := exec.Command("git", "-C", project, "branch", "-D", "--", branch).CombinedOutput(); err != nil {
			return branchFailed{strings.TrimSpace(string(out))}
		}
	}
	return nil
}

type branchFailed struct{ stderr string }

func (g branchFailed) Error() string {
	line, _, _ := strings.Cut(g.stderr, "\n")
	return line
}

func workspaceError(id string, err error) proto.Error {
	if g, ok := errors.AsType[branchFailed](err); ok {
		return proto.NewCodedError(id, proto.CodeGitFailed, g.Error(), g.stderr)
	}
	if code := worktree.Code(err); code != "" {
		return proto.NewErrorCode(id, code, err.Error())
	}
	return proto.NewCodedError(id, proto.CodeGitFailed, err.Error(), "")
}

// workspaceTree is path as a tree of a registered project, or false.
func (c *conn) workspaceTree(path string) (string, bool) {
	if c.s.Registry == nil || !filepath.IsAbs(path) {
		return "", false
	}
	path = filepath.Clean(path)
	_, ok := worktree.Containing(c.s.Registry.Load(), path)
	return path, ok
}

func (c *conn) branchList(m proto.ClientMessage) error {
	tree, ok := c.workspaceTree(m.Tree)
	if !ok {
		c.send(proto.NewErrorCode(m.ID, proto.CodeNotATree, "This folder isn't in a project on your Mac"))
		return nil
	}
	go func() {
		current, local, remote, err := workspace.Branches(tree)
		if err != nil {
			c.send(proto.NewCodedError(m.ID, proto.CodeGitFailed, err.Error(), ""))
			return
		}
		c.send(proto.NewBranchList(m.ID, current, local, remote))
	}()
	return nil
}

// accountList may fetch usage over the network, so it runs off the read loop.
func (c *conn) accountList(m proto.ClientMessage) error {
	a := c.s.Workspace.Accounts
	if a == nil {
		c.send(proto.NewAccountList(m.ID, nil))
		return nil
	}
	go func() {
		ctx, cancel := context.WithTimeout(c.ctx, usageWait)
		defer cancel()
		c.send(proto.NewAccountList(m.ID, a.List(ctx, m.Provider)))
	}()
	return nil
}

func (c *conn) agentAccount(m proto.ClientMessage) error {
	if _, err := c.s.Agents.Get(m.AgentID); err != nil {
		return err
	}
	if c.s.Workspace.Switch == nil {
		return errors.New("This session can't switch accounts")
	}
	if err := c.s.Workspace.Switch(m.AgentID, m.Account); err != nil {
		if errors.Is(err, accounts.ErrUnknown) {
			c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownAccount, err.Error()))
			return nil
		}
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
