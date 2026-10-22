package daemon

import (
	"context"
	"errors"
	"log"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

var (
	errNoSwitch    = errors.New("This session can't switch accounts")
	errSwitchToOut = errors.New("Sign in to this account before switching to it")
)

// LimitRefresh bounds the usage read that picks the account an agent at its limit moves to.
var LimitRefresh = 10 * time.Second

// Switch moves agent id to account, accounts.BuiltIn being the built-in
// login: now when it is between turns, else once its turn ends. An agent the
// user started in a shell stays on its account.
func (d *Daemon) Switch(agentID, account string) error {
	pr := d.presenceOf(agentID)
	if pr == nil || pr.typed || d.Resume == nil || d.Accounts == nil {
		return errNoSwitch
	}
	id, err := d.Accounts.Resolve(pr.provider, account)
	if err != nil {
		return err
	}
	if !d.Accounts.SignedIn(pr.provider, id) {
		return errSwitchToOut
	}
	pr.mu.Lock()
	pr.resend = ""
	pr.mu.Unlock()
	if want(id) == pr.a.Summary().Account {
		id = ""
	}
	pr.a.SetSwitching(id)
	go d.switchPending(pr)
	return nil
}

// limitHit moves pr, whose turn its account's limit ended, to the signed-in
// account with the most left, and sends its prompt again there. With
// switching at a limit off, or every account at one, the agent's own error stands.
func (d *Daemon) limitHit(pr *presence) {
	if pr.typed || d.Resume == nil || d.Accounts == nil || !d.Accounts.AutoSwitch() {
		return
	}
	ctx, cancel := context.WithTimeout(pr.ctx, LimitRefresh)
	d.Accounts.Refresh(ctx, "")
	cancel()
	s := pr.a.Summary()
	if s.Switching == "" {
		best, ok := d.Accounts.Best(pr.provider, cmpBuiltIn(s.Account))
		if !ok {
			return
		}
		pr.a.SetSwitching(best)
	}
	pr.mu.Lock()
	pr.resend = pr.a.Timeline.LastPrompt()
	pr.mu.Unlock()
	d.switchPending(pr)
}

// switchPending moves pr to the account it is switching to, once it is
// between turns. An agent still settling switches once it has.
func (d *Daemon) switchPending(pr *presence) {
	id := pr.a.Summary().Switching
	if id == "" || !d.settled(pr) || !movable(pr.a.Summary(), want(id)) {
		return
	}
	pr.mu.Lock()
	resend := pr.resend
	pr.mu.Unlock()
	// One that didn't move while it still could won't: the switch is dropped.
	if d.move(pr, want(id), resend) || movable(pr.a.Summary(), want(id)) {
		pr.a.SetSwitching("")
	}
}

// want is how an agent's summary names account id.
func want(id string) string {
	if id == accounts.BuiltIn {
		return ""
	}
	return id
}

func cmpBuiltIn(account string) string {
	if account == "" {
		return accounts.BuiltIn
	}
	return account
}

// settled is whether pr is its Terminal's agent, and not resuming there.
func (d *Daemon) settled(pr *presence) bool {
	id := pr.t.Info().ID
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.present[id] == pr && d.restoring[id] == nil
}

func (d *Daemon) presenceOf(agentID string) *presence {
	d.mu.Lock()
	defer d.mu.Unlock()
	for _, pr := range d.present {
		if pr.a.ID() == agentID {
			return pr
		}
	}
	return nil
}

// movable reports whether an agent can restart on account want: it is on
// another one, reports between turns (an unattached claude's status is a
// guess), and has a conversation to resume. A turn that failed, as one at a
// limit does, has ended too.
func movable(s proto.AgentSummary, want string) bool {
	idle := s.Status == "idle" || s.Status == "done"
	return s.Account != want && idle && s.Attached && s.ProviderSessionID != ""
}

// move replaces pr's process with its provider resumed on account want, with
// the access it has, and sends resend once it settles. The Agent keeps its id
// and Terminal, and refuses prompts as a restored one does until its new
// process settles. It reports whether the new process started.
func (d *Daemon) move(pr *presence, want, resend string) bool {
	info := pr.t.Info()
	e := state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows, Local: info.Local}
	pr.save(&e)
	if e.AgentID == "" {
		return false
	}
	if !kindOf[pr.provider].resumable(e) {
		return false
	}
	if e.Account != "" && d.Accounts != nil {
		// Its history must be reachable from the account it moves to.
		if err := d.Accounts.Share(pr.provider, e.Account); err != nil {
			log.Printf("switch %s: sharing %s: %v", e.AgentID, e.Account, err)
		}
	}
	e.Account = want
	e.KeepAccess = true
	cmd, args, reason := d.Resume(e)
	if reason != "" {
		log.Printf("switch %s: %s", e.AgentID, reason)
		return false
	}
	// Resume did file IO: a prompt may have started a turn meanwhile.
	if !movable(pr.a.Summary(), want) {
		return false
	}
	h := &hint{a: pr.a, saved: e, started: time.Now(), moved: true, resend: resend}
	d.mu.Lock()
	if d.present[info.ID] != pr || d.restoring[info.ID] != nil {
		d.mu.Unlock()
		return false
	}
	if d.restoring == nil {
		d.restoring = map[string]*hint{}
	}
	d.restoring[info.ID] = h
	h.timer = time.AfterFunc(RestoreWait, func() { d.finish(h, "resume_timeout") })
	d.mu.Unlock()
	driver := pr.a.Driver()
	pr.a.SetDriver(d.waiting(h))
	spec := terminal.Spec{ID: info.ID, Cwd: info.Cwd, Local: info.Local}
	if err := pr.t.Replace(cmd, args, d.terminalEnv(spec), pr.a.Summary().Origin); err != nil {
		log.Printf("switch %s: %v", e.AgentID, err)
		d.mu.Lock()
		if d.restoring[info.ID] == h {
			delete(d.restoring, info.ID)
		}
		d.mu.Unlock()
		h.timer.Stop()
		pr.a.SetDriver(driver)
		return false
	}
	go func() {
		<-pr.t.Done()
		d.drop(h)
	}()
	return true
}

// moved sends a moved agent's resend, once it has settled on its new account.
func (d *Daemon) moved(h *hint) {
	if h.resend == "" {
		return
	}
	if err := h.a.Driver().Prompt(h.resend); err != nil {
		log.Printf("switch %s: sending the prompt again: %v", h.a.ID(), err)
	}
}
