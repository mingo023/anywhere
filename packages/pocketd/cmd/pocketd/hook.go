package main

import (
	"io"
	"os"

	"pocketd/internal/daemon"
	"pocketd/internal/ops"
	"pocketd/internal/proc"
)

// hook runs for every Claude Code hook in pocketd's plugin. Printing nothing
// lets Claude go on as if there were no hook, so every failure falls back to that.
func hook(sock string) error {
	payload, err := io.ReadAll(os.Stdin)
	pty := os.Getenv("POCKETD_PTY")
	if err != nil || pty == "" {
		return nil
	}
	pid := nearestClaude(ancestors(os.Getpid()))
	if pid == 0 {
		return nil
	}
	c, err := ops.Dial(sock)
	if err != nil {
		return nil
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "hook", ID: pty, Pid: pid, Data: payload}) != nil {
		return nil
	}
	m, err := c.Recv()
	if err == nil && m.Ev == "hook" {
		os.Stdout.Write(m.Data)
	}
	return nil
}

// ancestors lists pid's parents, nearest first.
func ancestors(pid int) []proc.Proc {
	var chain []proc.Proc
	for {
		ppid, err := proc.Parent(pid)
		if err != nil || ppid <= 1 {
			return chain
		}
		p, err := proc.Read(ppid)
		if err != nil {
			return chain
		}
		chain = append(chain, p)
		pid = ppid
	}
}

// nearestClaude is the claude whose hook this is. A claude -p run by
// Claude's Bash tool is nearer than the claude that ran it.
func nearestClaude(chain []proc.Proc) int {
	for _, p := range chain {
		if daemon.Provider(p.Argv) == "claude" {
			return p.Pid
		}
	}
	return 0
}
