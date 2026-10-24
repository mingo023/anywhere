package wsserver

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/accounts"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/launch"
	"pocketd/internal/registry"
	"pocketd/internal/skills"
)

func withSkills(t *testing.T, project string) (home string, opt func(*Server)) {
	home, dir := t.TempDir(), t.TempDir()
	path := filepath.Join(dir, "desktop.json")
	if err := os.WriteFile(path, []byte(`{"projects":["`+project+`"],"repos":{}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	store, err := skills.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	d := &daemon.Daemon{Accounts: &accounts.Store{Root: filepath.Join(home, ".anywhere", "accounts"), Home: home}}
	return home, func(s *Server) {
		s.Launch, s.Skills = launch.New(d, registry.New(path), config.NewSettings(dir), nil), store
	}
}

func TestSkillsListSendsTheProjectsSkillsOnItsAccountBeforeAnyAgentRuns(t *testing.T) {
	project := t.TempDir()
	home, opt := withSkills(t, project)
	skill := filepath.Join(home, ".claude", "skills", "ship", "SKILL.md")
	os.MkdirAll(filepath.Dir(skill), 0o755)
	os.WriteFile(skill, []byte("---\ndescription: Ship it\n---\n"), 0o644)
	_, _, p := setup(t, opt)
	p.hello()
	p.send(`{"type":"skills.list","id":"s1","provider":"claude","project":"` + project + `"}`)
	if m := p.recv(); m["type"] != "skills.list" || !reflect.DeepEqual(m["commands"], []any{}) {
		t.Fatalf("kept: %v", m)
	}
	m := p.recv()
	want := []any{map[string]any{"name": "ship", "description": "Ship it"}}
	if m["type"] != "skills.list" || m["id"] != "s1" || m["provider"] != "claude" || m["project"] != project || !reflect.DeepEqual(m["commands"], want) {
		t.Fatalf("found: %v", m)
	}
}

func TestSkillsListRefusesAFolderThatIsntAProject(t *testing.T) {
	_, opt := withSkills(t, t.TempDir())
	_, _, p := setup(t, opt)
	p.hello()
	p.send(`{"type":"skills.list","id":"s1","provider":"claude","project":"/etc"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "s1" {
		t.Fatalf("%v", m)
	}
}
