package main

import (
	"io"
	"os"
	"time"

	"pocketd/internal/ops"
)

// hookRetry is how long a hook keeps redialing. An upgrading pocketd drops
// the socket for a moment, and its successor asks the user again. It must stay
// under the 5 s timeout of the plugin's status hooks, or a pocketd that is
// down shows up as a timeout error in Claude instead of a silent no-op.
var hookRetry = 3 * time.Second

// hook runs for every Claude Code hook in pocketd's plugin. Printing nothing
// lets Claude go on as if there were no hook, so every failure falls back to
// that. pocketd finds the claude it came from by this process's ancestry.
func hook(sock string) error {
	payload, err := io.ReadAll(os.Stdin)
	pty := os.Getenv("POCKETD_PTY")
	if err != nil || pty == "" {
		return nil
	}
	os.Stdout.Write(relay(sock, pty, payload, hookRetry))
	return nil
}

// relay sends the hook until pocketd answers or wait has passed.
func relay(sock, pty string, payload []byte, wait time.Duration) []byte {
	for deadline := time.Now().Add(wait); ; time.Sleep(100 * time.Millisecond) {
		if out, ok := ask(sock, pty, payload); ok || time.Now().After(deadline) {
			return out
		}
	}
}

// ask sends the hook once. ok means pocketd answered, even with an error:
// only a socket that drops is worth resending to.
func ask(sock, pty string, payload []byte) (out []byte, ok bool) {
	c, err := ops.Dial(sock)
	if err != nil {
		return nil, false
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "hook", ID: pty, Data: payload}) != nil {
		return nil, false
	}
	m, err := c.Recv()
	if err != nil {
		return nil, false
	}
	if m.Ev == "hook" {
		return m.Data, true
	}
	return nil, true
}
