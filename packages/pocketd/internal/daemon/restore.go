package daemon

import (
	"cmp"
	"errors"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

var (
	// RestoreWait is how long a resumed agent has to show up in its Terminal.
	RestoreWait = 30 * time.Second
	// RestoreSettle is how long an adopted agent has to reach its Conversation
	// before it counts as resumed anyway: an untrusted claude sends no hooks.
	RestoreSettle = 10 * time.Second
	// PlaceholderTTL is how long a failed Agent without a Terminal stays listed.
	PlaceholderTTL = 10 * time.Minute
)

// hint is a saved Agent waiting for its provider to come back in its Terminal.
// Its fields change under Daemon.mu.
type hint struct {
	a       *agent.Agent
	saved   state.Terminal
	started time.Time
	pr      *presence
	live    agent.Driver // pr's driver, which takes over once the restore settles
	over    bool
	timer   *time.Timer
}

// pending drives an Agent whose provider hasn't come back.
type pending struct {
	d *Daemon
	h *hint
}

var errRestoreFailed = errors.New("This session couldn't resume")

// refuse reads over as failed: a settled hint has handed the Agent to its provider's driver.
func (p pending) refuse() error {
	p.d.mu.Lock()
	defer p.d.mu.Unlock()
	if p.h.over {
		return errRestoreFailed
	}
	return agent.ErrResuming
}

func (p pending) Prompt(string) error { return p.refuse() }
func (p pending) Interrupt() error    { return p.refuse() }
func (p pending) Compact() error      { return p.refuse() }
func (p pending) Close()              { p.d.drop(p.h) }

// Snapshot is what a restart needs to bring every Terminal back. An Agent
// still resuming keeps its saved entry, so a second restart doesn't lose it.
func (d *Daemon) Snapshot() state.File {
	f := state.File{Version: state.Version, Terminals: []state.Terminal{}}
	for _, info := range d.Terminals.List() {
		e := state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows}
		d.mu.Lock()
		h := d.restoring[info.ID]
		if h != nil && !h.over {
			e = h.saved
			e.Cols, e.Rows = info.Cols, info.Rows
		}
		d.mu.Unlock()
		if pr := d.presentIn(info.ID); pr != nil && e.AgentID == "" {
			pr.save(&e)
		}
		f.Terminals = append(f.Terminals, e)
	}
	slices.SortFunc(f.Terminals, func(a, b state.Terminal) int { return strings.Compare(a.TerminalID, b.TerminalID) })
	return f
}

// Restore brings back the Terminals of the pocketd before this one, under
// their old ids, in their launch dirs. A Terminal that held an Agent runs its
// resume argv and keeps the Agent's id; any other is a login shell. It runs
// before the poller and ops Serve start, so the poller finds every hint and
// the first list holds them all. It reads the login shell's environment first,
// so no Terminal gets pocketd's own: under launchd, that has no TERM, and a
// PATH without the agents.
func (d *Daemon) Restore(f state.File, shell string) {
	if d.Capture != nil {
		d.Recapture()
	}
	for _, e := range f.Terminals {
		t, reason := d.reopen(e, shell)
		switch {
		case e.AgentID == "":
		case d.Resume == nil:
			d.report(e.AgentID, false, 0, "", "resume_off")
		default:
			d.expect(e, t, reason)
		}
	}
}

func (d *Daemon) reopen(e state.Terminal, shell string) (*terminal.Terminal, string) {
	if st, err := os.Stat(e.LaunchDir); err != nil || !st.IsDir() {
		log.Printf("restore %s: launch_dir_missing", e.TerminalID)
		return nil, "launch_dir_missing"
	}
	cmd, args, reason := shell, []string{"-l"}, ""
	if e.AgentID != "" && d.Resume != nil {
		if c, a, r := d.Resume(e); r == "" {
			cmd, args = c, a
		} else {
			reason = r
		}
	}
	t, err := d.spawn(terminal.Spec{ID: e.TerminalID, Cmd: cmd, Args: args, Cwd: e.LaunchDir, Cols: e.Cols, Rows: e.Rows})
	if err != nil {
		log.Printf("restore %s: %v", e.TerminalID, err)
		return nil, cmp.Or(reason, "resume_not_accepted")
	}
	return t, reason
}

// expect lists e's Agent under its old id until its provider shows up in t.
// Without t, or with a reason, it is a failed placeholder at once.
func (d *Daemon) expect(e state.Terminal, t *terminal.Terminal, reason string) {
	h := &hint{saved: e, started: time.Now()}
	x := agent.Restored{ID: e.AgentID, Cwd: e.LaunchDir, Provider: e.Provider, Conversation: e.ConversationID,
		Origin: e.Origin, Fallback: filepath.Base(e.LaunchDir), CreatedAt: e.CreatedAt, Done: e.Status == "done", Failed: e.Failed}
	if t != nil {
		x.Terminal = t.Info().ID
	}
	h.a = d.Agents.Restore(x, d.waiting(h))
	d.mu.Lock()
	if d.restoring == nil {
		d.restoring = map[string]*hint{}
	}
	d.restoring[e.TerminalID] = h
	if reason == "" {
		h.timer = time.AfterFunc(RestoreWait, func() { d.finish(h, "resume_timeout") })
	}
	d.mu.Unlock()
	if reason != "" {
		d.finish(h, reason)
	}
	if t == nil {
		time.AfterFunc(PlaceholderTTL, func() { d.drop(h) })
		return
	}
	go func() {
		<-t.Done()
		d.drop(h)
	}()
}

func (d *Daemon) waiting(h *hint) agent.Driver {
	return pending{d, h}
}

// adopt hands pr the hint waiting in its Terminal, if pr is the same provider
// and the hint is still resuming. The Agent keeps refusing prompts until the
// restore settles, as the provider may still be loading. Any other hint's
// placeholder closes.
func (d *Daemon) adopt(pr *presence, driver func(*agent.Agent) agent.Driver) *hint {
	d.mu.Lock()
	h := d.restoring[pr.t.Info().ID]
	fits := h != nil && !h.over && h.pr == nil && h.saved.Provider == pr.provider
	if fits {
		h.pr, h.live = pr, driver(h.a)
		h.timer.Stop()
		h.timer = time.AfterFunc(RestoreSettle, func() { d.finish(h, "") })
	}
	d.mu.Unlock()
	if h != nil && !fits {
		d.drop(h)
		return nil
	}
	return h
}

// hintOf is the hint pr adopted, while it is still listed.
func (d *Daemon) hintOf(pr *presence) *hint {
	d.mu.Lock()
	defer d.mu.Unlock()
	if h := d.restoring[pr.t.Info().ID]; h != nil && h.pr == pr {
		return h
	}
	return nil
}

// resumed settles pr's hint once pr is on the Conversation it was saved with.
func (d *Daemon) resumed(pr *presence, conversation string) {
	if h := d.hintOf(pr); h != nil && conversation == h.saved.ConversationID {
		d.finish(h, "")
	}
}

// RestoreExited fails the restore in terminal id when its resumed agent exits
// before it settles, which the login-shell wrapper reports before the poller
// can notice.
func (d *Daemon) RestoreExited(id string) {
	d.mu.Lock()
	h := d.restoring[id]
	d.mu.Unlock()
	if h != nil {
		d.finish(h, "agent_exited")
	}
}

// finish settles h, or fails it for reason. Only the first call counts, and
// a timeout loses to an adoption.
func (d *Daemon) finish(h *hint, reason string) {
	d.mu.Lock()
	if h.over || reason == "resume_timeout" && h.pr != nil {
		d.mu.Unlock()
		return
	}
	h.over = true
	if h.timer != nil {
		h.timer.Stop()
	}
	if reason == "" {
		delete(d.restoring, h.saved.TerminalID)
		h.a.SetDriver(h.live)
	}
	d.mu.Unlock()
	if reason != "" {
		h.a.SetAttached(false)
	}
	status := h.a.Summary().Status
	outcome := state.Outcome(h.saved, status, reason != "")
	// A turn already running has had the Working that clears the notice.
	if reason != "" || status != "working" {
		h.a.SetRestore(outcome)
	}
	d.report(h.a.ID(), reason == "", time.Since(h.started).Milliseconds(), outcome, reason)
}

// drop closes h's placeholder, unless an agent settled it.
func (d *Daemon) drop(h *hint) {
	d.mu.Lock()
	listed := d.restoring[h.saved.TerminalID] == h
	if listed {
		delete(d.restoring, h.saved.TerminalID)
	}
	d.mu.Unlock()
	if listed {
		d.Agents.Remove(h.a.ID())
	}
}

func (d *Daemon) report(agentID string, ok bool, ms int64, outcome, reason string) {
	if reason != "" {
		log.Printf("restore %s: %s", agentID, reason)
	}
	if d.OnRestore != nil {
		d.OnRestore(agentID, ok, ms, outcome, reason)
	}
}
