package state

import (
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

// Outcome is the one notice a restored Agent shows: failed, then interrupted
// (a turn that was running no longer is), then access_lowered (full resumes
// as ask, but for an agent that has no other access), then resumed.
func Outcome(saved Terminal, status string, failed bool) string {
	running := func(s string) bool { return s == "working" || s == "needsYou" }
	switch {
	case failed:
		return proto.RestoreFailed
	case running(saved.Status) && !running(status):
		return proto.RestoreInterrupted
	case saved.Launch != nil && saved.Launch.Access == "full" && !providers.NeverAsks(saved.Provider):
		return proto.RestoreAccessLowered
	}
	return proto.RestoreResumed
}
