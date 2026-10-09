package automation

import (
	"testing"

	"pocketd/internal/proto"
)

func TestAnAutomationWithoutAccessOrWorktreeRunsInItsFolderOnTheAgentsSettings(t *testing.T) {
	s := Spec(proto.Automation{Folder: "/r", Provider: "claude", Prompt: "p"}, "abcdef12-3456")
	if s.Checkout.Worktree != "/r" || s.Checkout.New != nil || s.Access != "settings" {
		t.Fatalf("%+v", s)
	}
}

func TestAnAutomationInANewWorktreeStartsEachRunInOneNamedFromThePrompt(t *testing.T) {
	s := Spec(proto.Automation{Folder: "/r", Provider: "codex", Prompt: "p", Access: "edits", NewWorktree: true}, "abcdef12-3456")
	if s.Checkout.Worktree != "" || s.Checkout.New == nil || s.Checkout.New.Name != "automation-abcdef12" || !s.Checkout.New.AutoName || s.Access != "edits" {
		t.Fatalf("%+v %+v", s, s.Checkout.New)
	}
}
