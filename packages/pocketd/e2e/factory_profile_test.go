//go:build unix

package e2e

import (
	"encoding/json"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func profileList(t *testing.T, m Message) []proto.ProfileInfo {
	t.Helper()
	var v struct {
		Profiles []proto.ProfileInfo `json:"profiles"`
	}
	if err := json.Unmarshal([]byte(m.Raw), &v); err != nil {
		t.Fatal(err)
	}
	return v.Profiles
}

func goBackendDoc() map[string]any {
	return map[string]any{"name": "Go backend", "provider": "claude", "model": "sonnet", "soul": "You are careful.", "instructions": "Write the test first.",
		"files": []map[string]any{{"path": "skills/review/SKILL.md", "text": "# Review\n"}}}
}

func factoryConfig(workers string) map[string]any {
	return map[string]any{"enabled": false, "spec": true, "reviewer": true, "lead": "claude-opus", "workers": workers, "review": "codex", "slots": 0, "fix": 2, "checks": []string{}}
}

func TestAProfileIsSavedListedReadAndKeptWhileAProjectUsesIt(t *testing.T) {
	h := Start(t, launchReady(""))
	o, other := h.Owner(proto.CapFactory), h.Owner(proto.CapFactory)
	ask := func(id string, m map[string]any) Message {
		m["id"] = id
		o.Send(m)
		return o.WaitFor("the reply to "+id, func(m Message) bool { return m.ID == id })
	}
	saved := ask("save", map[string]any{"type": "factory.profile.save", "profile": goBackendDoc()})
	list := profileList(t, saved)
	if saved.Type != "factory.profiles" || len(list) != 4 || !list[0].BuiltIn || list[3].ID != "go-backend" || list[3].Skills != 1 {
		t.Fatalf("save: %s", saved.Raw)
	}
	other.WaitFor("the push to the other owner", func(m Message) bool { return m.Type == "factory.profiles" && m.ID == "" && len(profileList(t, m)) == 4 })
	got := ask("get", map[string]any{"type": "factory.profile.get", "profile": "go-backend"})
	if got.Type != "factory.profile" || !strings.Contains(got.Raw, `"path":"skills/review/SKILL.md"`) || !strings.Contains(got.Raw, `"soul":"You are careful."`) {
		t.Fatalf("get: %s", got.Raw)
	}
	if m := ask("twin", map[string]any{"type": "factory.profile.save", "profile": goBackendDoc()}); m.Type != "error" || !strings.Contains(m.Raw, `"invalid_profile"`) {
		t.Fatalf("a second profile with the name: %s", m.Raw)
	}
	if m := ask("cfg", map[string]any{"type": "factory.config.save", "project": h.Repo, "config": factoryConfig("go-backend")}); m.Type == "error" {
		t.Fatalf("config: %s", m.Raw)
	}
	kept := ask("del", map[string]any{"type": "factory.profile.delete", "profile": "go-backend"})
	if kept.Type != "error" || !strings.Contains(kept.Raw, `"profile_in_use"`) || !strings.Contains(kept.Message, "Workers role") {
		t.Fatalf("a delete of a profile in use: %s", kept.Raw)
	}
	ask("cfg2", map[string]any{"type": "factory.config.save", "project": h.Repo, "config": factoryConfig("claude-sonnet")})
	if gone := ask("del2", map[string]any{"type": "factory.profile.delete", "profile": "go-backend"}); gone.Type != "factory.profiles" || len(profileList(t, gone)) != 3 {
		t.Fatalf("delete: %s", gone.Raw)
	}
	if m := ask("builtin", map[string]any{"type": "factory.profile.delete", "profile": "codex"}); m.Type != "error" || !strings.Contains(m.Raw, `"invalid_profile"`) {
		t.Fatalf("a delete of a built-in: %s", m.Raw)
	}
	if m := ask("none", map[string]any{"type": "factory.profile.get", "profile": "nope"}); m.Type != "error" || !strings.Contains(m.Raw, `"unknown_profile"`) {
		t.Fatalf("an unknown profile: %s", m.Raw)
	}
}

func TestAPhoneCantReadOrWriteAProfile(t *testing.T) {
	h := Start(t, launchReady(""))
	p := h.Paired(proto.CapFactory)
	for _, verb := range []string{"factory.profile.get", "factory.profile.delete"} {
		p.Send(map[string]any{"type": verb, "id": verb, "profile": "codex"})
		if m := p.WaitFor(verb+"'s reply", func(m Message) bool { return m.ID == verb }); m.Type != "error" {
			t.Fatalf("%s: %s", verb, m.Raw)
		}
	}
}
