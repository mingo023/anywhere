package team

import (
	"cmp"
	"errors"
	"fmt"
	"log"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
	"pocketd/internal/worktree"
)

// The transcript a reply is read from can lag the turn's end.
var (
	textTries = 10
	textWait  = 300 * time.Millisecond
)

var errNoTeam = errors.New("This session has no team")

// TurnEnded is agent.Registry's OnTurnEnd. It runs under the agent's lock, so it only queues.
func (t *Teams) TurnEnded(id string, ok bool) {
	if t != nil {
		t.post(event{kind: "turn", agent: id, ok: ok})
	}
}

// work is what e means for the task whose worker did it, if any.
func (t *Teams) work(e event) {
	teams, err := t.Store.List()
	if err != nil {
		log.Printf("teams: %v", err)
		return
	}
	team, task, ok := Find(teams, e.agent)
	switch {
	case !ok:
	case e.kind == "told":
		t.tell(team.Lead, Note(task.ID, e.text))
	case e.kind == "turn" && e.ok:
		t.handback(team, task)
	case e.kind == "turn":
		t.fail(team.Lead, task.ID, "The worker's turn failed")
	case e.to == "closed":
		t.fail(team.Lead, task.ID, "The worker's session closed")
	default:
		t.change(team.Lead, func(tm proto.Team) (proto.Team, bool) { return WorkerStatus(tm, e.agent, e.to) })
	}
}

// review reads the verdict in a turn end of a lead whose team is in review;
// false when id leads no team in review.
func (t *Teams) review(id string) bool {
	team, ok, err := t.Store.Get(id)
	if err != nil || !ok || team.State != "review" {
		return false
	}
	_, text := t.lastText(id, "")
	t.change(id, func(tm proto.Team) (proto.Team, bool) { return Verdict(tm, text) })
	return true
}

// Start applies the user's picks to lead's proposed plan and starts what can start.
func (t *Teams) Start(lead string, picks []proto.TaskPick) error {
	if t == nil {
		return errOff
	}
	if _, err := t.leadCwd(lead); err != nil {
		return err
	}
	err := errNoTeam
	t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Started(tm, picks)
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	t.pump(lead)
	return nil
}

// pump starts every task lead's team can start now, each on the first free
// name for its branch.
func (t *Teams) pump(lead string) {
	taken := t.taken(lead)
	var starting []proto.TeamTask
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		if tm.State != "running" {
			return tm, false
		}
		for _, task := range Startable(tm) {
			tm = Starting(tm, task.ID, taken)
			starting = append(starting, tm.Tasks[taskAt(tm, task.ID)])
		}
		return tm, len(starting) > 0
	})
	for _, task := range starting {
		go t.launch(team, task)
	}
}

// taken is every name a worker's branch can't take in lead's project: its
// branches, its remotes' and the folders in its worktrees folder. None when
// the project can't be read; the launch then fails saying why.
func (t *Teams) taken(lead string) []string {
	cwd, err := t.leadCwd(lead)
	if err != nil {
		return nil
	}
	f := t.Projects.Load()
	project, ok := worktree.Project(f, cwd)
	if !ok {
		return nil
	}
	taken, err := worktree.Taken(project, worktree.Dir(f, project))
	if err != nil {
		log.Printf("teams: %s: %v", lead, err)
	}
	return taken
}

// launch starts task's worker, records it, names it, and only then sends its
// assignment, so the turn that answers it finds the worker on record. team is
// the team as task started, for the branches it starts from.
func (t *Teams) launch(team proto.Team, task proto.TeamTask) {
	lead := team.Lead
	spec, err := t.spec(team, task)
	if err != nil {
		t.fail(lead, task.ID, err.Error())
		return
	}
	id, cwd, err := t.Launch(lead, task.ID, spec)
	if err != nil {
		t.fail(lead, task.ID, err.Error())
		return
	}
	launched, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) { return Launched(tm, task.ID, id, cwd), true })
	if !Assigns(launched, task.ID) {
		return
	}
	a, err := t.Agents.Get(id)
	if err == nil {
		if claim, ok := a.ClaimNaming(); ok {
			a.SetNamed(claim, task.Title)
		}
		err = a.Driver().Prompt(Assignment(task))
	}
	if err != nil {
		t.fail(lead, task.ID, err.Error())
	}
}

// spec is how task's worker launches, once its -base branch is made when it needs one.
func (t *Teams) spec(team proto.Team, task proto.TeamTask) (proto.LaunchSpec, error) {
	cwd, err := t.leadCwd(team.Lead)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	project, ok := worktree.Project(t.Projects.Load(), cwd)
	if !ok {
		return proto.LaunchSpec{}, errors.New("The lead's folder isn't in a project on this Mac")
	}
	head, err := worktree.Head(cwd)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	base := Base(team, task, head)
	if len(task.After) > 1 {
		if err := worktree.MergeBranches(project, base, Branches(team, task.After)); err != nil {
			return proto.LaunchSpec{}, err
		}
	}
	return WorkerSpec(project, team.Lead, task, base), nil
}

// handback commits what task's worker changed and hands it to the lead, once
// per turn. It runs only on Run's goroutine, so two handbacks never commit at once.
func (t *Teams) handback(team proto.Team, task proto.TeamTask) {
	turn, summary := t.lastText(task.Worker, task.Turn)
	if turn != "" && turn == task.Turn {
		return
	}
	stat, err := t.commit(team, task)
	if err != nil {
		t.fail(team.Lead, task.ID, err.Error())
		return
	}
	if _, ok := t.change(team.Lead, func(tm proto.Team) (proto.Team, bool) { return Handed(tm, task.ID, turn, summary, stat) }); !ok {
		return
	}
	t.tell(team.Lead, Handback(task, summary, stat))
	t.pump(team.Lead)
}

// commit commits everything in task's worktree and says what its branch changed.
func (t *Teams) commit(team proto.Team, task proto.TeamTask) (string, error) {
	cwd, err := t.leadCwd(team.Lead)
	if err != nil {
		return "", err
	}
	head, err := worktree.Head(cwd)
	if err != nil {
		return "", err
	}
	if _, err := worktree.Commit(task.Worktree, fmt.Sprintf("team %s: %s", task.ID, task.Title)); err != nil {
		return "", err
	}
	return worktree.Stat(task.Worktree, Base(team, task, head))
}

// fail fails a running task, tells the lead, and starts what its slot frees.
func (t *Teams) fail(lead, id, why string) {
	if _, ok := t.change(lead, func(tm proto.Team) (proto.Team, bool) { return Failed(tm, id, why) }); !ok {
		return
	}
	t.tell(lead, FailedHandback(id, why))
	t.pump(lead)
}

// tell mails text to agentID's chat.
func (t *Teams) tell(agentID, text string) {
	if err := t.Mail(agentID, text); err != nil {
		log.Printf("teams: mail to %s: %v", agentID, err)
	}
}

// lastText is id's last turn, by its prompt's id, and its reply, read again
// while the transcript catches up: until the turn has a reply and isn't before,
// as a turn whose prompt hasn't reached the transcript reads as the one before.
func (t *Teams) lastText(id, before string) (turn, text string) {
	for try := 1; ; try++ {
		a, err := t.Agents.Get(id)
		if err != nil {
			return "", ""
		}
		items, _ := a.Timeline.Page(0, turnItems)
		turn := TurnID(items)
		if text, ok := LastText(items); ok && (turn == "" || turn != before) || try == textTries {
			return turn, text
		}
		time.Sleep(textWait)
	}
}

func (t *Teams) leadCwd(lead string) (string, error) {
	a, err := t.Agents.Get(lead)
	if err != nil {
		return "", errors.New("The lead's session is closed")
	}
	return a.Summary().Cwd, nil
}

// Told is a prompt the user sent agentID from the chat; one sent to a worker is noted to its lead.
func (t *Teams) Told(agentID, text string) {
	if t != nil {
		t.post(event{kind: "told", agent: agentID, text: text})
	}
}

// Stop stops task id of lead's team and its worker's turn.
func (t *Teams) Stop(lead, id string) error {
	if t == nil {
		return errOff
	}
	err := errNoTeam
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Stopped(tm, id)
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	if a, err := t.Agents.Get(team.Tasks[taskAt(team, id)].Worker); err == nil {
		a.Driver().Interrupt()
	}
	return nil
}

// Merge merges lead's team into the lead's checkout in the background, once
// the checkout has no uncommitted changes.
func (t *Teams) Merge(lead string) error {
	if t == nil {
		return errOff
	}
	cwd, err := t.leadCwd(lead)
	if err != nil {
		return err
	}
	if dirty, err := worktree.Dirty(cwd); err != nil || dirty {
		return cmp.Or(err, worktree.ErrDirty)
	}
	err = errNoTeam
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Merging(tm)
		tm.Checkout = cwd
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	go t.merge(lead, cwd, team)
	return nil
}

// merge merges team's done tasks into cwd in order. A full merge cleans up
// before the team shows merged; a conflict is mailed to the lead.
func (t *Teams) merge(lead, cwd string, team proto.Team) {
	tasks := MergeOrder(team)
	branches := make([]string, len(tasks))
	for i, task := range tasks {
		branches[i] = task.Branch
	}
	done, err := worktree.Merge(cwd, branches)
	merged := make([]string, len(done))
	for i := range done {
		merged[i] = tasks[i].ID
	}
	conflict, _ := errors.AsType[*worktree.Conflict](err)
	if err == nil {
		t.cleanUp(cwd, team.Tasks)
	}
	t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm = Merged(tm, merged)
		switch {
		case conflict != nil:
			tm = Conflicted(tm, conflict.Files)
		case err != nil:
			tm = MergeFailed(tm, err.Error())
		}
		return tm, true
	})
	if conflict != nil {
		t.tell(lead, ConflictNote(conflict.Branch, conflict.Files))
	}
}

// cleanUp closes the workers of tasks and removes their worktrees; their branches stay.
func (t *Teams) cleanUp(cwd string, tasks []proto.TeamTask) {
	f := t.Projects.Load()
	project, ok := worktree.Project(f, cwd)
	if !ok {
		return
	}
	for _, task := range tasks {
		if a, err := t.Agents.Get(task.Worker); err == nil {
			a.Driver().Close()
		}
		if task.Worktree == "" {
			continue
		}
		if err := worktree.Remove(f, project, task.Worktree); err != nil {
			log.Printf("teams: removing %s: %v", task.Worktree, err)
		}
	}
}

// Allow is broker.Broker's Allow: it lets a worker run a shell command its
// running team's plan allows, and notes that in the worker's timeline.
func (t *Teams) Allow(req proto.PermissionRequest) bool {
	if t == nil {
		return false
	}
	teams, err := t.Store.List()
	if err != nil || !Allows(teams, req.AgentID, req.ToolName, req.Detail.Command) {
		return false
	}
	if a, err := t.Agents.Get(req.AgentID); err == nil {
		a.Record(timeline.Event{Kind: "notice", Text: "Allowed by the team plan: " + req.Detail.Command})
	}
	return true
}
