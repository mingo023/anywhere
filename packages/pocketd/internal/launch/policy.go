package launch

import (
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

// Access is the access s's agent runs with, which is what the Session
// records: full for an agent that never asks before it runs a tool, whatever
// s asks for.
func Access(s proto.LaunchSpec) string {
	if providers.NeverAsks(s.Provider) {
		return "full"
	}
	return s.Access
}

// Chats is whether s runs for the chat: as asked, or always when it carries a
// prompt for an agent whose status shows only in its chat.
func Chats(s proto.LaunchSpec) bool {
	return s.Chat || s.Prompt != "" && providers.Get(s.Provider).OnlyChats()
}

// Check is what a non-owner may ask for; the owner may ask for anything
// Argv accepts. A phone picks any access, as the Mac does, but must have
// picked full for an agent that has no other.
func Check(w Who, s proto.LaunchSpec) *Failure {
	if w.Owner {
		return nil
	}
	if providers.NeverAsks(s.Provider) && s.Access != "full" {
		return fail("access_not_allowed", providers.Get(s.Provider).Name+" runs every tool without asking. Start it with Full access.")
	}
	if s.Parent != "" {
		return fail("access_not_allowed", "Only your Mac starts a session under another")
	}
	if n := s.Checkout.New; n != nil && (n.Base != "" || n.Branch != "" || n.PR != "" || n.Copy != nil || n.Setup != nil) {
		return fail("access_not_allowed", "Only your Mac picks the base, branch, PR, copy or setup")
	}
	return nil
}
