package launch

import (
	"context"
	"log"
	"path/filepath"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/naming"
	"pocketd/internal/worktree"
)

// job names a new session from its prompt, and the Worktree it made when its Name was a placeholder.
type job struct {
	provider, exe, prompt string
	env                   []string
	// tree is "" unless naming may name the Worktree and rename its branch.
	project, tree, branch string
	// local is the Local the session started in, which naming names while its name is a placeholder.
	local string
}

const tries = 3

var (
	replyWait = 15 * time.Minute
	replyPoll = time.Second
)

func generate(provider, exe, prompt string, env []string) (naming.Result, error) {
	argv := naming.Command(provider, exe, prompt)
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
	renamable := j.tree != "" && worktree.Renamable(j.tree, j.branch)
	var named *naming.Result
	reply := ""
	for try := 0; try < tries; try++ {
		n, err := l.generate(j.provider, j.exe, naming.Prompt(j.prompt, reply), j.env)
		if err != nil {
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

func (l *Launcher) title(a *agent.Agent, claim int, j job, title string) {
	if title == "" {
		return
	}
	a.SetNamed(claim, title)
	if j.tree != "" && l.Names != nil {
		l.Names.Auto(j.tree, title)
	}
	if j.local != "" && l.Locals != nil {
		l.Locals.Auto(j.local, title)
	}
}

// firstReply waits for a's first turn to end and returns what the agent last
// said; "" when a closes or replyWait passes first.
func firstReply(a *agent.Agent) string {
	for end := time.Now().Add(replyWait); time.Now().Before(end); time.Sleep(replyPoll) {
		switch a.Summary().Status {
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
