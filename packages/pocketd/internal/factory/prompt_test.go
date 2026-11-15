package factory

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTheSystemPromptIsTheRulesThenTheSoulThenTheInstructions(t *testing.T) {
	text := SystemPrompt(goBackend())
	rules, soul, how := strings.Index(text, "<anywhere_factory_rules>"), strings.Index(text, "You are careful."), strings.Index(text, "Write the test first.")
	if rules != 0 || soul < strings.Index(text, "</anywhere_factory_rules>") || how < soul {
		t.Fatalf("rules at %d, soul at %d, instructions at %d:\n%s", rules, soul, how, text)
	}
}

func TestABuiltInProfilesSystemPromptIsTheRulesAlone(t *testing.T) {
	if text := SystemPrompt(BuiltIns()[0]); text != Rules+"\n" {
		t.Fatalf("%q", text)
	}
}

func TestAPromptFileIsNamedByItsTextAndNeverRewritten(t *testing.T) {
	home := t.TempDir()
	one, err := WritePrompt(home, "rules\n\nsoul\n")
	if err != nil || filepath.Dir(one) != filepath.Join(home, "state", "factory", "prompts") || !strings.HasSuffix(one, ".md") {
		t.Fatalf("%s %v", one, err)
	}
	os.Chmod(one, 0o400)
	again, err := WritePrompt(home, "rules\n\nsoul\n")
	other, _ := WritePrompt(home, "rules\n\nanother soul\n")
	if err != nil || again != one || other == one {
		t.Fatalf("%s, %s, %s: %v", one, again, other, err)
	}
	if text, _ := os.ReadFile(one); string(text) != "rules\n\nsoul\n" {
		t.Fatalf("%q", text)
	}
}

func TestAnEditedProfileWritesANewFileAndLeavesTheOldOne(t *testing.T) {
	home, doc := t.TempDir(), goBackend()
	before, _ := WritePrompt(home, SystemPrompt(doc))
	doc.Soul = "You are bold."
	after, _ := WritePrompt(home, SystemPrompt(doc))
	old, _ := os.ReadFile(before)
	if before == after || !strings.Contains(string(old), "You are careful.") {
		t.Fatalf("%s %s:\n%s", before, after, old)
	}
}

func TestPreparingAProfileWritesItsPromptAndCopiesItsSkillsIntoTheCheckout(t *testing.T) {
	home, dir := t.TempDir(), gitRepo(t)
	e := &Engine{Home: home, Profiles: OpenProfiles(home)}
	e.Profiles.Save(goBackend())
	own := filepath.Join(dir, ".claude", "skills", "test")
	os.MkdirAll(own, 0o755)
	system, skipped, err := e.Prepare("go-backend", "claude", dir)
	text, _ := os.ReadFile(system)
	if err != nil || !strings.HasPrefix(string(text), Rules) || !strings.Contains(string(text), "You are careful.") {
		t.Fatalf("%s: %v\n%s", system, err, text)
	}
	if _, err := os.Stat(filepath.Join(dir, ".claude", "skills", "review", "SKILL.md")); err != nil || len(skipped) != 1 || skipped[0] != "test" {
		t.Fatalf("skipped %q, %v", skipped, err)
	}
}

func TestPreparingRefusesAProfileThatIsGoneOrRunsAnotherAgent(t *testing.T) {
	home := t.TempDir()
	e := &Engine{Home: home, Profiles: OpenProfiles(home)}
	if _, _, err := e.Prepare("gone", "claude", t.TempDir()); !errors.Is(err, ErrUnknownProfile) {
		t.Fatalf("gone: %v", err)
	}
	if _, _, err := e.Prepare("codex", "claude", t.TempDir()); err == nil {
		t.Fatal("a codex profile prepared a claude session")
	}
	var off *Engine
	if _, _, err := off.Prepare("codex", "codex", t.TempDir()); !errors.Is(err, ErrOff) {
		t.Fatalf("no engine: %v", err)
	}
}
