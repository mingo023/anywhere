package launch

import "pocketd/internal/proto"

var rank = map[string]int{"ask": 0, "edits": 1, "auto": 2, "full": 3}

// Check is what a non-owner may ask for; the owner may ask for anything
// Argv accepts. maxAccess is the owner's phone.maxAccess.
func Check(w Who, s proto.LaunchSpec, maxAccess string) *Failure {
	if w.Owner {
		return nil
	}
	if n := s.Checkout.New; n != nil && (n.Base != "" || n.Branch != "" || n.PR != "" || n.Copy != nil || n.Setup != nil) {
		return fail("access_not_allowed", "Only your Mac picks the base, branch, PR, copy or setup")
	}
	if s.Access == "full" {
		return fail("access_not_allowed", "Full access starts only from your Mac")
	}
	if rank[s.Access] > rank[maxAccess] {
		return fail("access_not_allowed", "On your Mac: ⌘K → Phone access level")
	}
	return nil
}
