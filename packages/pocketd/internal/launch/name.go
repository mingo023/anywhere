package launch

import (
	"context"
	"log"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/naming"
	"pocketd/internal/worktree"
)

// job names a new session from its prompt, and the Worktree it made when its Name was a placeholder.
type job struct {
	provider, exe, prompt, agent string
	env                          []string
	// tree is "" unless naming may name the Worktree and rename its branch.
	project, tree, branch string
}

const tries = 3

var (
	replyWait = 15 * time.Minute
	replyPoll = time.Second
)

func generate(provider, exe, prompt string, env []string) (naming.Result, error) {
	return naming.Generate(context.Background(), naming.Command(provider, exe, prompt), env)
}

func (l *Launcher) name(j job) {
	a, err := l.d.Agents.Get(j.agent)
	if err != nil {
		return
	}
	renamable := j.tree != "" && worktree.Renamable(j.tree, j.branch)
	var named *naming.Result
	reply := ""
	for try := 0; try < tries; try++ {
		n, err := l.generate(j.provider, j.exe, naming.Prompt(j.prompt, reply), j.env)
		if err != nil {
			continue
		}
		named = &n
		l.title(a, j.tree, n.Title)
		if !n.Vague || reply != "" {
			break
		}
		if reply = firstReply(a); reply == "" {
			break
		}
	}
	if named == nil {
		if l.Names != nil {
			l.Names.Failed(j.agent)
		}
		return
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

func (l *Launcher) title(a *agent.Agent, tree, title string) {
	if title == "" {
		return
	}
	a.SetNamed(title)
	if tree != "" && l.Names != nil {
		l.Names.Auto(tree, title)
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
