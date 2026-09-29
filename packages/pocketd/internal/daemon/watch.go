package daemon

import (
	"context"
	"slices"
	"strings"
	"time"

	"pocketd/internal/proc"
	"pocketd/internal/terminal"
)

var WatchEvery = 250 * time.Millisecond

func (d *Daemon) Watch(ctx context.Context) {
	tick := time.NewTicker(WatchEvery)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			d.poll()
		}
	}
}

func (d *Daemon) poll() {
	for _, t := range d.Terminals.All() {
		d.observe(t)
	}
	for _, pr := range d.exited() {
		d.endAgent(pr)
	}
}

// exited finds agents whose terminal closed before a poll saw them leave.
func (d *Daemon) exited() []*presence {
	d.mu.Lock()
	defer d.mu.Unlock()
	var gone []*presence
	for _, pr := range d.present {
		select {
		case <-pr.t.Done():
			gone = append(gone, pr)
		default:
		}
	}
	return gone
}

func (d *Daemon) observe(t *terminal.Terminal) {
	d.watch.Lock()
	defer d.watch.Unlock()
	text, procs := foreground(t)
	t.SetForeground(text)
	provider, p, ok := agentProc(procs)
	d.mu.Lock()
	pr := d.present[t.Info().ID]
	d.mu.Unlock()
	if pr != nil && (!ok || p.Pid != pr.pid) {
		d.endAgent(pr)
		pr = nil
	}
	if ok && pr == nil {
		d.attach(d.startAgent(t, provider, p))
	}
}

// foreground lists the group even when it is the terminal's own process:
// that is where a Pocket-spawned or exec'd agent runs.
func foreground(t *terminal.Terminal) (text string, procs []proc.Proc) {
	pgid, err := t.Pgrp()
	if err != nil || pgid <= 0 {
		return "", nil
	}
	pids, _ := proc.Members(pgid)
	for _, pid := range pids {
		if p, err := proc.Read(pid); err == nil {
			procs = append(procs, p)
		}
	}
	if pgid == t.Pid() || len(procs) == 0 {
		return "", procs
	}
	leader := procs[0]
	if i := slices.IndexFunc(procs, func(p proc.Proc) bool { return p.Pid == pgid }); i >= 0 {
		leader = procs[i]
	}
	return strings.Join(leader.Argv, " "), procs
}
