package state

import (
	"testing"

	"pocketd/internal/proto"
)

func TestFailedBeatsInterruptedBeatsAccessLoweredBeatsResumed(t *testing.T) {
	full := &Launch{Access: "full"}
	for _, c := range []struct {
		saved  Terminal
		status string
		failed bool
		want   string
	}{
		{Terminal{Status: "working", Launch: full}, "idle", true, proto.RestoreFailed},
		{Terminal{Status: "working", Launch: full}, "idle", false, proto.RestoreInterrupted},
		{Terminal{Status: "idle", Launch: full}, "idle", false, proto.RestoreAccessLowered},
		{Terminal{Status: "idle", Launch: &Launch{Access: "edits"}}, "idle", false, proto.RestoreResumed},
		{Terminal{Status: "done"}, "done", false, proto.RestoreResumed},
	} {
		if got := Outcome(c.saved, c.status, c.failed); got != c.want {
			t.Errorf("Outcome(%+v, %q, %v) = %q, want %q", c.saved, c.status, c.failed, got, c.want)
		}
	}
}

func TestAWorkingAgentIsInterrupted(t *testing.T) {
	for _, saved := range []string{"working", "needsYou"} {
		if got := Outcome(Terminal{Status: saved}, "idle", false); got != proto.RestoreInterrupted {
			t.Errorf("saved %s: %q", saved, got)
		}
	}
}

func TestAStillRunningTurnIsNotInterrupted(t *testing.T) {
	if got := Outcome(Terminal{Status: "working"}, "working", false); got != proto.RestoreResumed {
		t.Fatalf("Outcome = %q", got)
	}
}
