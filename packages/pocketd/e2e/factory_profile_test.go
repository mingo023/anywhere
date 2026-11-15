//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
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

func TestAConfigThatNamesAProfileThatIsGoneIsRefused(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapFactory)
	o.Send(map[string]any{"type": "factory.config.save", "id": "cfg", "project": h.Repo, "config": factoryConfig("nope")})
	m := o.WaitFor("the reply", func(m Message) bool { return m.ID == "cfg" })
	if m.Type != "error" || !strings.Contains(m.Raw, `"unknown_profile"`) || m.Message != "No profile nope" {
		t.Fatalf("got %s", m.Raw)
	}
}

func TestAWorkerWithAProfileStartsWithItsPromptAndItsSkillsAndCommitsNeither(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity)
	o := h.Owner("names.v1", proto.CapFactory)
	o.Send(map[string]any{"type": "factory.profile.save", "id": "save", "profile": goBackendDoc()})
	if m := o.WaitFor("the saved profile", func(m Message) bool { return m.ID == "save" }); m.Type != "factory.profiles" {
		t.Fatalf("save: %s", m.Raw)
	}
	create(o, "lead", inRepo(h, map[string]any{"chat": true}))
	created := reply(o, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	lead := created.AgentID
	h.WaitScreen(created.TerminalID, "fake claude ready")
	o.Send(map[string]any{"type": "agent.prompt", "id": "plan", "agentId": lead, "text": teamPlan(t), "team": true})
	o.WaitFor("the proposed plan", teamOf(lead, "proposed"))
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": lead, "tasks": []map[string]any{
		{"id": "t1", "agent": "codex", "profile": "go-backend", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	var t1 proto.TeamTask
	o.WaitFor("t1's handback", func(m Message) bool {
		if !taskIn(lead, "t1", "done")(m) {
			return false
		}
		t1 = m.Team.Tasks[0]
		return true
	})
	if t1.Profile != "go-backend" || t1.Agent != "claude" || t1.Model != "sonnet" {
		t.Fatalf("t1 runs %+v", t1)
	}
	if text, err := os.ReadFile(filepath.Join(t1.Worktree, ".claude", "skills", "review", "SKILL.md")); err != nil || string(text) != "# Review\n" {
		t.Fatalf("the skill in the checkout: %q %v", text, err)
	}
	if files := gitOut(t, t1.Worktree, "log", "-1", "--name-only", "--format="); files != "a.txt" {
		t.Fatalf("the handback commit holds %q", files)
	}
	if left := gitOut(t, t1.Worktree, "status", "--porcelain"); left != "" {
		t.Fatalf("git lists %q", left)
	}
	prompts, _ := filepath.Glob(filepath.Join(h.Home, "state", "factory", "prompts", "*.md"))
	if len(prompts) != 1 {
		t.Fatalf("prompt files %q", prompts)
	}
	if text, _ := os.ReadFile(prompts[0]); !strings.HasPrefix(string(text), "<anywhere_factory_rules>") || !strings.Contains(string(text), "You are careful.") {
		t.Fatalf("the prompt file:\n%s", text)
	}
	o.Send(map[string]any{"type": "agent.list", "id": "al"})
	list := o.WaitFor("the agent list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "al" })
	var v struct {
		Agents []struct{ ID, TerminalID string } `json:"agents"`
	}
	json.Unmarshal([]byte(list.Raw), &v)
	term := ""
	for _, a := range v.Agents {
		if a.ID == t1.Worker {
			term = a.TerminalID
		}
	}
	if term == "" {
		t.Fatalf("t1's worker %s isn't listed: %s", t1.Worker, list.Raw)
	}
	// Fake claude prints the first line of the file its argv names.
	h.WaitScreen(term, "system prompt: <anywhere_factory_rules>")
}

func TestMovingAnUnknownTicketToChecksIsRefusedByItsCode(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner("board.v1", "team.v1", "factory.v1")
	o.Send(map[string]any{"type": "factory.advance", "id": "adv", "ticketId": "nope"})
	if m := o.WaitFor("the reply", func(m Message) bool { return m.ID == "adv" }); m.Type != "error" || !strings.Contains(m.Raw, `"unknown_ticket"`) {
		t.Fatalf("got %s", m.Raw)
	}
}
