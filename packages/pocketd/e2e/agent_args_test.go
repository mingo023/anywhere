package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// argvClaude is a claude command that prints the words it was given, then
// runs the fake claude with them.
func argvClaude(h *Harness) string {
	path := filepath.Join(h.Home, "argv", "claude")
	os.MkdirAll(filepath.Dir(path), 0o755)
	body := fmt.Sprintf("#!/bin/sh\necho \"ARGV: $*\"\nexec '%s' \"$@\"\n", filepath.Join(binDir, "fake", "claude"))
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		h.t.Fatal(err)
	}
	return path
}

func TestLaunchesUseTheAgentArgsFromConfig(t *testing.T) {
	h := Start(t, launchReady(""))
	for key, value := range map[string]string{"claude.command": argvClaude(h), "claude.promptArgs": "--verbose --"} {
		if out, code := h.Pocketd("config", "set", key, value); code != 0 {
			t.Fatalf("set %s: exit %d: %q", key, code, out)
		}
	}
	if out, code := h.Pocketd("config", "set", "claude.forkArgs", "--fork-session"); code != 1 || !strings.Contains(out, "{sessionId}") {
		t.Fatalf("fork args without the id: exit %d: %q", code, out)
	}
	o := h.Owner()

	create(o, "r1", inRepo(h, map[string]any{"prompt": "hello"}))
	c := reply(o, "agent.creating", "error")
	h.WaitScreen(c.TerminalID, "ARGV: --permission-mode default -n hello --verbose -- hello")
	h.WaitScreen(c.TerminalID, "echo: hello")

	create(o, "r2", inRepo(h, map[string]any{"fork": "0a1b-2c3d"}))
	c = reply(o, "agent.creating", "error")
	h.WaitScreen(c.TerminalID, "ARGV: --resume 0a1b-2c3d --fork-session --permission-mode default")

	if out, code := h.Pocketd("config", "set", "claude.forkArgs", ""); code != 0 {
		t.Fatalf("clear fork args: exit %d: %q", code, out)
	}
	create(o, "r3", inRepo(h, map[string]any{"fork": "0a1b-2c3d"}))
	if e := reply(o, "agent.creating", "error"); e.Type != "error" || e.Code != "fork_unsupported" {
		t.Fatalf("fork without fork args: %s", e.Raw)
	}
}
