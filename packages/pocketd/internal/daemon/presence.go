package daemon

import (
	"context"
	"errors"
	"slices"
	"sync"
	"syscall"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/claude"
	"pocketd/internal/proc"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
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
	prompted   bool // codex started on a prompt: its first thread is its own
	thread     string
	unfollow   context.CancelFunc
	asks       map[string]int // open permission requests by permissionKey
	launch     state.Launch
	hooked     bool // a SessionStart came; a restored Agent has its Conversation before that
}

func (d *Daemon) startAgent(t *terminal.Terminal, provider string, p proc.Proc) *presence {
	ctx, cancel := context.WithCancel(context.Background())
	pr := &presence{t: t, pid: p.Pid, provider: provider, argv: p.Argv, env: p.Env, ctx: ctx, cancel: cancel, asks: map[string]int{},
		launch: state.Parse(provider, p.Argv)}
	if provider == "codex" && prompted(p.Argv) {
		pr.prompted, pr.enter = true, time.Now()
	}
	info := t.Info()
	driver := func(a *agent.Agent) agent.Driver { return termDriver{t: t, a: a, pid: p.Pid} }
	if h := d.adopt(pr, driver); h != nil {
		pr.a = h.a
	} else {
		pr.a = d.Agents.AddFunc(terminal.NewID(), info.Cwd, provider, driver)
	}
	pr.a.SetTerminal(info.ID)
	pr.a.SetEffort(pr.launch.Effort)
	if o := t.TakeOrigin(); o != "" {
		pr.a.SetOrigin(o)
	}
	d.place(pr)
	d.mu.Lock()
	if d.present == nil {
		d.present = map[string]*presence{}
	}
	d.present[info.ID] = pr
	d.mu.Unlock()
	return pr
}

// place names the Project and Worktree pr's Terminal was opened in. The
// branch can move under a running agent, so each turn end places it again.
func (d *Daemon) place(pr *presence) {
	p, _ := worktree.Find(d.Registry.Load(), pr.t.Info().Cwd)
	pr.a.SetLocation(p.Project, p.Worktree, p.Branch, p.Main)
}

func (d *Daemon) turnEnded(pr *presence, failed bool) {
	pr.a.TurnEnded(failed)
	d.place(pr)
}

// ClaudeAttachWait is how long a new claude has to send its SessionStart.
// Claude skips hooks in a folder the user hasn't trusted.
var ClaudeAttachWait = 5 * time.Second

// attach connects pr to its provider's status. A claude starts attached, so
// it doesn't flash "not attached" before its first hook.
func (d *Daemon) attach(pr *presence) {
	if h := d.hintOf(pr); h != nil && h.handoff {
		defer d.handedOver(pr, h)
	}
	if pr.provider == "codex" {
		d.attachCodex(pr)
		if h := d.hintOf(pr); h != nil && pr.sock != "" {
			d.bind(pr, h.saved.ConversationID)
		}
		return
	}
	if h := d.hintOf(pr); h != nil && claude.InProjects(h.saved.TranscriptPath, pr.env) {
		pr.mu.Lock()
		if pr.transcript == "" {
			pr.transcript, pr.stopTail = h.saved.TranscriptPath, d.tail(pr, h.saved.TranscriptPath, "")
		}
		pr.mu.Unlock()
	}
	expired := time.After(ClaudeAttachWait)
	go func() {
		select {
		case <-pr.ctx.Done():
		case <-expired:
			pr.mu.Lock()
			defer pr.mu.Unlock()
			if !pr.hooked {
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

func (pr *presence) setMode(permissionMode string) {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	pr.launch = state.Mode(pr.launch, permissionMode)
}

// save puts pr's Conversation in e, once there is one to resume.
func (pr *presence) save(e *state.Terminal) {
	s := pr.a.Summary()
	if s.ProviderSessionID == "" {
		return
	}
	pr.mu.Lock()
	launch := pr.launch
	e.TranscriptPath = pr.transcript
	pr.mu.Unlock()
	e.Provider, e.ConversationID, e.Launch = pr.provider, s.ProviderSessionID, &launch
	e.AgentID, e.CreatedAt, e.Status, e.Failed, e.Origin, e.Pinned = s.ID, s.CreatedAt, s.Status, s.Failed, s.Origin, s.Pinned
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
	h := d.restoring[id]
	if h != nil && h.pr == pr {
		h.pr = nil
	} else {
		h = nil
	}
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
	if h == nil {
		d.Agents.Remove(pr.a.ID())
		return
	}
	d.finish(h, "agent_exited")
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
