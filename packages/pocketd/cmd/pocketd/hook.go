package main

import (
	"io"
	"os"

	"pocketd/internal/ops"
)

// hook is Claude Code's PermissionRequest hook. Printing nothing leaves the
// decision to Claude's own dialog, so every failure falls back to that.
func hook(sock string) error {
	payload, err := io.ReadAll(os.Stdin)
	if err != nil {
		return nil
	}
	c, err := ops.Dial(sock)
	if err != nil {
		return nil
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "hook", Data: payload}) != nil {
		return nil
	}
	m, err := c.Recv()
	if err == nil && m.Ev == "hook" {
		os.Stdout.Write(m.Data)
	}
	return nil
}
