package team

import "pocketd/internal/proto"

// Why a restart failed a task or a team, or stopped a merge.
const (
	WorkerRestarted = "the worker stopped when pocketd restarted"
	LeadRestarted   = "the lead stopped when pocketd restarted"
	MergeRestarted  = "pocketd restarted during the merge"
)

// Worker is what a restart left of a task's worker.
type Worker struct {
	// Status is its status now; "" when it didn't come back.
	Status string
	// Turn names its last turn by its prompt's id (TurnID).
	Turn string
	// Ended is whether its last turn has ended; OK whether that turn ended normally.
	Ended, OK bool
}

// Facts is what a restart left of a team's lead, its workers in a turn (by task id) and the lead's checkout.
type Facts struct {
	// Lead is the lead's status; "" when it didn't come back.
	Lead    string
	Workers map[string]Worker
	// Merging is whether a merge is in progress in the team's checkout.
	Merging bool
}

// Recovery is what is left to do once a recovered team is saved.
type Recovery struct {
	// HandBack lists tasks whose worker finished; Failed those failed here, for the lead to hear.
	HandBack, Failed []string
	// Abort aborts the merge in the team's checkout; Reread reads the planning lead's last turn again.
	Abort, Reread bool
}

// Recovered is t once a restart left f, what is left to do, and whether t changed.
func Recovered(t proto.Team, f Facts) (proto.Team, Recovery, bool) {
	var r Recovery
	switch t.State {
	case "planning":
		switch f.Lead {
		case "":
			t.State, t.Error = "failed", LeadRestarted
			return t, r, true
		case "idle", "done":
			r.Reread = true
		}
		return t, r, false
	case "merging":
		if !f.Merging {
			return MergeFailed(t, MergeRestarted), r, true
		}
		t = Conflicted(t, nil)
		t.Error, r.Abort = MergeRestarted, true
		return t, r, true
	case "running", "review":
	default:
		return t, r, false
	}
	changed := false
	for _, task := range t.Tasks {
		if !inTurn(task.State) {
			continue
		}
		w, edited := f.Workers[task.ID], false
		switch {
		case w.Status == "working" || w.Status == "needsYou":
			t, edited = WorkerStatus(t, task.Worker, w.Status)
		case w.Ended && w.OK:
			if w.Turn == "" || w.Turn != task.Turn {
				r.HandBack = append(r.HandBack, task.ID)
			}
		case w.Ended:
			t, edited = Failed(t, task.ID, "The worker's turn failed")
			r.Failed = append(r.Failed, task.ID)
		default:
			t, edited = Failed(t, task.ID, WorkerRestarted)
			r.Failed = append(r.Failed, task.ID)
		}
		changed = changed || edited
	}
	return t, r, changed
}
