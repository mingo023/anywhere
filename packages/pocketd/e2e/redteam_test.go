package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestRedTeamFromInsideATerminal is the no-self-approval gate: a process inside
// a Pocket Terminal tries every way to act as the owner while a claude waits on
// an ask.
func TestRedTeamFromInsideATerminal(t *testing.T) {
	h, phone, term, agent := StartClaude(t)
	h.Prompt(term, "run ls")
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })
	out := filepath.Join(h.Home, "redteam.out")
	h.Spawn(filepath.Join(binDir, "redteam"), "inside", out, term, agent, req.Request.RequestID)
	var raw []byte
	h.eventually("the red-team results", func() bool {
		var err error
		raw, err = os.ReadFile(out)
		return err == nil
	})
	got := map[string]string{}
	for _, line := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		probe, outcome, _ := strings.Cut(line, " ")
		got[probe] = outcome
	}
	for probe, want := range map[string]string{
		"ws.scopes":          "observe",
		"ws.resolve":         "scope_denied",
		"ws.pair.begin":      "scope_denied",
		"ws.prompt":          "ask_open",
		"ws.close":           "ask_open",
		"ops.spawn":          "scope_denied",
		"ops.attach":         "scope_denied",
		"ops.screen":         "scope_denied",
		"ops.input":          "ask_open",
		"ops.prompt":         "ask_open",
		"ops.devices":        "scope_denied",
		"ops.devices.rename": "scope_denied",
		"ops.devices.revoke": "scope_denied",
		"ops.pair.begin":     "scope_denied",
		"hook.other":         "not_own_terminal",
		"hook.own":           "hook_forged",
		"ws.create":          "scope_denied",
		"ws.config.set":      "scope_denied",
		"ops.config-set":     "scope_denied",
		"ops.launch-exit":    "not_own_terminal",
	} {
		if got[probe] != want {
			t.Errorf("%s: %q, want %q", probe, got[probe], want)
		}
	}
	if os.Getenv("REDTEAM_RESIDUALS") == "1" {
		for _, probe := range []string{"residual.nohup", "residual.launchctl", "residual.tiocsti"} {
			if _, ok := got[probe]; !ok {
				t.Errorf("%s didn't run", probe)
			}
		}
	}
	for probe, outcome := range got {
		if name, ok := strings.CutPrefix(probe, "residual."); ok {
			t.Logf("residual: %s %s", name, outcome)
		}
	}
	if screen := h.Screen(term); strings.Contains(screen, "hook: ") || strings.Contains(screen, "desktop: ") {
		t.Fatalf("the ask was answered from inside:\n%s", screen)
	}
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "deny"})
	h.WaitScreen(term, "hook: deny")
}
