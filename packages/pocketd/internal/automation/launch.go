package automation

import (
	"cmp"

	"pocketd/internal/proto"
)

// Spec is the Session run starts: the folder as it is, or a fresh Worktree
// pocketd names from the prompt, on the automation's access, else the agent's
// own permission settings.
func Spec(a proto.Automation, run string) proto.LaunchSpec {
	checkout := proto.Checkout{Worktree: a.Folder}
	if a.NewWorktree {
		// The name holds only until pocketd names the branch from the prompt; the run's ID keeps it free.
		checkout = proto.Checkout{New: &proto.NewWorktree{Name: "automation-" + run[:min(8, len(run))], AutoName: true}}
	}
	return proto.LaunchSpec{
		Project:  a.Folder,
		Checkout: checkout,
		Provider: a.Provider,
		Access:   cmp.Or(a.Access, "settings"),
		Prompt:   a.Prompt,
	}
}
