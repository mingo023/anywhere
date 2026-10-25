package skills

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func write(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func names(list []proto.Command) []string {
	out := []string{}
	for _, c := range list {
		out = append(out, c.Name)
	}
	return out
}

func TestClaudeListsTheProjectsThenTheAccountsSkillsAndCommandsByTheirFolder(t *testing.T) {
	config, cwd := t.TempDir(), t.TempDir()
	write(t, filepath.Join(cwd, ".claude", "skills", "ship", "SKILL.md"), "---\nname: shipping\ndescription: Ship it\nargument-hint: <tag>\n---\nbody")
	write(t, filepath.Join(cwd, ".claude", "commands", "lint.md"), "---\ndescription: Lint\n---\n")
	write(t, filepath.Join(config, "skills", "ship", "SKILL.md"), "---\ndescription: The account's twin\n---\n")
	write(t, filepath.Join(config, "skills", "fix-root-cause", "SKILL.md"), "---\nname: fix-root-causes\n---\n")
	write(t, filepath.Join(config, "commands", "deploy", "staging.md"), "no frontmatter")
	list := Scan("claude", config, t.TempDir(), cwd)
	if got := names(list); !reflect.DeepEqual(got, []string{"ship", "lint", "fix-root-cause", "staging"}) {
		t.Fatalf("names = %v", got)
	}
	if list[0] != (proto.Command{Name: "ship", Description: "Ship it", ArgumentHint: "<tag>"}) {
		t.Fatalf("project skill = %+v", list[0])
	}
}

func TestASkillUsersCantInvokeIsLeftOut(t *testing.T) {
	config := t.TempDir()
	write(t, filepath.Join(config, "skills", "background", "SKILL.md"), "---\nuser-invocable: false\n---\n")
	if got := Scan("claude", config, t.TempDir(), t.TempDir()); len(got) != 0 {
		t.Fatalf("got %v", got)
	}
}

func TestEnabledPluginsInstalledForTheUserOrThisProjectAddTheirSkillsUnderTheirName(t *testing.T) {
	config, cwd, other := t.TempDir(), t.TempDir(), t.TempDir()
	user, project, off, elsewhere := t.TempDir(), t.TempDir(), t.TempDir(), t.TempDir()
	write(t, filepath.Join(user, "skills", "create", "SKILL.md"), "---\ndescription: Make a skill\n---\n")
	write(t, filepath.Join(project, "commands", "review.md"), "")
	write(t, filepath.Join(off, "skills", "off", "SKILL.md"), "")
	write(t, filepath.Join(elsewhere, "skills", "elsewhere", "SKILL.md"), "")
	write(t, filepath.Join(config, "plugins", "installed_plugins.json"), `{"version":2,"plugins":{
		"skill-creator@official":[{"scope":"user","installPath":"`+user+`"}],
		"pr@official":[{"scope":"project","installPath":"`+project+`","projectPath":"`+cwd+`"}],
		"off@official":[{"scope":"user","installPath":"`+off+`"}],
		"other@official":[{"scope":"local","installPath":"`+elsewhere+`","projectPath":"`+other+`"}]}}`)
	write(t, filepath.Join(config, "settings.json"), `{"enabledPlugins":{"skill-creator@official":true,"off@official":true,"other@official":true}}`)
	write(t, filepath.Join(cwd, ".claude", "settings.json"), `{"enabledPlugins":{"pr@official":true,"off@official":false}}`)
	list := Scan("claude", config, t.TempDir(), cwd)
	if got := names(list); !reflect.DeepEqual(got, []string{"pr:review", "skill-creator:create"}) {
		t.Fatalf("names = %v", got)
	}
}

func TestCodexReadsAgentsSkillsFromCwdUpToTheRepoRootThenHome(t *testing.T) {
	above, home := t.TempDir(), t.TempDir()
	repo := filepath.Join(above, "repo")
	cwd := filepath.Join(repo, "pkg")
	write(t, filepath.Join(repo, ".git", "HEAD"), "ref: refs/heads/main")
	write(t, filepath.Join(cwd, ".agents", "skills", "near", "SKILL.md"), "---\nname: near-skill\n---\n")
	write(t, filepath.Join(repo, ".agents", "skills", "root", "SKILL.md"), "---\nname: root\n---\n")
	write(t, filepath.Join(above, ".agents", "skills", "outside", "SKILL.md"), "---\nname: outside\n---\n")
	write(t, filepath.Join(home, ".agents", "skills", "mine", "SKILL.md"), "---\nname: mine\n---\n")
	if got := names(Scan("codex", t.TempDir(), home, cwd)); !reflect.DeepEqual(got, []string{"near-skill", "root", "mine"}) {
		t.Fatalf("names = %v", got)
	}
}

func TestAProviderWithNoHarnessHasNoCommands(t *testing.T) {
	if got := Scan("gemini", t.TempDir(), t.TempDir(), t.TempDir()); got != nil {
		t.Fatalf("got %v", got)
	}
}

func TestFrontmatterJoinsFoldedValuesAndUnquotes(t *testing.T) {
	fm := frontmatter("---\r\nname: 'it''s'\r\ndescription: >-\r\n  Two\r\n  lines\r\nargument-hint: \"<a> \\\"b\\\"\"\r\nmetadata:\r\n  short: x\r\n---\r\ndescription: body")
	want := map[string]string{"name": "it's", "description": "Two lines", "argument-hint": `<a> "b"`, "metadata": ""}
	if !reflect.DeepEqual(fm, want) {
		t.Fatalf("got %q", fm)
	}
	if len(frontmatter("# no frontmatter\nname: x")) != 0 {
		t.Fatal("read past a missing block")
	}
}

func TestListSendsWhatItKeptThenTheScanOnlyWhenItChanged(t *testing.T) {
	s, err := Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	config := t.TempDir()
	k := Key{"claude", config, t.TempDir()}
	var sent [][]string
	send := func(l []proto.Command) { sent = append(sent, names(l)) }
	write(t, filepath.Join(config, "skills", "a", "SKILL.md"), "")
	if err := s.List(k, t.TempDir(), send); err != nil {
		t.Fatal(err)
	}
	if err := s.List(k, t.TempDir(), send); err != nil {
		t.Fatal(err)
	}
	write(t, filepath.Join(config, "skills", "b", "SKILL.md"), "")
	if err := s.List(k, t.TempDir(), send); err != nil {
		t.Fatal(err)
	}
	want := [][]string{{}, {"a"}, {"a"}, {"a"}, {"a", "b"}}
	if !reflect.DeepEqual(sent, want) {
		t.Fatalf("sent %v, want %v", sent, want)
	}
}

func TestTheStoreKeepsEachScanApart(t *testing.T) {
	home := t.TempDir()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	a, b := Key{"claude", "/c", "/a"}, Key{"claude", "/c", "/b"}
	s.Put(a, []proto.Command{{Name: "x", Description: "d", ArgumentHint: "h"}})
	s.Put(b, []proto.Command{{Name: "y"}})
	s.Close()
	s, _ = Open(home)
	defer s.Close()
	got, err := s.Get(a)
	if err != nil || !reflect.DeepEqual(got, []proto.Command{{Name: "x", Description: "d", ArgumentHint: "h"}}) {
		t.Fatalf("a = %v, %v", got, err)
	}
}
