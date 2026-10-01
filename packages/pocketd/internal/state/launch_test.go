package state

import "testing"

func TestParseReadsBackWhatEveryLaunchArgvAsksFor(t *testing.T) {
	for _, c := range []struct {
		provider string
		argv     []string
		want     Launch
	}{
		{"claude", []string{"claude", "--permission-mode", "default"}, Launch{Access: "ask"}},
		{"claude", []string{"claude", "--permission-mode", "acceptEdits"}, Launch{Access: "edits"}},
		{"claude", []string{"claude", "--permission-mode", "auto"}, Launch{Access: "auto"}},
		{"claude", []string{"claude", "--permission-mode", "bypassPermissions", "--allow-dangerously-skip-permissions"}, Launch{Access: "full"}},
		{"claude", []string{"claude", "--dangerously-skip-permissions"}, Launch{Access: "full"}},
		{"claude", []string{"claude", "--permission-mode", "plan"}, Launch{Access: "ask", Plan: true}},
		{"claude", []string{"claude", "--permission-mode=acceptEdits", "--model", "opus", "--effort", "high", "--", "--model x"}, Launch{Access: "edits", Model: "opus", Effort: "high"}},
		{"codex", []string{"codex", "-s", "read-only", "-a", "on-request"}, Launch{Access: "ask"}},
		{"codex", []string{"codex", "-s", "workspace-write", "-a", "on-request"}, Launch{Access: "edits"}},
		{"codex", []string{"codex", "--approve-for-me"}, Launch{Access: "auto"}},
		{"codex", []string{"codex", "-s", "danger-full-access", "-a", "never"}, Launch{Access: "full"}},
		{"codex", []string{"codex", "--dangerously-bypass-approvals-and-sandbox"}, Launch{Access: "full"}},
		{"codex", []string{"codex", "--sandbox=workspace-write", "-m", "gpt-5.5", "--", "-s danger-full-access"}, Launch{Access: "edits", Model: "gpt-5.5"}},
	} {
		if got := Parse(c.provider, c.argv); got != c.want {
			t.Errorf("Parse(%q) = %+v, want %+v", c.argv, got, c.want)
		}
	}
}

func TestParseUnknownIsAsk(t *testing.T) {
	for _, argv := range [][]string{{"claude"}, {"claude", "--permission-mode", "dontAsk"}, {"codex", "-s", "sideways"}, {"codex", "--model"}} {
		if got := Parse(argv[0], argv); got.Access != "ask" || got.Plan {
			t.Errorf("Parse(%q) = %+v, want ask", argv, got)
		}
	}
}

func TestModeFollowsClaudePermissionModes(t *testing.T) {
	l := Launch{Access: "edits", Model: "opus"}
	if got := Mode(l, "plan"); got != (Launch{Access: "edits", Plan: true, Model: "opus"}) {
		t.Errorf("plan: %+v", got)
	}
	if got := Mode(Launch{Access: "ask", Plan: true}, "bypassPermissions"); got != (Launch{Access: "full"}) {
		t.Errorf("bypassPermissions: %+v", got)
	}
	if got := Mode(l, "dontAsk"); got != l {
		t.Errorf("unknown: %+v", got)
	}
}
