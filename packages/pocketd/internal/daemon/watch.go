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
	d.looked()
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
	text, procs, read := foreground(t)
	if !read {
		// A holder too busy to answer says nothing about the agent in it.
		return
	}
	t.SetForeground(text)
	provider, p, ok := agentProc(procs)
	d.mu.Lock()
	pr := d.present[t.Info().ID]
	d.mu.Unlock()
	if pr != nil && (!ok || p.Pid != pr.pid) {
		d.endAgent(pr)
		pr = nil
	}
	switch {
	case ok && pr == nil:
		d.attach(d.startAgent(t, provider, p))
	case !ok:
		d.absent(t)
	}
}

// foreground lists the group even when it is the terminal's own process:
// that is where a Pocket-spawned or exec'd agent runs. read is false when the
// group couldn't be read.
func foreground(t *terminal.Terminal) (text string, procs []proc.Proc, read bool) {
	pgid, err := t.Pgrp()
	if err != nil {
		return "", nil, false
	}
	if pgid <= 0 {
		return "", nil, true
	}
	pids, err := proc.Members(pgid)
	if err != nil {
		return "", nil, false
	}
	for _, pid := range pids {
		if p, err := proc.Read(pid); err == nil {
			procs = append(procs, p)
		}
	}
	if pgid == t.Pid() || len(procs) == 0 {
		return "", procs, true
	}
	leader := procs[0]
	if i := slices.IndexFunc(procs, func(p proc.Proc) bool { return p.Pid == pgid }); i >= 0 {
		leader = procs[i]
	}
	return strings.Join(leader.Argv, " "), procs, true
}
