package launch

import "pocketd/internal/proto"

// Check is what a non-owner may ask for; the owner may ask for anything
// Argv accepts. A phone picks any access, as the Mac does.
func Check(w Who, s proto.LaunchSpec) *Failure {
	if w.Owner {
		return nil
	}
	if s.Parent != "" {
		return fail("access_not_allowed", "Only your Mac starts a session under another")
	}
	if n := s.Checkout.New; n != nil && (n.Base != "" || n.Branch != "" || n.PR != "" || n.Copy != nil || n.Setup != nil) {
		return fail("access_not_allowed", "Only your Mac picks the base, branch, PR, copy or setup")
	}
	return nil
}
