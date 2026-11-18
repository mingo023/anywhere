package launch

import (
	"context"
	"errors"
	"log"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/agent"
	"pocketd/internal/naming"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// job names a new session from its prompt, and its Worktree when that Worktree's name is a placeholder.
type job struct {
	provider, exe, prompt string
	env                   []string
	// tree is "" unless naming may name the Worktree; branch is "" unless it may also rename the branch.
	project, tree, branch string
	// local is the Local the session started in, which naming names while its name is a placeholder.
	local string
	// retitle: the claude Pocket started was named a placeholder, which naming names its conversation over.
	retitle bool
}

const tries = 3

var (
	replyWait = 15 * time.Minute
	replyPoll = time.Second
)

var errNoNaming = errors.New("naming: this agent has no headless run to name with")

func generate(provider, exe, prompt string, env []string) (naming.Result, error) {
	argv := naming.Command(provider, exe, prompt)
	if argv == nil {
		return naming.Result{}, errNoNaming
	}
	if !filepath.IsAbs(exe) {
		argv = InShell(shellOf(env), argv)
	}
	return naming.Generate(context.Background(), argv, env)
}

// Prompted names a's session from a prompt typed into it. A create names
// the session it starts itself, with the Worktree and Local it made.
func (l *Launcher) Prompted(a *agent.Agent, prompt string, env []string) {
	l.mu.Lock()
	_, creating := l.pending[a.Summary().TerminalID]
	l.mu.Unlock()
	if creating {
		return
	}
	claim, ok := a.ClaimNaming()
	if !ok {
		return
	}
	go func() {
		if exe, ok := l.find(a.Provider(), env); ok {
			l.name(a, claim, job{provider: a.Provider(), exe: exe, env: env, prompt: prompt})
		}
	}()
}

// name runs on claim, which a.ClaimNaming gave.
func (l *Launcher) name(a *agent.Agent, claim int, j job) {
	renamable := j.branch != "" && worktree.Renamable(j.tree, j.branch)
	var named *naming.Result
	reply := ""
	for try := 0; try < tries; try++ {
		n, err := l.generate(j.provider, j.exe, naming.Prompt(j.prompt, reply), j.env)
		if errors.Is(err, errNoNaming) {
			// No failure: such an agent titles its session itself, or leaves it unnamed.
			if n.Title = ownTitle(a); n.Title == "" {
				return
			}
		} else if errors.Is(err, naming.ErrLimit) && l.moveOff(&j) {
			// Accounts it moved off count as at their limit, so moves end.
			try--
			continue
		} else if err != nil {
			continue
		}
		named = &n
		l.title(a, claim, j, n.Title)
		if !n.Vague || reply != "" {
			break
		}
		if reply = firstReply(a); reply == "" {
			break
		}
	}
	if named == nil {
		if l.Names != nil {
			l.Names.Failed(a.ID())
		}
		return
	}
	if j.local != "" && named.Title != "" && l.Locals != nil {
		l.Locals.Settle(j.local)
	}
	if !renamable || named.Branch == "" {
		return
	}
	if _, err := worktree.RenameBranch(j.project, j.tree, j.branch, named.Branch); err != nil {
		log.Printf("naming: %s: %v", j.tree, err)
		return
	}
	if p, ok := worktree.Find(l.reg.Load(), j.tree); ok {
		a.SetLocation(p.Project, p.Worktree, p.Branch, p.Main)
	}
}

// moveOff moves j off the account that refused it at a limit, onto the
// signed-in one with the most left, as an agent at its limit moves; false
// when switching at a limit is off, every account is at one, or the CLI is
// a shell function.
func (l *Launcher) moveOff(j *job) bool {
	s := l.d.Accounts
	// A CLI the shell finds runs after the user's rc, which can set the account back.
	if s == nil || !filepath.IsAbs(j.exe) {
		return false
	}
	from := s.Of(j.provider, j.env)
	if from == "" {
		from = accounts.BuiltIn
	}
	s.Hit(j.provider, from)
	if !s.AutoSwitch() {
		return false
	}
	to, ok := s.Best(j.provider, from)
	if !ok {
		return false
	}
	account := s.Env(j.provider, to)
	var drop []string
	for _, kv := range slices.Concat(s.Env(j.provider, from), account) {
		k, _, _ := strings.Cut(kv, "=")
		drop = append(drop, k)
	}
	if account != nil {
		// As the shell a managed account's agent runs in drops them.
		drop = append(drop, accounts.Unset...)
	}
	j.env = slices.Concat(slices.DeleteFunc(slices.Clone(j.env), func(kv string) bool {
		k, _, _ := strings.Cut(kv, "=")
		return slices.Contains(drop, k)
	}), account)
	return true
}

func (l *Launcher) title(a *agent.Agent, claim int, j job, title string) {
	if title == "" {
		return
	}
	a.SetNamed(claim, title)
	if j.retitle {
		term := a.Summary().TerminalID
		l.d.Retitle(func(t terminal.Info) bool { return t.ID == term }, title)
	}
	if j.tree != "" && l.Names != nil {
		l.Names.Auto(j.tree, title)
	}
	if j.local != "" && l.Locals != nil {
		l.Locals.Auto(j.local, title)
	}
}

// ownTitle waits for the title a's agent gives its session; "" when a closes
// or replyWait passes first.
func ownTitle(a *agent.Agent) string {
	for end := time.Now().Add(replyWait); time.Now().Before(end); time.Sleep(replyPoll) {
		if title := a.OwnTitle(); title != "" || a.Turn() == "closed" {
			return title
		}
	}
	return ""
}

// firstReply waits for a's first turn to end and returns what the agent last
// said; "" when a closes or replyWait passes first.
func firstReply(a *agent.Agent) string {
	for end := time.Now().Add(replyWait); time.Now().Before(end); time.Sleep(replyPoll) {
		switch a.Turn() {
		case "closed":
			return ""
		case "idle", "done":
			items, _ := a.Timeline.Page(0, 500)
			for i := len(items) - 1; i >= 0; i-- {
				if items[i].Kind == "assistant" && items[i].Text != "" {
					return items[i].Text
				}
			}
		}
	}
	return ""
}

// Retitle names a's session again from its first prompt and latest reply.
// Only the session's title changes: its Worktree and branch keep their names.
func (l *Launcher) Retitle(a *agent.Agent) error {
	items, _ := a.Timeline.Page(0, 1<<20)
	prompt, reply := "", ""
	for _, it := range items {
		if it.ParentToolUseID != "" {
			continue
		}
		if it.Kind == "user" && prompt == "" {
			prompt = it.Text
		}
		if it.Kind == "assistant" && it.Text != "" {
			reply = it.Text
		}
	}
	if prompt == "" {
		return errors.New("This session has no prompt to name it from")
	}
	env := l.d.LoginEnv()
	exe, ok := l.find(a.Provider(), env)
	if !ok {
		return errors.New(command(a.Provider()) + " isn't installed on your Mac")
	}
	n, err := l.generate(a.Provider(), exe, naming.Prompt(prompt, reply), env)
	if err != nil {
		return err
	}
	if n.Title == "" {
		return errors.New("Naming gave no title")
	}
	a.Rename(n.Title)
	return nil
}
