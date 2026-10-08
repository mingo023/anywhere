package automation

import "pocketd/internal/proto"

// Spec is the Session a run starts: the folder as it is, on the agent's own
// permission settings.
func Spec(a proto.Automation) proto.LaunchSpec {
	return proto.LaunchSpec{
		Project:  a.Folder,
		Checkout: proto.Checkout{Worktree: a.Folder},
		Provider: a.Provider,
		Access:   "settings",
		Prompt:   a.Prompt,
	}
}
