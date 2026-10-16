package daemon

import (
	"log"
	"os"
	"slices"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

// Follow restarts provider's agents that Pocket launched and that run on
// another account than the default, once they are idle, resuming their
// conversation on the default's. A busy agent moves when its turn ends.
func (d *Daemon) Follow(provider string) {
	if d.Resume == nil || d.Accounts == nil {
		return
	}
	id, err := d.Accounts.Resolve(provider, "")
	if err != nil {
		return
	}
	want := id
	if want == accounts.BuiltIn {
		want = ""
	}
	// A default signed out since it was picked must not drag agents to a login prompt.
	if len(d.movers(provider, want)) == 0 || !d.Accounts.SignedIn(provider, id) {
		return
	}
	d.followTo(provider, want)
}

// movable reports whether an agent should restart on account want: it is on
// another one, reports between turns (an unattached claude's status is a
// guess), and has a conversation to resume.
func movable(s proto.AgentSummary, want string) bool {
	idle := s.Status == "idle" || (s.Status == "done" && !s.Failed)
	return s.Account != want && idle && s.Attached && s.ProviderSessionID != ""
}

func (d *Daemon) movers(provider, want string) []*presence {
	d.mu.Lock()
	var all []*presence
	for _, pr := range d.present {
		if pr.provider == provider {
			all = append(all, pr)
		}
	}
	d.mu.Unlock()
	return slices.DeleteFunc(all, func(pr *presence) bool { return pr.typed || !movable(pr.a.Summary(), want) })
}

func (d *Daemon) followTo(provider, want string) {
	for _, pr := range d.movers(provider, want) {
		d.move(pr, want)
	}
}

// move replaces pr's process with its provider resumed on account want. The
// Agent keeps its id and Terminal, and refuses prompts as a restored one does
// until its new process settles.
func (d *Daemon) move(pr *presence, want string) {
	info := pr.t.Info()
	e := state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows, Local: info.Local}
	pr.save(&e)
	if e.AgentID == "" {
		return
	}
	// A conversation that has had no prompt has no transcript, and claude can't resume it.
	if pr.provider == "claude" {
		if _, err := os.Stat(e.TranscriptPath); err != nil {
			return
		}
	}
	if e.Account != "" && d.Accounts != nil {
		// Its history must be reachable from the account it moves to.
		if err := d.Accounts.Share(pr.provider, e.Account); err != nil {
			log.Printf("follow %s: sharing %s: %v", e.AgentID, e.Account, err)
		}
	}
	e.Account = want
	cmd, args, reason := d.Resume(e)
	if reason != "" {
		log.Printf("follow %s: %s", e.AgentID, reason)
		return
	}
	// Resume did file IO: a prompt may have started a turn meanwhile.
	if !movable(pr.a.Summary(), want) {
		return
	}
	h := &hint{a: pr.a, saved: e, started: time.Now(), moved: true}
	d.mu.Lock()
	if d.present[info.ID] != pr || d.restoring[info.ID] != nil {
		d.mu.Unlock()
		return
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
		log.Printf("follow %s: %v", e.AgentID, err)
		d.mu.Lock()
		if d.restoring[info.ID] == h {
			delete(d.restoring, info.ID)
		}
		d.mu.Unlock()
		h.timer.Stop()
		pr.a.SetDriver(driver)
		return
	}
	go func() {
		<-pr.t.Done()
		d.drop(h)
	}()
}
