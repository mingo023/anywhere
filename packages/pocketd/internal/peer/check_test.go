package peer

import (
	"testing"

	"pocketd/internal/devices"
)

func code(r *Refusal) string {
	if r == nil {
		return ""
	}
	return r.Code
}

func TestEveryVerbAndPrincipalMatchesTheMatrix(t *testing.T) {
	owner := OwnerOf(1)
	phone := FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	legacy := FromDevice(devices.Device{ID: devices.LegacyID, Scopes: devices.LegacyScopes})
	pty := Principal{Kind: PTY, Pid: 2, Terminal: "t1", Scopes: PTYScopes}
	for verb, want := range map[string][4]string{
		"ws:agent.list":         {"", "", "", ""},
		"ws:project.list":       {"", "", "", ""},
		"ws:agent.prompt":       {"", "", "", "scope_denied"},
		"ws:agent.close":        {"", "", "", "scope_denied"},
		"ws:agent.pin":          {"", "", "", "scope_denied"},
		"ws:permission.resolve": {"", "", "", "scope_denied"},
		"ws:pair.begin":         {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:agent.create":       {"", "", "scope_denied", "scope_denied"},
		"ws:agent.providers":    {"", "", "", ""},
		"ws:config.set":         {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:worktree.rename":    {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:list":              {"", "", "", ""},
		"ops:status":            {"", "", "", ""},
		"ops:hook":              {"", "", "", ""},
		"ops:spawn":             {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:attach":            {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:screen":            {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:input":             {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:pair.begin":        {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:launch-exit":       {"", "", "", ""},
		"ops:config-set":        {"", "scope_denied", "scope_denied", "scope_denied"},
		"ops:upgrade":           {"", "scope_denied", "scope_denied", "scope_denied"},
	} {
		for i, p := range []Principal{owner, phone, legacy, pty} {
			if got := code(p.Check(verb, "", "")); got != want[i] {
				t.Errorf("%s by principal %d: %q, want %q", verb, i, got, want[i])
			}
		}
	}
}

func TestAnUnknownVerbIsRefusedEvenForTheOwner(t *testing.T) {
	if got := code(OwnerOf(1).Check("ops:bogus", "", "")); got != "scope_denied" {
		t.Fatalf("got %q", got)
	}
}

func TestAPtyPeerIsToldAboutTheOpenAskBeforeItsScope(t *testing.T) {
	pty := Principal{Kind: PTY, Terminal: "t1", Scopes: PTYScopes}
	for _, verb := range []string{"ops:input", "ops:prompt", "ws:agent.prompt", "ws:agent.close"} {
		if got := code(pty.Check(verb, "t2", "1")); got != "ask_open" {
			t.Errorf("%s: %q", verb, got)
		}
	}
	if got := code(pty.Check("ops:attach", "t2", "")); got != "scope_denied" {
		t.Errorf("attach: %q", got)
	}
	if got := code(OwnerOf(1).Check("ops:input", "t2", "1")); got != "" {
		t.Errorf("owner input: %q", got)
	}
}

func TestAPtyPromptStartingWithBangOrSlashIsRefused(t *testing.T) {
	pty := Principal{Kind: PTY, Terminal: "t1", Scopes: []Scope{Observe, Drive}}
	for text, want := range map[string]string{"!rm -rf ~": "prompt_refused", "/compact": "prompt_refused", "fix the test": ""} {
		if got := code(pty.Check("ws:agent.prompt", "", text)); got != want {
			t.Errorf("%q: %q, want %q", text, got, want)
		}
	}
	if got := code(OwnerOf(1).Check("ws:agent.prompt", "", "/compact")); got != "" {
		t.Errorf("owner /compact: %q", got)
	}
}

func TestAScopeRefusalNamesTheFix(t *testing.T) {
	r := Principal{Kind: PTY, Scopes: PTYScopes}.Check("ops:attach", "", "")
	if r.Message != "attach needs owner; run it outside Pocket Terminals, or against a scratch pocketd (POCKETD_SOCK)" {
		t.Fatalf("%q", r.Message)
	}
}
