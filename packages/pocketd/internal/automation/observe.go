package automation

import (
	"strings"

	"pocketd/internal/proto"
)

const (
	maxWhy     = 80
	maxSummary = 240
)

// Seen is what the agent of a run looks like now.
type Seen struct {
	Status  string // the agent's status: idle, working, needsYou or done
	Failed  bool
	Summary string // the last assistant text, read once the turn ended
	// Answered is whether the transcript holds an assistant reply to the
	// prompt. An agent the desktop is showing goes idle, not done, when a turn
	// ends, and a turn shorter than a poll is never seen working.
	Answered bool
}

// finished is a status a run never leaves.
func finished(status string) bool {
	switch status {
	case "succeeded", "failed", "cancelled", "skipped":
		return true
	}
	return false
}

// Step is r after looking at its agent, and whether that changed r. worked is
// whether the agent has been seen working: a fresh agent is idle before its
// prompt starts a turn, and Done means a turn already ended, as does a reply
// in the transcript. A finished run stays as it is.
func Step(r proto.Run, seen Seen, present, worked bool) (proto.Run, bool) {
	if finished(r.Status) {
		return r, false
	}
	next := r
	switch {
	case !present || seen.Status == "closed":
		next.Status, next.Why = "cancelled", "Session closed"
	case seen.Status == "working":
		next.Status, next.Why = "running", ""
	case seen.Status == "needsYou":
		next.Status, next.Why = "waiting", ""
	case seen.Status == "done" || seen.Status == "idle" && (worked || seen.Answered):
		next.Status, next.Why, next.Summary = "succeeded", "", seen.Summary
		if seen.Failed {
			next.Status, next.Why = "failed", "The agent failed"
		}
	}
	return next, next != r
}

// plain is text on one line, cut to n runes with an ellipsis.
func plain(text string, n int) string {
	line := strings.Join(strings.Fields(text), " ")
	if r := []rune(line); len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return line
}
