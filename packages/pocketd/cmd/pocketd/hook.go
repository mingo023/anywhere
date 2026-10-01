package main

import (
	"io"
	"os"

	"pocketd/internal/ops"
)

// hook runs for every Claude Code hook in pocketd's plugin. Printing nothing
// lets Claude go on as if there were no hook, so every failure falls back to
// that. pocketd finds the claude it came from by this process's ancestry.
func hook(sock string) error {
	payload, err := io.ReadAll(os.Stdin)
	pty := os.Getenv("POCKETD_PTY")
	if err != nil || pty == "" {
		return nil
	}
	c, err := ops.Dial(sock)
	if err != nil {
		return nil
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "hook", ID: pty, Data: payload}) != nil {
		return nil
	}
	m, err := c.Recv()
	if err == nil && m.Ev == "hook" {
		os.Stdout.Write(m.Data)
	}
	return nil
}
