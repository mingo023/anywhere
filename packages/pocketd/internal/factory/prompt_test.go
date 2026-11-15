package factory

import (
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
