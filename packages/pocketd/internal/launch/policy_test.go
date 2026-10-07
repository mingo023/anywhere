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
	for _, c := range []struct {
		name      string
		w         Who
		s         proto.LaunchSpec
		maxAccess string
		want      string
	}{
		{"owner full", owner, spec("claude", "full", false), "ask", ""},
		{"owner settings", owner, spec("claude", "settings", false), "ask", ""},
		{"owner base", owner, newTree(proto.NewWorktree{Name: "n", Base: "dev"}), "ask", ""},
		{"phone ask", phone, spec("claude", "ask", false), "ask", ""},
		{"phone edits under auto", phone, spec("codex", "edits", false), "auto", ""},
		{"phone auto under auto", phone, spec("claude", "auto", false), "auto", ""},
		{"phone plan", phone, spec("claude", "ask", true), "ask", ""},
		{"phone new worktree", phone, newTree(proto.NewWorktree{Name: "n"}), "ask", ""},
		{"phone edits above ask", phone, spec("claude", "edits", false), "ask", "On your Mac: ⌘K → Phone access level"},
		{"phone auto above edits", phone, spec("claude", "auto", false), "edits", "On your Mac: ⌘K → Phone access level"},
		{"phone full", phone, spec("claude", "full", false), "auto", "Full access starts only from your Mac"},
		{"phone settings", phone, spec("codex", "settings", false), "auto", "Only your Mac starts agents with their own settings"},
		{"phone base", phone, newTree(proto.NewWorktree{Name: "n", Base: "dev"}), "ask", "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone branch", phone, newTree(proto.NewWorktree{Branch: "fix/login"}), "ask", "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone pr", phone, newTree(proto.NewWorktree{PR: "123"}), "ask", "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone copy", phone, newTree(proto.NewWorktree{Name: "n", Copy: &no}), "ask", "Only your Mac picks the base, branch, PR, copy or setup"},
		{"phone setup", phone, newTree(proto.NewWorktree{Name: "n", Setup: &no}), "ask", "Only your Mac picks the base, branch, PR, copy or setup"},
	} {
		f := Check(c.w, c.s, c.maxAccess)
		switch {
		case c.want == "" && f != nil:
			t.Errorf("%s: refused %+v", c.name, f)
		case c.want != "" && (f == nil || f.Code != "access_not_allowed" || f.Message != c.want):
			t.Errorf("%s: got %+v, want %q", c.name, f, c.want)
		}
	}
}
