package daemon

import (
	"log"
	"maps"
	"slices"

	"pocketd/internal/handoff"
	"pocketd/internal/holder"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

// Carry holds every transcript tail still and describes each Terminal, with
// its Agent, for the pocketd that comes next: after an upgrade's exec, or after
// this one stops while holders keep the Terminals running. Release lets the
// tails go on when this pocketd does.
func (d *Daemon) Carry() []handoff.Terminal {
	d.holdTails()
	saved := map[string]state.Terminal{}
	for _, e := range d.Snapshot().Terminals {
		saved[e.TerminalID] = e
	}
	out := []handoff.Terminal{}
	for _, t := range d.Terminals.All() {
		info := t.Info()
		e, ok := saved[info.ID]
		if !ok {
			e = state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows, Local: info.Local}
		}
		out = append(out, handoff.Terminal{Saved: e, Cmd: info.Cmd, Args: info.Args, Cwd: info.Cwd, Agent: d.carried(e.AgentID, d.presentIn(info.ID))})
	}
	return out
}

func (d *Daemon) Release() {
	for _, pr := range d.held {
		pr.tailing.Unlock()
	}
	d.held = nil
}

// holdTails stops each transcript tail between two lines, so the next pocketd reads on from where this one
// stopped.
func (d *Daemon) holdTails() {
	for _, pr := range d.presences() {
		pr.tailing.Lock()
		d.held = append(d.held, pr)
	}
}

func (d *Daemon) presences() []*presence {
	d.mu.Lock()
	defer d.mu.Unlock()
	return slices.Collect(maps.Values(d.present))
}

// carried is what agentID's Agent, present as pr, keeps across the exec beyond its saved entry.
func (d *Daemon) carried(agentID string, pr *presence) *handoff.Agent {
	if agentID == "" {
		return nil
	}
	a, err := d.Agents.Get(agentID)
	if err != nil {
		return nil
	}
	s := a.Summary()
	phase := "idle"
	if turn := a.Turn(); turn == "working" || turn == "needsYou" {
		phase = turn
	}
	c := &handoff.Agent{Named: a.Named(), Phase: phase, Epoch: s.Epoch, Seq: s.MaxSeq, Background: a.Timeline.Background()}
	if pr != nil {
		c.Read = pr.tailed.Load()
	}
	return c
}

// Adopt takes over the pty fds a pocketd from before holders handed down
// across exec. Unlike Restore, nothing is spawned, so the login env is read in
// the background.
func (d *Daemon) Adopt(f handoff.File) {
	if d.Capture != nil {
		go d.Recapture()
	}
	for _, e := range f.Terminals {
		if e.FD == 0 {
			continue
		}
		t, err := d.Terminals.Adopt(terminal.Adopted{ID: e.Saved.TerminalID, Cmd: e.Cmd, Args: e.Args, Cwd: e.Cwd,
			Cols: e.Saved.Cols, Rows: e.Saved.Rows, FD: e.FD, Pid: e.Pid, Screen: e.Screen, Local: e.Saved.Local})
		if err != nil {
			log.Printf("adopt %s: %v", e.Saved.TerminalID, err)
			continue
		}
		if e.Agent != nil && e.Saved.AgentID != "" {
			d.expect(e.Saved, t, "", e.Agent)
		}
	}
}

// Reattach takes over the Terminals whose holders outlived the pocketd before
// this one, each under its id with its Agent. carried is what that pocketd
// handed down when it stopped cleanly; without it, an Agent goes on from its
// saved entry and its store. A Terminal that ended while no pocketd was there
// is let go. It returns the saved entries no holder kept, for Restore.
func (d *Daemon) Reattach(clients []*holder.Client, carried []handoff.Terminal, saved []state.Terminal) []state.Terminal {
	if len(clients) > 0 && d.Capture != nil {
		go d.Recapture()
	}
	handed := map[string]handoff.Terminal{}
	for _, e := range carried {
		handed[e.Saved.TerminalID] = e
	}
	entries := map[string]state.Terminal{}
	for _, e := range saved {
		entries[e.TerminalID] = e
	}
	held := map[string]bool{}
	for _, c := range clients {
		held[c.ID] = true
		if c.Exited {
			c.Close()
			continue
		}
		t, err := d.Terminals.Reattach(c)
		if err != nil {
			log.Printf("reattach %s: %v", c.ID, err)
			c.Close()
			continue
		}
		e, ok := entries[c.ID]
		var agent *handoff.Agent
		if h, found := handed[c.ID]; found {
			e, ok, agent = h.Saved, true, h.Agent
		}
		if !ok || e.AgentID == "" {
			continue
		}
		if agent == nil {
			agent = &handoff.Agent{Phase: phaseOf(e.Status)}
		}
		d.expect(e, t, "", agent)
	}
	var rest []state.Terminal
	for _, e := range saved {
		if !held[e.TerminalID] {
			rest = append(rest, e)
		}
	}
	return rest
}

// phaseOf is the turn phase a saved status stands for.
func phaseOf(status string) string {
	if status == "working" || status == "needsYou" {
		return status
	}
	return "idle"
}

// handedOver settles a hint the pocketd before exec handed down. Its agent
// never stopped, so it stays attached and gets no restore notice.
func (d *Daemon) handedOver(pr *presence, h *hint) {
	pr.mu.Lock()
	pr.hooked = true
	pr.mu.Unlock()
	d.finish(h, "")
}
