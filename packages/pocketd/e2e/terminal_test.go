//go:build unix

package e2e

import (
	"encoding/base64"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// A phone opens a login shell in a project, the desktop hears of it as it
// does of an agent's, and the phone types into it.
func TestAPhonesShellOpensOnTheDesktopAndTakesItsTyping(t *testing.T) {
	h := Start(t, launchReady(""))
	desktop := h.Owner()
	phone := h.Paired(proto.CapTerminal)
	phone.Send(map[string]any{"type": "terminal.create", "id": "c", "requestId": "r1", "cwd": h.Repo})
	created := phone.WaitFor("terminal.created", func(m Message) bool { return m.Type == "terminal.created" || m.Type == "error" })
	if created.Type != "terminal.created" || created.RequestID != "r1" || created.TerminalID == "" {
		t.Fatalf("%s", created.Raw)
	}
	tab := desktop.WaitFor("agent.creating", func(m Message) bool { return m.Type == "agent.creating" })
	if tab.RequestID != "r1" || tab.TerminalID != created.TerminalID || tab.Cwd != h.Repo || tab.ID != "" {
		t.Fatalf("%s", tab.Raw)
	}
	phone.Send(map[string]any{"type": "terminal.attach", "id": "a", "terminalId": created.TerminalID})
	phone.WaitFor("terminal.attached", func(m Message) bool { return m.Type == "terminal.attached" })
	phone.Send(map[string]any{"type": "terminal.input", "id": "i", "terminalId": created.TerminalID, "data": base64.StdEncoding.EncodeToString([]byte("echo e2e-$((6*7))\r"))})
	phone.WaitFor("the shell's answer", func(m Message) bool { return strings.Contains(m.Raw, "e2e-42") })
}
