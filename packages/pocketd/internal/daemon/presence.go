package daemon

import (
	"context"
	"errors"
	"slices"
	"sync"
	"syscall"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/proc"
	"pocketd/internal/terminal"
)

// presence is the agent that process pid is, for as long as pid stays in t's foreground.
type presence struct {
	t         *terminal.Terminal
	a         *agent.Agent
	pid       int
	provider  string
	argv, env []string
	ctx       context.Context
	cancel    context.CancelFunc
	sock      string // codex's app-server; set under Daemon.mu

	mu         sync.Mutex
	transcript string
	stopTail   func()
	enter      time.Time
	thread     string
	unfollow   context.CancelFunc
	asks       map[string]int // open permission requests by permissionKey
}

func (d *Daemon) startAgent(t *terminal.Terminal, provider string, p proc.Proc) *presence {
	ctx, cancel := context.WithCancel(context.Background())
	pr := &presence{t: t, pid: p.Pid, provider: provider, argv: p.Argv, env: p.Env, ctx: ctx, cancel: cancel, asks: map[string]int{}}
	info := t.Info()
	pr.a = d.Agents.AddFunc(terminal.NewID(), info.Cwd, provider, func(a *agent.Agent) agent.Driver {
		return termDriver{t: t, a: a, pid: p.Pid}
	})
	pr.a.SetTerminal(info.ID)
	d.mu.Lock()
	if d.present == nil {
		d.present = map[string]*presence{}
	}
	d.present[info.ID] = pr
	d.mu.Unlock()
	return pr
}

// ClaudeAttachWait is how long a new claude has to send its SessionStart.
// Claude skips hooks in a folder the user hasn't trusted.
var ClaudeAttachWait = 5 * time.Second

// attach connects pr to its provider's status. A claude starts attached, so
// it doesn't flash "not attached" before its first hook.
func (d *Daemon) attach(pr *presence) {
	if pr.provider == "codex" {
		d.attachCodex(pr)
		return
	}
	expired := time.After(ClaudeAttachWait)
	go func() {
		select {
		case <-pr.ctx.Done():
		case <-expired:
			pr.mu.Lock()
			defer pr.mu.Unlock()
			if pr.a.Summary().ProviderSessionID == "" {
				pr.a.SetAttached(false)
			}
		}
	}()
}

// working is Working unless a permission request other than key's is open:
// a parallel tool's hooks must not hide that dialog. Key's own tool ending
// means its dialog was answered at the desk.
func (pr *presence) working(key string) {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	if len(pr.asks) == 0 || len(pr.asks) == 1 && pr.asks[key] > 0 {
		pr.a.Working()
	}
}

// endAgent ends pr once; the poller and a hook's observe can both find it gone.
func (d *Daemon) endAgent(pr *presence) {
	id := pr.t.Info().ID
	d.mu.Lock()
	if d.present[id] != pr {
		d.mu.Unlock()
		return
	}
	delete(d.present, id)
	d.unwatch(pr.sock)
	d.mu.Unlock()
	pr.cancel()
	pr.mu.Lock()
	if pr.stopTail != nil {
		pr.stopTail()
	}
	if pr.unfollow != nil {
		pr.unfollow()
	}
	pr.mu.Unlock()
	d.Broker.DenyAll(pr.a.ID())
	d.Agents.Remove(pr.a.ID())
}

type termDriver struct {
	t   *terminal.Terminal
	a   *agent.Agent
	pid int
}

var errNotForeground = errors.New("Agent is no longer in the foreground")

// Prompt, Interrupt and Compact refuse once pid left the foreground: the
// poller may not have noticed yet, and whatever they type would reach the shell.
func (c termDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.t.Prompt(text)
}

func (c termDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.t.Write([]byte{0x1b})
}

// Close leaves a pid that left the foreground alone: it may belong to another process by now.
func (c termDriver) Close() {
	if c.inForeground() {
		syscall.Kill(c.pid, syscall.SIGTERM)
	}
}

func (c termDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	c.a.SetCompacting()
	return c.t.Prompt("/compact")
}

func (c termDriver) inForeground() bool {
	_, procs := foreground(c.t)
	return slices.ContainsFunc(procs, func(p proc.Proc) bool { return p.Pid == c.pid })
}
