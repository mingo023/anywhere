package wsserver

import (
	"errors"
	"log"
	"path/filepath"
	"slices"
	"sync"
	"time"

	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// reaping keeps one agent worktree's cleanup at a time: two sessions there may close together.
var reaping sync.Mutex

// AgentClosed cleans up after a session that closed, in terminal, at cwd: the worktree pocketd made for an agent's
// session there goes once nobody else works in it (ReapAgentWorktree).
func (s *Server) AgentClosed(session, cwd, terminal string) {
	if s.Registry == nil || cwd == "" {
		return
	}
	if real, err := filepath.EvalSymlinks(cwd); err == nil {
		cwd = real
	}
	for _, tree := range s.Registry.Load().AgentWorktrees {
		if inside(cwd, tree) {
			s.reap(tree, session, terminal)
			return
		}
	}
}

// ReapAgentWorktree deletes the agent's worktree at tree, with its teardown and its branch when the branch holds no
// commit of its own, once no open session or terminal is in it. One holding uncommitted work, or a teardown or delete
// that fails, stays and leaves the agents' list, so the sidebar shows it.
func (s *Server) ReapAgentWorktree(tree string) { s.reap(tree, "", "") }

// reap is ReapAgentWorktree for the closed session's sake, whose own terminal it closes.
func (s *Server) reap(tree, session, own string) {
	reaping.Lock()
	defer reaping.Unlock()
	f := s.Registry.Load()
	if !slices.Contains(f.AgentWorktrees, tree) {
		return
	}
	t, err := findTree(f, tree)
	if errors.Is(err, worktree.ErrUnknownWorktree) {
		s.forgetAgentTree(tree, "it's gone")
		return
	}
	if err != nil {
		log.Printf("worktree: cleaning up %s: %v", tree, err)
		return
	}
	c := &conn{s: s}
	if slices.ContainsFunc(c.openIn(tree), func(id string) bool { return id != session }) {
		return
	}
	terminals := c.terminalsIn(tree)
	if others := slices.DeleteFunc(slices.Clone(terminals), func(x *terminal.Terminal) bool { return x.Info().ID == own }); len(others) > 0 {
		go func() {
			for _, x := range others {
				<-x.Done()
			}
			s.reap(tree, session, "")
		}()
		return
	}
	if dirty, err := worktree.DirtyFiles(tree); err != nil || dirty > 0 || t.Branch == "" && worktree.DetachedCommits(tree) > 0 {
		s.forgetAgentTree(tree, "it holds work nothing else does")
		return
	}
	for _, x := range terminals {
		x.Close()
		select {
		case <-x.Done():
		case <-time.After(closeWait):
		}
	}
	if err := c.teardown(f, t, defaultTeardownSecs); err != nil {
		s.forgetAgentTree(tree, err.Error())
		return
	}
	deleteBranch := t.Branch != "" && worktree.LostCommits(t.project, t.Branch) == 0
	if err := c.removeTree(f, t, deleteBranch, false, false); err != nil {
		s.forgetAgentTree(tree, err.Error())
		return
	}
	log.Printf("worktree: removed %s, its agent's session closed", tree)
}

// forgetAgentTree drops tree from the agents' worktrees, which puts it on the sidebar.
func (s *Server) forgetAgentTree(tree, why string) {
	log.Printf("worktree: keeping %s: %s", tree, why)
	if err := s.Registry.AgentWorktree(tree, false); err != nil {
		log.Printf("worktree: %v", err)
	}
	if p := s.Workspace.Projects; p != nil {
		p.Changed()
	}
}
