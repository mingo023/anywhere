package wsserver

import (
	"cmp"
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/hostop"
	"pocketd/internal/hosts"
	"pocketd/internal/launch"
	"pocketd/internal/ops"
	"pocketd/internal/orch"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/terminal"
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
	// SubagentTrees names the worktrees the subagents of the sessions in a worktree worked in, which go with it; nil names none.
	SubagentTrees func(tree string, treeOf func(cwd string, at int64) string) (orch.Subtrees, error)
	// HostDelete deletes a worktree on a linked host; nil deletes none there.
	HostDelete func(ctx context.Context, host, path string, subagents []string, d hosts.Deletion) error
	// HostInspect is what deleting a worktree on a linked host would take there; nil inspects none there.
	HostInspect func(ctx context.Context, host, path string) (proto.WorktreeInspect, error)
	// Env is the login env a teardown script runs in; nil runs it in pocketd's own.
	Env func() []string
	// TrashDir is where a trashed worktree goes on an OS without a Trash.
	TrashDir string
	// Session is the store's row for a session, open or closed; nil knows none.
	Session func(id string) (*orch.Session, error)
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
	case "worktree.inspect":
		if !slices.Contains(c.caps, proto.CapWorktrees) {
			return proto.ErrMalformed
		}
		return c.worktreeInspect(m)
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
		made, fl := c.s.Launch.Worktree(w, m.RequestID, n, func(step, note string) {
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
	d := hosts.Deletion{DeleteBranch: m.DeleteBranch, Force: m.Force, Trash: m.Trash, TeardownSecs: cmp.Or(m.TeardownSecs, defaultTeardownSecs), Teardown: m.Teardown == nil || *m.Teardown, Branch: m.Branch}
	go func() {
		if err := c.removeWorktree(m.Path, m.Subagents, d); err != nil {
			c.send(workspaceError(m.ID, err))
			return
		}
		c.send(proto.NewAck(m.ID))
	}()
	return nil
}

// defaultTeardownSecs bounds a teardown script when the client names no limit.
const defaultTeardownSecs = 120

func (c *conn) worktreeInspect(m proto.ClientMessage) error {
	if c.s.Registry == nil {
		return errors.New("This pocketd can't inspect worktrees")
	}
	go func() {
		trees, lost, anyway, err := c.inspect(m.Path)
		if err != nil {
			c.send(workspaceError(m.ID, err))
			return
		}
		c.send(proto.NewWorktreeInspect(m.ID, trees, lost, anyway))
	}()
	return nil
}

// inspect is what deleting the worktree at path takes: it and its subagents' worktrees, here then on linked hosts,
// as removeWorktree would find them. An offline host's are left out, as the delete skips them.
func (c *conn) inspect(path string) ([]proto.InspectedTree, int, int, error) {
	f := c.s.Registry.Load()
	tree, err := findTree(f, path)
	if err != nil {
		return nil, 0, 0, err
	}
	subtrees, onHosts, err := c.subagentTrees(f, tree.Path)
	if err != nil {
		return nil, 0, 0, err
	}
	line := c.openIn(tree.Path)
	subtrees = slices.DeleteFunc(subtrees, func(t linkedTree) bool { return c.othersIn(t.Path, line) })
	out := []proto.InspectedTree{c.inspected(tree)}
	lost, anyway := 0, 0
	branches := map[string][]string{}
	if tree.Branch == "" {
		lost = worktree.DetachedCommits(tree.Path)
	}
	for _, t := range append(subtrees, tree) {
		if t.Branch == "" && t.Path != tree.Path {
			anyway += worktree.DetachedCommits(t.Path)
		}
		if t.Branch != "" && tree.Branch != "" {
			branches[t.project] = append(branches[t.project], t.Branch)
		}
	}
	for project, bs := range branches {
		lost += worktree.LostCommits(project, bs...)
	}
	for _, t := range subtrees {
		out = append(out, c.inspected(t))
	}
	if ask := c.s.Workspace.HostInspect; ask != nil {
		for _, h := range onHosts {
			ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
			there, err := ask(ctx, h.Host, h.Path)
			cancel()
			if _, offline := errors.AsType[hosts.Offline](err); offline {
				continue
			}
			if r, ok := errors.AsType[hosts.Refusal](err); ok && r.Reply.Code == worktree.Code(worktree.ErrUnknownWorktree) {
				continue
			}
			if err != nil {
				return nil, 0, 0, err
			}
			for _, t := range there.Trees {
				t.Host = cmp.Or(t.Host, h.Host)
				out = append(out, t)
			}
			// A detached child's own commits go whether or not the branches stay.
			if len(there.Trees) > 0 && there.Trees[0].Branch == "" {
				anyway += there.Lost
			} else {
				lost += there.Lost
			}
			anyway += there.LostAnyway
		}
	}
	return out, lost, anyway, nil
}

func (c *conn) inspected(t linkedTree) proto.InspectedTree {
	dirty, _ := worktree.DirtyFiles(t.Path)
	return proto.InspectedTree{Path: t.Path, Branch: t.Branch, Dirty: dirty, Terminals: len(c.terminalsIn(t.Path))}
}

// removeWorktree closes the terminals opened in the worktree at path, removes
// its folder and its name, and its branch when asked; the same for each
// worktree Anywhere made for its sessions' subagents, here first and on linked
// hosts before that, so a failure leaves the worktree at path to retry from.
// The main worktree, branches whose commits nothing else holds, and a host
// that refuses or is offline refuse the delete before anything here closes,
// unless forced. A parent's Mac deleting its subagents' worktree here names
// them: a worktree someone else works in, or made since they closed, isn't theirs.
func (c *conn) removeWorktree(path string, subagents []string, d hosts.Deletion) error {
	deleteBranch, force := d.DeleteBranch, d.Force
	f := c.s.Registry.Load()
	tree, err := findTree(f, path)
	if err != nil {
		return err
	}
	if d.Branch != "" && d.Branch != tree.Branch {
		deleteBranch = false
	}
	if subagents != nil && c.othersIn(tree.Path, subagents) {
		return worktree.ErrInUse
	}
	if at := worktree.MadeAt(tree.Path); subagents != nil && (at == 0 || at > c.madeBy(tree.Path, subagents)) {
		return worktree.ErrUnknownWorktree
	}
	subtrees, onHosts, err := c.subagentTrees(f, tree.Path)
	if err != nil {
		return err
	}
	line := append(slices.Clone(subagents), c.openIn(tree.Path)...)
	subtrees = slices.DeleteFunc(subtrees, func(t linkedTree) bool { return c.othersIn(t.Path, line) })
	trees := append(subtrees, tree)
	if !force {
		if n := lostCommits(trees, deleteBranch); n > 0 {
			return unpushedCommits(n)
		}
	}
	if del := c.s.Workspace.HostDelete; del != nil {
		for _, h := range onHosts {
			ctx, cancel := context.WithTimeout(context.Background(), time.Duration(d.TeardownSecs)*time.Second+time.Minute)
			err := del(ctx, h.Host, h.Path, h.Sessions, d)
			cancel()
			// An offline host's go when its pocketd closes their sessions and cleans up after them.
			if _, offline := errors.AsType[hosts.Offline](err); offline {
				continue
			}
			if err != nil && (!force || isTeardownFailure(err)) {
				return err
			}
		}
	}
	c.closeTerminals(trees)
	for _, t := range trees {
		if d.Teardown {
			if err := c.teardown(f, t, d.TeardownSecs); err != nil {
				return err
			}
		}
	}
	for _, t := range trees {
		if err := c.removeTree(f, t, deleteBranch, force, d.Trash); err != nil {
			return err
		}
	}
	return nil
}

// teardownFailed is a worktree whose teardown script failed: nothing of it was deleted.
type teardownFailed struct {
	path string
	worktree.TeardownFailed
}

func isTeardownFailure(err error) bool {
	if r, ok := errors.AsType[hosts.Refusal](err); ok {
		return r.Reply.Code == proto.CodeTeardownFailed
	}
	_, ok := errors.AsType[teardownFailed](err)
	return ok
}

// teardown runs the project's teardown script in t, as its settings or its repo name it. A non-owner's comes from the
// project's own checkout only, as its setup does: a branch or PR it picked could carry any script.
func (c *conn) teardown(f registry.File, t linkedTree, secs int) error {
	from := t.Path
	if c.who.Kind != peer.Owner {
		from = t.project
	}
	script := worktree.Teardown(f.Repos[t.project].Teardown, t.project, from)
	env := os.Environ()
	if c.s.Workspace.Env != nil {
		env = c.s.Workspace.Env()
	}
	err := worktree.RunTeardown(shellenv.LoginShell(), script, t.project, t.Path, env, time.Duration(secs)*time.Second)
	if failed, ok := errors.AsType[worktree.TeardownFailed](err); ok {
		return teardownFailed{t.Path, failed}
	}
	return err
}

// lostCommits is what deleting trees together loses: their branches' commits
// counted as one set when those go, and what detached HEADs alone hold.
func lostCommits(trees []linkedTree, deleteBranch bool) int {
	branches := map[string][]string{}
	n := 0
	for _, t := range trees {
		switch {
		case t.Branch == "":
			n += worktree.DetachedCommits(t.Path)
		case deleteBranch:
			branches[t.project] = append(branches[t.project], t.Branch)
		}
	}
	for project, bs := range branches {
		n += worktree.LostCommits(project, bs...)
	}
	return n
}

type linkedTree struct {
	project string
	worktree.Worktree
}

func findTree(f registry.File, path string) (linkedTree, error) {
	path = filepath.Clean(path)
	project, ok := worktree.Containing(f, path)
	if !ok || !filepath.IsAbs(path) {
		return linkedTree{}, worktree.ErrUnknownWorktree
	}
	trees, err := worktree.List(project)
	if err != nil {
		return linkedTree{}, worktree.ErrUnknownProject
	}
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	i := slices.IndexFunc(trees, func(w worktree.Worktree) bool { return w.Path == path })
	switch {
	case i < 0:
		return linkedTree{}, worktree.ErrUnknownWorktree
	case trees[i].Main:
		return linkedTree{}, worktree.ErrMain
	}
	return linkedTree{project, trees[i]}, nil
}

// SubagentWorktrees is each worktree that the subagents of the sessions in the
// worktree at path worked in, every generation down, as this machine's lineage
// holds them, closed or open: here, and on the hosts it delegated to.
func (s *Server) SubagentWorktrees(path string) ([]ops.Tree, error) {
	f := s.Registry.Load()
	tree, err := findTree(f, path)
	if errors.Is(err, worktree.ErrUnknownWorktree) {
		// The lineage places sessions by the same lookup, so none can be under it.
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	c := &conn{s: s}
	subtrees, onHosts, err := c.subagentTrees(f, tree.Path)
	if err != nil {
		return nil, err
	}
	out := make([]ops.Tree, 0, len(subtrees)+len(onHosts))
	for _, t := range subtrees {
		out = append(out, ops.Tree{Path: t.Path, Since: worktree.MadeAt(t.Path)})
	}
	for _, h := range onHosts {
		ids := make([]string, len(h.Sessions))
		for i, id := range h.Sessions {
			ids[i] = h.Host + "/" + id
		}
		out = append(out, ops.Tree{Host: h.Host, Path: h.Path, Since: c.madeBy(h.Path, ids)})
	}
	return out, nil
}

// subagentTrees fails when the lineage can't be read: the tree at path is what
// would name them again, so it must stay.
func (c *conn) subagentTrees(f registry.File, path string) ([]linkedTree, []orch.HostTree, error) {
	if c.s.Workspace.SubagentTrees == nil {
		return nil, nil, nil
	}
	listed := map[string][]worktree.Worktree{}
	treeOf := func(cwd string, at int64) string {
		if real, err := filepath.EvalSymlinks(cwd); err == nil {
			cwd = real
		}
		project, ok := worktree.Containing(f, cwd)
		if !ok {
			return ""
		}
		if _, ok := listed[project]; !ok {
			listed[project], _ = worktree.List(project)
		}
		best := ""
		for _, w := range listed[project] {
			if inside(cwd, w.Path) && len(w.Path) > len(best) {
				best = w.Path
			}
		}
		if worktree.MadeAt(best) > at {
			return ""
		}
		return best
	}
	found, err := c.s.Workspace.SubagentTrees(path, treeOf)
	if err != nil {
		return nil, nil, err
	}
	var out []linkedTree
	for _, p := range found.Local {
		if t, err := findTree(f, p); err == nil && t.Made && !slices.ContainsFunc(out, func(o linkedTree) bool { return o.Path == t.Path }) {
			out = append(out, t)
		}
	}
	return out, found.Hosts, nil
}

// madeBy is when the worktree at tree was made by, on this machine's clock, if
// it's the one the sessions named worked in: before the last of them there
// started, as a session starts in its worktree and a newer one is made only
// once they're gone. A store's close and update times move later; this doesn't.
// 0 when none is known, as when the host pruned them: nothing then proves it theirs.
func (c *conn) madeBy(tree string, sessions []string) int64 {
	by := int64(0)
	for _, id := range sessions {
		var s *orch.Session
		if c.s.Workspace.Session != nil {
			s, _ = c.s.Workspace.Session(id)
		}
		if s == nil {
			continue
		}
		cwd := s.Cwd
		if real, err := filepath.EvalSymlinks(cwd); err == nil {
			cwd = real
		}
		if inside(cwd, tree) {
			by = max(by, s.CreatedAt)
		}
	}
	return by
}

// othersIn is whether a terminal in tree belongs to neither the open sessions
// named nor their subagents.
func (c *conn) othersIn(tree string, sessions []string) bool {
	if c.s.Terminals == nil {
		return false
	}
	ours := map[string]bool{}
	for _, id := range sessions {
		ours[id] = true
	}
	theirs := map[string]bool{}
	if c.s.OpenSessions != nil {
		open, _ := c.s.OpenSessions()
		for grew := true; grew; {
			grew = false
			for _, s := range open {
				if !ours[s.ID] && ours[s.ParentID] && s.Relationship == "subagent" {
					ours[s.ID], grew = true, true
				}
			}
		}
		for _, s := range open {
			if ours[s.ID] && s.Terminal != "" {
				theirs[s.Terminal] = true
			}
		}
	}
	return slices.ContainsFunc(c.s.Terminals.All(), func(t *terminal.Terminal) bool {
		info := t.Info()
		return info.Local == "" && inside(info.Cwd, tree) && !theirs[info.ID]
	})
}

// openIn is the open sessions working in tree.
func (c *conn) openIn(tree string) []string {
	if c.s.OpenSessions == nil {
		return nil
	}
	open, _ := c.s.OpenSessions()
	var ids []string
	for _, s := range open {
		cwd := s.Cwd
		if real, err := filepath.EvalSymlinks(cwd); err == nil {
			cwd = real
		}
		if inside(cwd, tree) {
			ids = append(ids, s.ID)
		}
	}
	return ids
}

// terminalsIn is the terminals working in tree; a Local's run in its project's checkout but belong to none.
func (c *conn) terminalsIn(tree string) []*terminal.Terminal {
	if c.s.Terminals == nil {
		return nil
	}
	var in []*terminal.Terminal
	for _, t := range c.s.Terminals.All() {
		if info := t.Info(); info.Local == "" && inside(info.Cwd, tree) {
			in = append(in, t)
		}
	}
	return in
}

func (c *conn) closeTerminals(trees []linkedTree) {
	var closing []<-chan struct{}
	for _, w := range trees {
		for _, t := range c.terminalsIn(w.Path) {
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

// removeTree removes the worktree t, after moving its folder to the Trash when trash is set, and its branch when
// deleteBranch is.
func (c *conn) removeTree(f registry.File, t linkedTree, deleteBranch, force, trash bool) error {
	path, project := t.Path, t.project
	t.Branch = branchToDelete(project, path, t.Branch, force)
	trashed := ""
	if trash {
		var err error
		if trashed, err = hostop.Trash(path, c.s.Workspace.TrashDir); err != nil {
			log.Printf("worktree: trashing %s: %v", path, err)
			trashed = ""
		}
	}
	if err := worktree.Remove(f, project, path); err != nil {
		// Naming it from its prompt may be writing its branch's lock into the git folder git is deleting.
		time.Sleep(time.Second)
		if err := worktree.Remove(f, project, path); err != nil {
			if trashed != "" {
				os.Rename(trashed, path)
			}
			return err
		}
	}
	if c.s.Names != nil {
		c.s.Names.Forget(path)
	}
	if err := c.s.Registry.AgentWorktree(path, false); err != nil {
		log.Printf("worktree: forgetting %s: %v", path, err)
	}
	if p := c.s.Workspace.Projects; p != nil {
		defer p.Scan()
	}
	if branch := t.Branch; deleteBranch && branch != "" {
		return dropBranch(project, branch, force)
	}
	return nil
}

// dropBranch deletes branch, or what naming its worktree from its prompt
// renamed it to since it was checked, which may land after its worktree went.
func dropBranch(project, branch string, force bool) error {
	out, err := exec.Command("git", "-C", project, "branch", "-D", "--", branch).CombinedOutput()
	if err == nil {
		return nil
	}
	failed := branchFailed{strings.TrimSpace(string(out))}
	if exec.Command("git", "-C", project, "show-ref", "--verify", "--quiet", "refs/heads/"+branch).Run() == nil {
		return failed
	}
	refs, _ := exec.Command("git", "-C", project, "for-each-ref", "--format=%(refname:short)", "refs/heads").Output()
	for _, b := range strings.Fields(string(refs)) {
		if !renamedFrom(project, b, branch) {
			continue
		}
		if !force && worktree.LostCommits(project, b) > 0 {
			return nil
		}
		if out, err := exec.Command("git", "-C", project, "branch", "-D", "--", b).CombinedOutput(); err != nil {
			return branchFailed{strings.TrimSpace(string(out))}
		}
		return nil
	}
	return failed
}

// branchToDelete is the branch that goes with the worktree at path, whose
// branch was checked as checked. Naming a worktree from its prompt renames its
// branch, maybe since: follow the rename, unless forced only when it loses
// nothing unchecked.
func branchToDelete(project, path, checked string, force bool) string {
	if checked == "" || exec.Command("git", "-C", project, "show-ref", "--verify", "--quiet", "refs/heads/"+checked).Run() == nil {
		return checked
	}
	trees, err := worktree.List(project)
	if err != nil {
		return ""
	}
	i := slices.IndexFunc(trees, func(w worktree.Worktree) bool { return w.Path == path })
	if i < 0 || !renamedFrom(project, trees[i].Branch, checked) || !force && worktree.LostCommits(project, trees[i].Branch) > 0 {
		return ""
	}
	return trees[i].Branch
}

// renamedFrom is whether branch was once named old, per the entry `git branch -m` leaves in its reflog
// (renamed_from in crates/git/src/git.rs).
func renamedFrom(project, branch, old string) bool {
	out, _ := exec.Command("git", "-C", project, "log", "-g", "--format=%gs", "refs/heads/"+branch, "--").Output()
	return branch != "" && slices.ContainsFunc(strings.Split(string(out), "\n"), func(l string) bool {
		return strings.HasPrefix(l, "Branch: renamed refs/heads/"+old+" to ")
	})
}

type branchFailed struct{ stderr string }

func (g branchFailed) Error() string {
	line, _, _ := strings.Cut(g.stderr, "\n")
	return line
}

// unpushedCommits is how many commits deleting the branch would lose.
type unpushedCommits int

func (n unpushedCommits) Error() string {
	if n == 1 {
		return "1 commit is only on this branch"
	}
	return fmt.Sprintf("%d commits are only on this branch", int(n))
}

func workspaceError(id string, err error) proto.Error {
	if r, ok := errors.AsType[hosts.Refusal](err); ok {
		r.Reply.ID = id
		return r.Reply
	}
	if n, ok := errors.AsType[unpushedCommits](err); ok {
		e := proto.NewErrorCode(id, proto.CodeUnpushedCommits, n.Error())
		e.Count = int(n)
		return e
	}
	if g, ok := errors.AsType[branchFailed](err); ok {
		return proto.NewCodedError(id, proto.CodeGitFailed, g.Error(), g.stderr)
	}
	if t, ok := errors.AsType[teardownFailed](err); ok {
		e := proto.NewCodedError(id, proto.CodeTeardownFailed, t.Error(), t.Tail)
		e.Path = t.path
		return e
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
