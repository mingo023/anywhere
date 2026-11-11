package state

import (
	"testing"

	"pocketd/internal/providers"
)

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
		{"codex", []string{"codex", "app-server", "-c", `sandbox_mode="read-only"`, "-c", `approval_policy="on-request"`, "-c", `model="gpt-6-luna"`, "-c", `model_reasoning_effort="low"`}, Launch{Access: "ask", Model: "gpt-6-luna", Effort: "low"}},
		{"codex", []string{"codex", "app-server", "-c", `sandbox_mode="workspace-write"`, "-c", `approvals_reviewer="auto_review"`}, Launch{Access: "auto"}},
		{"codex", []string{"codex", "--sandbox=workspace-write", "-m", "gpt-5.5", "--", "-s danger-full-access"}, Launch{Access: "edits", Model: "gpt-5.5"}},
		{"cursor", []string{"cursor-agent", "--model", "gpt-6", "fix it"}, Launch{Access: "settings", Model: "gpt-6"}},
		{"grok", []string{"grok", "--model", "grok-4.6", "--reasoning-effort", "high"}, Launch{Access: "settings", Model: "grok-4.6", Effort: "high"}},
		{"grok", []string{"grok", "--always-approve"}, Launch{Access: "full"}},
		{"grok", []string{"grok", "--permission-mode", "plan"}, Launch{Access: "settings", Plan: true}},
		{"opencode", []string{"opencode", "--agent", "plan", "-m", "opencode/big-pickle", "--prompt", "--auto"}, Launch{Access: "settings", Plan: true, Model: "opencode/big-pickle"}},
		{"opencode", []string{"opencode", "-s", "ses_1", "--fork", "--auto"}, Launch{Access: "full"}},
		{"opencode", []string{"opencode", "--agent", "build"}, Launch{Access: "settings"}},
		{"pi", []string{"pi", "--model", "deepseek/deepseek-v4-pro", "--thinking", "high", "--", "--tools x"}, Launch{Access: "full", Model: "deepseek/deepseek-v4-pro", Effort: "high"}},
		{"pi", []string{"pi", "--tools", "read,grep,find,ls"}, Launch{Access: "full", Plan: true}},
		{"pi", []string{"pi", "", "", ""}, Launch{Access: "full"}},
		{"omp", []string{"omp", "--tools", "read,grep,glob,lsp", "--model", "x/y"}, Launch{Access: "full", Plan: true, Model: "x/y"}},
		{"fx", []string{"fx", "--model", "gpt-6"}, Launch{Access: "settings", Model: "gpt-6"}},
		{"hermes", []string{"hermes", "chat", "-m", "gpt-6-sol", "--reasoning", "low", "--yolo", "-q", "--model x"}, Launch{Access: "full", Model: "gpt-6-sol", Effort: "low"}},
		{"hermes", []string{"hermes", "chat", "--resume", "local-1"}, Launch{Access: "settings"}},
		{"antigravity", []string{"agy"}, Launch{Access: "settings"}},
	} {
		if got := Parse(c.provider, c.argv); got != c.want {
			t.Errorf("Parse(%q) = %+v, want %+v", c.argv, got, c.want)
		}
	}
}

func TestParseUnknownIsAsk(t *testing.T) {
	for _, argv := range [][]string{{"claude", "--permission-mode", "dontAsk"}, {"codex", "-s", "sideways"}} {
		if got := Parse(argv[0], argv); got.Access != "ask" || got.Plan {
			t.Errorf("Parse(%q) = %+v, want ask", argv, got)
		}
	}
}

func TestParseWithoutAccessFlagsIsTheAgentsSettings(t *testing.T) {
	for _, argv := range [][]string{{"claude"}, {"claude", "--model", "opus", "--", "--permission-mode auto"}, {"codex"}, {"codex", "--model"}} {
		if got := Parse(argv[0], argv); got.Access != "settings" || got.Plan {
			t.Errorf("Parse(%q) = %+v, want settings", argv, got)
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

func TestEveryProviderHasAParser(t *testing.T) {
	for _, p := range providers.All {
		if parsers[p.ID] == nil {
			t.Errorf("%s has no parser", p.ID)
		}
	}
}

func TestASwitchedPermissionModeReadsBackAsItsAccess(t *testing.T) {
	for _, access := range []string{"ask", "edits", "auto", "full"} {
		if got := Mode(Launch{Access: "settings", Plan: true}, PermissionMode(access)); got.Access != access || got.Plan {
			t.Errorf("%s: got %+v", access, got)
		}
	}
}

func TestAChatArgvCarriesTheLaunchItsCommandHasNoFlagsFor(t *testing.T) {
	for _, c := range []struct {
		provider string
		l        Launch
	}{
		{"hermes", Launch{Access: "edits", Model: "openai-codex:gpt-6-sol", Effort: "high"}},
		{"opencode", Launch{Access: "ask", Plan: true, Model: "opencode/big-pickle"}},
		{"cursor", Launch{Access: "settings"}},
		{"antigravity", Launch{Access: "full"}},
		{"grok", Launch{Access: "auto", Model: "grok-4.6"}},
		{"omp", Launch{Access: "full", Plan: true, Effort: "xhigh"}},
	} {
		argv := append([]string{c.provider, "acp"}, c.l.Carried()...)
		if got := Parse(c.provider, argv); got != c.l {
			t.Errorf("%s: Parse(%q) = %+v, want %+v", c.provider, argv, got, c.l)
		}
		if run, l, ok := Cut(argv); !ok || l != c.l || len(run) != 2 || run[1] != "acp" {
			t.Errorf("%s: Cut(%q) = %q, %+v, %v", c.provider, argv, run, l, ok)
		}
	}
}

func TestAnArgvThatCarriesNoLaunchIsRunWhole(t *testing.T) {
	argv := []string{"claude", "-p", "--model", "opus"}
	if run, _, ok := Cut(argv); ok || len(run) != len(argv) {
		t.Fatalf("Cut = %q, %v", run, ok)
	}
}

func TestACarriedAccessItCantReadIsAsk(t *testing.T) {
	if got := Parse("hermes", []string{"hermes", "acp", Carry, "--access", "sideways"}); got.Access != "ask" {
		t.Fatalf("got %+v", got)
	}
}

// pi and omp run every tool without asking, whatever their launch asked for.
func TestAnAgentThatNeverAsksIsFullWhateverItsArgvSays(t *testing.T) {
	for _, provider := range []string{"pi", "omp"} {
		argv := append([]string{provider, "--mode", "rpc"}, Launch{Access: "ask", Model: "a/b"}.Carried()...)
		if got := Parse(provider, argv); got != (Launch{Access: "full", Model: "a/b"}) {
			t.Errorf("%s chat: %+v", provider, got)
		}
		if got := Parse(provider, []string{provider}); got.Access != "full" {
			t.Errorf("%s TUI: %+v", provider, got)
		}
	}
}
