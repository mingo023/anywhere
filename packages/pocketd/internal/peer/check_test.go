package peer

import (
	"testing"

	"pocketd/internal/devices"
	"pocketd/internal/proto"
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
		"ws:local.create":       {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:local.rename":       {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:local.delete":       {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:automation.save":    {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:automation.enable":  {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:automation.delete":  {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:automation.run":     {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:terminal.list":      {"", "", "", ""},
		"ws:terminal.attach":    {"", "", "", ""},
		"ws:terminal.detach":    {"", "", "", ""},
		"ws:branch.list":        {"", "", "", ""},
		"ws:account.list":       {"", "", "", ""},
		"ws:git.status":         {"", "", "", ""},
		"ws:git.watch":          {"", "", "", ""},
		"ws:git.diff":           {"", "", "", ""},
		"ws:git.log":            {"", "", "", ""},
		"ws:git.commit":         {"", "", "", ""},
		"ws:gh.status":          {"", "", "", ""},
		"ws:gh.pr":              {"", "", "", ""},
		"ws:gh.prs":             {"", "", "", ""},
		"ws:fs.list":            {"", "", "", ""},
		"ws:fs.read":            {"", "", "", ""},
		"ws:fs.search":          {"", "", "", ""},
		"ws:push.register":      {"", "", "", ""},
		"ws:push.activity":      {"", "", "", ""},
		"ws:push.prefs":         {"", "", "", ""},
		"ws:push.unregister":    {"", "", "", ""},
		"ws:push.test":          {"", "", "", ""},
		"ws:terminal.input":     {"", "", "", "scope_denied"},
		"ws:terminal.resize":    {"", "", "", "scope_denied"},
		"ws:terminal.create":    {"", "", "", "scope_denied"},
		"ws:terminal.close":     {"", "", "", "scope_denied"},
		"ws:agent.account":      {"", "", "", "scope_denied"},
		"ws:git.stage":          {"", "", "", "scope_denied"},
		"ws:git.discard":        {"", "", "", "scope_denied"},
		"ws:git.commit.create":  {"", "", "", "scope_denied"},
		"ws:git.push":           {"", "", "", "scope_denied"},
		"ws:git.pull":           {"", "", "", "scope_denied"},
		"ws:gh.pr.create":       {"", "", "", "scope_denied"},
		"ws:gh.pr.merge":        {"", "", "", "scope_denied"},
		"ws:gh.pr.ready":        {"", "", "", "scope_denied"},
		"ws:gh.thread.reply":    {"", "", "", "scope_denied"},
		"ws:gh.thread.resolve":  {"", "", "", "scope_denied"},
		"ws:attach.put":         {"", "", "", "scope_denied"},
		"ws:worktree.create":    {"", "", "scope_denied", "scope_denied"},
		"ws:worktree.delete":    {"", "", "scope_denied", "scope_denied"},
		"ops:list":              {"", "", "", ""},
		"ops:status":            {"", "", "", ""},
		"ops:hook":              {"", "", "", ""},
		"ops:pipe":              {"", "", "", ""},
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

func TestASideQuestionNeedsDriveLikeAPrompt(t *testing.T) {
	phone := FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	pty := Principal{Kind: PTY, Pid: 2, Terminal: "t1", Scopes: PTYScopes}
	if got := code(OwnerOf(1).Check("ws:agent.btw", "", "")); got != "" {
		t.Errorf("owner: %q", got)
	}
	if got := code(phone.Check("ws:agent.btw", "", "")); got != "" {
		t.Errorf("phone: %q", got)
	}
	if got := code(pty.Check("ws:agent.btw", "", "")); got != "scope_denied" {
		t.Errorf("pty: %q", got)
	}
}

func TestEveryPhoneVerbHasAScope(t *testing.T) {
	for _, verb := range proto.PhoneVerbs() {
		if _, ok := Needs["ws:"+verb]; !ok {
			t.Errorf("ws:%s is missing from Needs", verb)
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
