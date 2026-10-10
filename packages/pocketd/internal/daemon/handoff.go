package daemon

import (
	"errors"
	"fmt"
	"log"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

// HandoffPause is how long Handoff waits for each Terminal's reader to stop.
var HandoffPause = 2 * time.Second

// Handoff pauses every Terminal, so unread output waits in the kernel, and
// describes each, with its Agent, for the pocketd that execs next. If the
// exec doesn't happen, CancelHandoff resumes them.
func (d *Daemon) Handoff() ([]handoff.Terminal, error) {
	all := d.Terminals.All()
	for _, t := range all {
		if err := t.Pause(HandoffPause); err != nil && !errors.Is(err, terminal.ErrClosed) {
			return nil, fmt.Errorf("pause %s: %w", t.Info().ID, err)
		}
	}
	saved := map[string]state.Terminal{}
	for _, e := range d.Snapshot().Terminals {
		saved[e.TerminalID] = e
	}
	out := []handoff.Terminal{}
	for _, t := range all {
		info := t.Info()
		h, err := t.Handoff()
		if errors.Is(err, terminal.ErrClosed) {
			continue
		}
		if err != nil {
			return nil, err
		}
		e, ok := saved[info.ID]
		if !ok {
			e = state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows, Local: info.Local}
		}
		out = append(out, handoff.Terminal{Saved: e, Cmd: info.Cmd, Args: info.Args, Cwd: info.Cwd,
			FD: h.FD, Pid: h.Pid, Screen: h.Screen, Agent: d.carried(e.AgentID)})
	}
	return out, nil
}

func (d *Daemon) CancelHandoff() {
	for _, t := range d.Terminals.All() {
		t.Resume()
	}
}

// carried is what agentID's Agent keeps across the exec beyond its saved entry.
func (d *Daemon) carried(agentID string) *handoff.Agent {
	if agentID == "" {
		return nil
	}
	a, err := d.Agents.Get(agentID)
	if err != nil {
		return nil
	}
	s := a.Summary()
	phase := "idle"
	if s.Status == "working" || s.Status == "needsYou" {
		phase = s.Status
	}
	return &handoff.Agent{Named: a.Named(), Phase: phase, Epoch: s.Epoch, Seq: s.MaxSeq}
}

// Adopt takes over what the pocketd before exec handed down. Unlike Restore,
// nothing is spawned, so the login env is read in the background.
func (d *Daemon) Adopt(f handoff.File) {
	if d.Capture != nil {
		go d.Recapture()
	}
	for _, e := range f.Terminals {
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

// handedOver settles a hint the pocketd before exec handed down. Its agent
// never stopped, so it stays attached and gets no restore notice.
func (d *Daemon) handedOver(pr *presence, h *hint) {
	pr.mu.Lock()
	pr.hooked = true
	pr.mu.Unlock()
	d.finish(h, "")
}
