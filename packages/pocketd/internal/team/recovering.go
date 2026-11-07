package team

import (
	"cmp"
	"log"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// Recover picks up every team where pocketd's restart left it (Recovered). Call it once the restored sessions
// have come back or failed (daemon.WaitRestored), so a worker still resuming isn't taken for gone.
func (t *Teams) Recover() {
	if t == nil {
		return
	}
	teams, err := t.Store.List()
	if err != nil {
		log.Printf("teams: %v", err)
		return
	}
	for _, team := range teams {
		t.pickUp(team)
	}
}

// pickUp applies Recovered to team, then does what is left.
func (t *Teams) pickUp(team proto.Team) {
	lead, f := team.Lead, t.facts(team)
	var r Recovery
	next, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		var changed bool
		tm, r, changed = Recovered(tm, f)
		return tm, changed
	})
	if r.Abort {
		if err := worktree.AbortMerge(next.Checkout); err != nil {
			log.Printf("teams: %s: aborting the merge: %v", lead, err)
		}
	}
	for _, id := range r.Failed {
		t.tell(lead, FailedHandback(id, next.Tasks[taskAt(next, id)].Summary))
	}
	for _, id := range r.HandBack {
		t.post(event{kind: "turn", agent: next.Tasks[taskAt(next, id)].Worker, ok: true})
	}
	if r.Reread {
		t.Ended(lead)
	}
	t.awaits(lead, next.State)
	t.pump(lead)
}

// facts is what the restart left of team's lead, its workers in a turn, and a merging lead's checkout.
func (t *Teams) facts(team proto.Team) Facts {
	f := Facts{Lead: t.back(team.Lead), Workers: map[string]Worker{}}
	for _, task := range team.Tasks {
		if !inTurn(task.State) {
			continue
		}
		w := Worker{Status: t.back(task.Worker)}
		if w.Status != "" && w.Status != "working" && w.Status != "needsYou" {
			w.Turn, w.Ended, w.OK = t.lastTurn(task.Worker)
		}
		f.Workers[task.ID] = w
	}
	if team.State == "merging" && team.Checkout != "" {
		var err error
		if f.Merging, err = worktree.MergeInProgress(team.Checkout); err != nil {
			log.Printf("teams: %s: %v", team.Lead, err)
		}
	}
	return f
}

// back is id's status, or "" when it didn't come back from the restart.
func (t *Teams) back(id string) string {
	if id == "" {
		return ""
	}
	a, err := t.Agents.Get(id)
	if err != nil {
		return ""
	}
	s := a.Summary()
	if s.Status == "closed" || s.Restore == proto.RestoreFailed {
		return ""
	}
	return s.Status
}

// lastTurn is id's last turn by its prompt's id, whether it has ended (looking again while the transcript catches
// up), and whether it ended normally.
func (t *Teams) lastTurn(id string) (turn string, ended, ok bool) {
	a, err := t.Agents.Get(id)
	if err != nil {
		return "", false, false
	}
	for i := range reads {
		if i > 0 {
			time.Sleep(cmp.Or(t.Poll, time.Second))
		}
		items, _ := a.Timeline.Page(0, turnItems)
		if _, done, failed := Reply(items); done {
			return TurnID(items), true, !failed
		}
	}
	return "", false, false
}
