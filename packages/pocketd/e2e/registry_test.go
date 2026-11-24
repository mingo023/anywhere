//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

// registryHome seeds desktop.json with two clones of one repo, another host's project and some shared settings.
func registryHome(h *Harness) {
	for _, dir := range []string{"a/app", "b/app"} {
		os.MkdirAll(filepath.Join(h.Home, dir), 0o755)
	}
	a, b := filepath.Join(h.Home, "a", "app"), filepath.Join(h.Home, "b", "app")
	raw, _ := json.Marshal(map[string]any{
		"projects": []string{a, "//box/code/x", b},
		"repos":    map[string]any{a: map[string]any{"name": "app", "color": 7}, b: map[string]any{"name": "app"}, "//box/code/x": map[string]any{"name": "x"}},
		"tracked":  []string{filepath.Join(h.Home, "wt", "one"), "//box/wt/two"},
		"worktree": map[string]any{"root": "/wt"},
	})
	if err := os.WriteFile(filepath.Join(h.Home, "desktop.json"), raw, 0o600); err != nil {
		h.t.Fatal(err)
	}
}

func registryList(p *Phone, what string, ok func(proto.RegistryList) bool) proto.RegistryList {
	p.t.Helper()
	var l proto.RegistryList
	p.WaitFor(what, func(m Message) bool {
		l = proto.RegistryList{}
		return m.Type == "registry.list" && json.Unmarshal([]byte(m.Raw), &l) == nil && ok(l)
	})
	return l
}

func anyList(proto.RegistryList) bool { return true }

func paths(l proto.RegistryList) []string {
	var out []string
	for _, p := range l.Projects {
		out = append(out, p.Path)
	}
	return out
}

func TestPocketdServesTheProjectsItImportedAndKeepsThemAcrossARestart(t *testing.T) {
	h := Start(t, registryHome)
	a, b := filepath.Join(h.Home, "a", "app"), filepath.Join(h.Home, "b", "app")
	l := registryList(h.Owner("registry.v2"), "the list on hello", anyList)
	if !slices.Equal(paths(l), []string{a, b}) || l.Projects[0].ID == l.Projects[1].ID || l.Projects[0].Repo.Color != 7 ||
		l.Settings.WorktreeRoot != "/wt" || !slices.Equal(l.Tracked, []string{filepath.Join(h.Home, "wt", "one")}) {
		t.Fatalf("%+v", l)
	}

	if err := os.WriteFile(filepath.Join(h.Home, "desktop.json"), []byte(`{"projects":["/elsewhere"],"worktree":{"root":"/other"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	h.Restart()
	again := registryList(h.Owner("registry.v2"), "the list after a restart", anyList)
	if !slices.Equal(paths(again), []string{a, b}) || again.Projects[0].ID != l.Projects[0].ID || again.Projects[1].ID != l.Projects[1].ID || again.Settings.WorktreeRoot != "/wt" {
		t.Fatalf("%+v", again)
	}
}

func TestAnOwnerAddsAndRemovesProjectsAndEveryClientIsToldAndARestartKeepsThem(t *testing.T) {
	h := Start(t, registryHome)
	c := filepath.Join(h.Home, "c")
	os.MkdirAll(c, 0o755)
	owner, watcher := h.Owner("registry.v2"), h.Owner("registry.v2")
	registryList(owner, "owner's first list", anyList)
	registryList(watcher, "watcher's first list", anyList)

	owner.Send(map[string]any{"type": "project.add", "id": "add", "path": c, "repo": map[string]any{"name": "C"}})
	owner.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "add" })
	has := func(l proto.RegistryList) bool { return slices.Contains(paths(l), c) }
	added := registryList(watcher, "the add reaching the other client", has)
	var id string
	for _, p := range added.Projects {
		if p.Path == c {
			id = p.ID
		}
	}
	if id == "" {
		t.Fatalf("%+v", added)
	}

	h.Restart()
	owner = h.Owner("registry.v2")
	if l := registryList(owner, "the list after a restart", has); l.Projects[2].ID != id {
		t.Fatalf("%+v", l)
	}
	owner.Send(map[string]any{"type": "project.remove", "id": "rm", "path": c})
	owner.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "rm" })
	registryList(owner, "the removal", func(l proto.RegistryList) bool { return !has(l) })

	owner.Send(map[string]any{"type": "project.set", "id": "set", "path": c, "repo": map[string]any{}})
	owner.WaitFor("unknown project", func(m Message) bool { return m.Type == "error" && m.ID == "set" && m.Code == proto.CodeUnknownProject })
}

func TestTheDesktopWithoutTheRegistryCapIsRefusedAndAPhoneWithoutItIsServedItsProjectsWithIds(t *testing.T) {
	h := Start(t, registryHome)
	desktop := h.DialOwner()
	desktop.Send(map[string]any{"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3})
	desktop.WaitFor("client_too_old", func(m Message) bool { return m.Type == "error" && m.Code == proto.CodeClientTooOld })

	phone := h.Paired()
	phone.Send(map[string]any{"type": "project.list", "id": "pl"})
	m := phone.WaitFor("project.list", func(m Message) bool { return m.Type == "project.list" && m.ID == "pl" })
	var list proto.ProjectList
	if err := json.Unmarshal([]byte(m.Raw), &list); err != nil {
		t.Fatal(err)
	}
	if len(list.Projects) != 2 || list.Projects[0].ID == "" || list.Projects[0].ID == list.Projects[1].ID {
		t.Fatalf("%s", m.Raw)
	}
}
