package launch

import (
	"testing"

	"pocketd/internal/proto"
)

func TestThePhoneMayStartOnlyWhatTheOwnerAllows(t *testing.T) {
	owner, phone := Who{Owner: true, Key: "owner"}, Who{Key: "device:d1"}
	no := false
	newTree := func(n proto.NewWorktree) proto.LaunchSpec {
		s := spec("claude", "ask", false)
		s.Checkout = proto.Checkout{New: &n}
		return s
	}
	under := func(parent string) proto.LaunchSpec {
		s := spec("claude", "ask", false)
		s.Parent = parent
		return s
	}
	for _, c := range []struct {
		name string
		w    Who
		s    proto.LaunchSpec
		want string
	}{
		{"owner full", owner, spec("claude", "full", false), ""},
		{"owner settings", owner, spec("claude", "settings", false), ""},
		{"owner base", owner, newTree(proto.NewWorktree{Name: "n", Base: "dev"}), ""},
		{"phone ask", phone, spec("claude", "ask", false), ""},
		{"phone edits", phone, spec("codex", "edits", false), ""},
		{"phone auto", phone, spec("claude", "auto", false), ""},
		{"phone plan", phone, spec("claude", "ask", true), ""},
		{"phone new worktree", phone, newTree(proto.NewWorktree{Name: "n"}), ""},
		{"phone full", phone, spec("claude", "full", false), ""},
		{"phone settings", phone, spec("codex", "settings", false), ""},
		{"phone base", phone, newTree(proto.NewWorktree{Name: "n", Base: "dev"}), "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone branch", phone, newTree(proto.NewWorktree{Branch: "fix/login"}), "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone pr", phone, newTree(proto.NewWorktree{PR: "123"}), "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone copy", phone, newTree(proto.NewWorktree{Name: "n", Copy: &no}), "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone setup", phone, newTree(proto.NewWorktree{Name: "n", Setup: &no}), "Only your Mac picks the base, branch, PR, copy or setup"},
		{"owner parent", owner, under("a1"), ""},
		{"phone parent", phone, under("a1"), "Only your Mac starts a session under another"},
		{"owner pi ask", owner, spec("pi", "ask", false), ""},
		{"phone pi full", phone, spec("pi", "full", false), ""},
		{"phone pi ask", phone, spec("pi", "ask", false), "Pi runs every tool without asking. Start it with Full access."},
		{"phone omp settings", phone, spec("omp", "settings", false), "omp runs every tool without asking. Start it with Full access."},
	} {
		f := Check(c.w, c.s)
		switch {
		case c.want == "" && f != nil:
			t.Errorf("%s: refused %+v", c.name, f)
		case c.want != "" && (f == nil || f.Code != "access_not_allowed" || f.Message != c.want):
			t.Errorf("%s: got %+v, want %q", c.name, f, c.want)
		}
	}
}

func TestAPromptedAgentWhoseStatusShowsOnlyInItsChatAlwaysStartsAsOne(t *testing.T) {
	for _, c := range []struct {
		provider, prompt string
		chat, want       bool
	}{
		{"hermes", "fix it", false, true},
		{"pi", "fix it", false, true},
		{"hermes", "", false, false},
		{"claude", "fix it", false, false},
		{"codex", "fix it", true, true},
	} {
		s := spec(c.provider, "ask", false)
		s.Prompt, s.Chat = c.prompt, c.chat
		if got := Chats(s); got != c.want {
			t.Errorf("%s prompt %q chat %v: Chats = %v, want %v", c.provider, c.prompt, c.chat, got, c.want)
		}
	}
}
