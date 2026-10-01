// Package launch starts Sessions from a LaunchSpec. It is the only code that
// builds an agent's command line.
package launch

import (
	"fmt"
	"slices"
	"sync"
	"time"

	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/events"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

const (
	poll       = 250 * time.Millisecond
	attachWait = 10 * time.Second
	probeTTL   = 30 * time.Second
)

// createWait bounds setup plus the agent's start, so a create that hangs
// still ends and its receipt can expire.
var createWait = 10 * time.Minute

// Who asked. Key separates one principal's request ids from another's.
type Who struct {
	Owner bool
	Key   string
}

type Failure struct{ Code, Message, Detail string }

func fail(code, message string) *Failure { return &Failure{Code: code, Message: message} }

type Creating struct {
	Terminal, Cwd string
	Setup         bool
}

type Result struct {
	AgentID, TerminalID string
	Err                 *Failure
}

type exit struct {
	phase  string
	status int
	tail   string
}

type found struct {
	path string
	at   time.Time
}

type Launcher struct {
	d        *daemon.Daemon
	reg      *registry.Registry
	set      *config.Settings
	ev       *events.Log
	receipts *receipts

	mu      sync.Mutex
	pending map[string]chan exit
	found   map[string]found
}

func New(d *daemon.Daemon, reg *registry.Registry, set *config.Settings, ev *events.Log) *Launcher {
	return &Launcher{d: d, reg: reg, set: set, ev: ev, receipts: newReceipts(time.Now), pending: map[string]chan exit{}, found: map[string]found{}}
}

// Create runs one agent.create. creating is called once the Terminal exists,
// before setup runs. A retry of the same request joins the first and gets its
// replies.
func (l *Launcher) Create(w Who, requestID string, s proto.LaunchSpec, creating func(Creating)) Result {
	x, fresh := l.receipts.take(w.Key+"\x00"+requestID, s)
	if x == nil {
		return Result{Err: fail("duplicate", "This request was already sent with a different spec")}
	}
	if !fresh {
		<-x.done
		c, r := l.receipts.outcome(x)
		if c.Terminal != "" {
			creating(c)
		}
		return r
	}
	r := l.create(w, s, func(c Creating) {
		l.receipts.creating(x, c)
		creating(c)
	})
	l.receipts.finish(x, r)
	ok, code, origin := r.Err == nil, "", "desktop"
	if !ok {
		code = r.Err.Code
	}
	if !w.Owner {
		origin = "phone"
	}
	l.ev.Emit(events.Event{Kind: "create", Origin: origin, Provider: s.Provider, OK: &ok, Code: code})
	return r
}

func (l *Launcher) create(w Who, s proto.LaunchSpec, creating func(Creating)) Result {
	if fl := Check(w, s, l.set.PhoneMaxAccess()); fl != nil {
		return Result{Err: fl}
	}
	f := l.reg.Load()
	if !slices.Contains(f.Projects, s.Project) {
		return Result{Err: fail("unknown_project", "This project isn't on your Mac anymore")}
	}
	env := l.d.LoginEnv()
	exe, ok := l.find(s.Provider, env)
	if !ok {
		return Result{Err: fail("provider_unavailable", s.Provider+" isn't installed on your Mac")}
	}
	name := ""
	if s.Checkout.New != nil {
		name = s.Checkout.New.Name
	}
	argv, fl := Argv(s, name)
	if fl != nil {
		return Result{Err: fl}
	}
	argv[0] = exe
	cwd, setup, fl := l.checkout(w, f, s, env)
	if fl != nil {
		return Result{Err: fl}
	}
	id := terminal.NewID()
	exits := make(chan exit, 1)
	l.mu.Lock()
	l.pending[id] = exits
	l.mu.Unlock()
	defer func() {
		l.mu.Lock()
		delete(l.pending, id)
		l.mu.Unlock()
	}()
	shell := lookup(env, "SHELL")
	if shell == "" {
		shell = "/bin/zsh"
	}
	origin := "desktop"
	if !w.Owner {
		origin = "phone"
	}
	t, err := l.d.Terminals.Spawn(terminal.Spec{ID: id, Cmd: shell, Args: Wrap(shell, l.d.Exe, setup, argv), Cwd: cwd,
		Env: l.d.Env(env, id), Cols: 100, Rows: 30, Origin: origin})
	if err != nil {
		return Result{Err: &Failure{Code: "spawn_failed", Message: "Couldn't start a terminal", Detail: err.Error()}}
	}
	creating(Creating{Terminal: id, Cwd: cwd, Setup: setup != ""})
	return l.wait(t, id, exits)
}

// checkout is the Session's folder and the setup to run there. A new
// Worktree copies and runs setup unless the owner turned them off.
func (l *Launcher) checkout(w Who, f registry.File, s proto.LaunchSpec, env []string) (string, string, *Failure) {
	if s.Checkout.Worktree != "" {
		trees, err := worktree.List(s.Project)
		if err != nil {
			return "", "", gitFailure(err)
		}
		if !slices.ContainsFunc(trees, func(t worktree.Worktree) bool { return t.Path == s.Checkout.Worktree }) {
			return "", "", fail("unknown_worktree", "This worktree isn't in the project anymore")
		}
		return s.Checkout.Worktree, "", trust(w, s, s.Checkout.Worktree, env)
	}
	if fl := trust(w, s, s.Project, env); fl != nil {
		return "", "", fl
	}
	n := s.Checkout.New
	create := worktree.Add
	if on(n.Copy) {
		create = worktree.Create
	}
	c, err := create(f, s.Project, n.Name, n.Base)
	if err != nil {
		return "", "", gitFailure(err)
	}
	setup := ""
	if on(n.Setup) {
		setup = f.Repos[s.Project].Setup
	}
	return c.Path, setup, nil
}

func on(b *bool) bool { return b == nil || *b }

// trust refuses a phone's claude where Claude would wait on its trust dialog:
// nobody at the Mac would see it.
func trust(w Who, s proto.LaunchSpec, dir string, env []string) *Failure {
	if w.Owner || s.Provider != "claude" || trustedAt(env, dir) {
		return nil
	}
	return fail("folder_not_trusted", "Trust this folder in Claude on your Mac first")
}

func gitFailure(err error) *Failure {
	if code := worktree.Code(err); code != "" {
		return fail(code, err.Error())
	}
	return &Failure{Code: "spawn_failed", Message: "git couldn't make the worktree", Detail: Tail(err.Error())}
}

func (l *Launcher) wait(t *terminal.Terminal, id string, exits chan exit) Result {
	tick := time.NewTicker(poll)
	defer tick.Stop()
	deadline := time.After(createWait)
	var present time.Time
	for {
		select {
		case e := <-exits:
			return exited(id, e)
		case <-deadline:
			msg := fmt.Sprintf("The session didn't start in %v", createWait)
			return Result{TerminalID: id, Err: &Failure{Code: "spawn_failed", Message: msg, Detail: Tail(t.Screen())}}
		case <-t.Done():
			select {
			case e := <-exits:
				return exited(id, e)
			default:
			}
			msg := fmt.Sprintf("terminal exited %d", t.ExitCode())
			return Result{TerminalID: id, Err: &Failure{Code: "spawn_failed", Message: msg, Detail: msg}}
		case <-tick.C:
			a, ok := l.agentIn(id)
			switch {
			case !ok:
			case a.Attached:
				return Result{AgentID: a.ID, TerminalID: id}
			case present.IsZero():
				present = time.Now()
			case time.Since(present) >= attachWait:
				return Result{AgentID: a.ID, TerminalID: id}
			}
		}
	}
}

func exited(id string, e exit) Result {
	what := "The agent"
	if e.phase == "setup" {
		what = "Setup"
	}
	return Result{TerminalID: id, Err: &Failure{Code: "spawn_failed", Message: fmt.Sprintf("%s exited %d", what, e.status), Detail: e.tail}}
}

func (l *Launcher) agentIn(terminalID string) (proto.AgentSummary, bool) {
	for _, a := range l.d.Agents.List() {
		if a.TerminalID == terminalID {
			return a, true
		}
	}
	return proto.AgentSummary{}, false
}

// Exited records a wrapper's `pocketd hook exit`. It reads the screen now,
// while the Terminal still shows the failure. Exits of finished creates are
// ignored.
func (l *Launcher) Exited(terminalID, phase string, status int) {
	if phase == "agent" {
		l.d.RestoreExited(terminalID)
	}
	l.mu.Lock()
	ch, ok := l.pending[terminalID]
	l.mu.Unlock()
	if !ok {
		return
	}
	tail := ""
	if t := l.d.Terminals.Get(terminalID); t != nil {
		tail = Tail(t.Screen())
	}
	select {
	case ch <- exit{phase, status, tail}:
	default:
	}
}

// Providers lists the agents on the login PATH. maxAccess is w's ceiling.
func (l *Launcher) Providers(w Who) (list []proto.ProviderInfo, maxAccess, phoneMax string) {
	env := l.d.LoginEnv()
	_, claude := l.find("claude", env)
	_, codex := l.find("codex", env)
	phoneMax = l.set.PhoneMaxAccess()
	maxAccess = phoneMax
	if w.Owner {
		maxAccess = "full"
	}
	return []proto.ProviderInfo{
		{ID: "claude", Available: claude, Efforts: ClaudeEfforts, Plan: true},
		{ID: "codex", Available: codex, Efforts: []string{}, Plan: false},
	}, maxAccess, phoneMax
}

func (l *Launcher) SetPhoneMaxAccess(v string) error { return l.set.SetPhoneMaxAccess(v) }

func (l *Launcher) ResumeCmd(saved state.Terminal) (string, []string, string) {
	return resumeCmd(saved, shellenv.LoginShell(), l.d.Exe, l.d.LoginEnv())
}

func (l *Launcher) find(provider string, env []string) (string, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if f, ok := l.found[provider]; ok && time.Since(f.at) < probeTTL {
		return f.path, f.path != ""
	}
	path, err := terminal.LookPath(provider, env)
	if err != nil {
		path = ""
	}
	l.found[provider] = found{path, time.Now()}
	return path, path != ""
}
