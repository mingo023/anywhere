package team

import (
	"cmp"
	"errors"
	"fmt"
	"slices"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// Started applies the user's edits at Start to a proposed team: the picked
// tasks stay, in plan order, with their agent, model and allowlist; the rest
// go. A kept task's after must all be kept, and the kept tasks must still make
// a valid plan, though one task is enough. Each waits to start.
func Started(t proto.Team, picks []proto.TaskPick) (proto.Team, error) {
	if t.State != "proposed" {
		return t, errors.New("This team's plan isn't waiting to start")
	}
	kept := map[string]proto.TaskPick{}
	for _, p := range picks {
		if _, twice := kept[p.ID]; twice {
			return t, fmt.Errorf("Task %s is listed twice", p.ID)
		}
		if taskAt(t, p.ID) < 0 {
			return t, fmt.Errorf("The plan has no task %s", p.ID)
		}
		kept[p.ID] = p
	}
	for _, x := range t.Tasks {
		if _, ok := kept[x.ID]; !ok {
			continue
		}
		for _, a := range x.After {
			if _, ok := kept[a]; !ok {
				return t, fmt.Errorf("Task %s starts from %s's work; keep %s or remove %s too", x.ID, a, a, x.ID)
			}
		}
	}
	tasks := []proto.TeamTask{}
	for _, x := range t.Tasks {
		p, ok := kept[x.ID]
		if !ok {
			continue
		}
		x.Agent, x.Model, x.Allow, x.State = p.Agent, p.Model, p.Allow, "waiting"
		if x.Allow == nil {
			x.Allow = []string{}
		}
		tasks = append(tasks, x)
	}
	if err := ValidateKept(tasks); err != nil {
		return t, err
	}
	t.Tasks, t.State, t.Error = tasks, "running", ""
	return t, nil
}

// Base is what task of t starts from: head (the lead's commit) without
// dependencies, its one dependency's branch, or for several its own branch's
// -base branch, which pocketd merges them into.
func Base(t proto.Team, task proto.TeamTask, head string) string {
	switch len(task.After) {
	case 0:
		return head
	case 1:
		return t.Tasks[taskAt(t, task.After[0])].Branch
	}
	return task.Branch + "-base"
}

// Branches are the branches t's tasks ids took.
func Branches(t proto.Team, ids []string) []string {
	out := make([]string, len(ids))
	for i, id := range ids {
		out[i] = t.Tasks[taskAt(t, id)].Branch
	}
	return out
}

// Free is the first of name, name-2, name-3, … that taken doesn't hold, nor
// its -base branch, read as worktree.Validate reads taken.
func Free(name string, taken []string) string {
	for n := 1; ; n++ {
		b := name
		if n > 1 {
			b = fmt.Sprintf("%s-%d", name, n)
		}
		if worktree.Validate(b, taken) != worktree.ErrExists && worktree.Validate(b+"-base", taken) != worktree.ErrExists {
			return b
		}
	}
}

// WorkerSpec launches task's worker: a chat under the lead, in a new worktree
// named for its branch, from base, accepting edits. It has no prompt: the
// assignment goes once the worker is on record.
func WorkerSpec(project, lead string, task proto.TeamTask, base string) proto.LaunchSpec {
	return proto.LaunchSpec{
		Project:  project,
		Checkout: proto.Checkout{New: &proto.NewWorktree{Name: task.Branch, Base: base}},
		Provider: task.Agent,
		Model:    task.Model,
		Access:   "edits",
		Chat:     true,
		Parent:   lead,
	}
}

// Assignment is a worker's first prompt.
func Assignment(task proto.TeamTask) string {
	return fmt.Sprintf(`<anywhere_team_assignment task=%q>
You're one worker in a team that a lead agent planned. Your task: %s.
%s
Change only these files: %s. Don't commit, and don't create branches or worktrees.
When you're done, reply with a short summary of what you changed and how you checked it.
</anywhere_team_assignment>`, task.ID, strings.TrimSuffix(task.Title, "."), task.Instructions, strings.Join(task.Files, ", "))
}

// Starting marks task id running, before its worker exists, on the first
// free name for its branch: not in taken (the project's branches and worktree
// folders, worktree.Taken) and no other task of t's.
func Starting(t proto.Team, id string, taken []string) proto.Team {
	names := slices.Clone(taken)
	for _, x := range t.Tasks {
		if x.Branch != "" {
			names = append(names, x.Branch)
		}
	}
	branch := Free(Branch(t.Lead, id), names)
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State, x.Branch = "running", branch })
	return t
}

// Launched records task id's worker and its folder.
func Launched(t proto.Team, id, worker, folder string) proto.Team {
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.Worker, x.Worktree = worker, folder })
	return t
}

// Assigns is whether task id's worker, just launched, still gets its
// assignment: not once the user stopped the task while it launched.
func Assigns(t proto.Team, id string) bool {
	i := taskAt(t, id)
	return i >= 0 && inTurn(t.Tasks[i].State)
}

// WorkerStatus shows a running worker's status on its task: working is
// running, needsYou is needsYou. Anything else changes nothing.
func WorkerStatus(t proto.Team, worker, status string) (proto.Team, bool) {
	state := map[string]string{"working": "running", "needsYou": "needsYou"}[status]
	i := slices.IndexFunc(t.Tasks, func(x proto.TeamTask) bool { return worker != "" && x.Worker == worker })
	if state == "" || i < 0 || !inTurn(t.Tasks[i].State) || t.Tasks[i].State == state {
		return t, false
	}
	return editTask(t, t.Tasks[i].ID, func(x *proto.TeamTask) { x.State = state })
}

// Failed fails task id while it runs, with why as its summary.
func Failed(t proto.Team, id, why string) (proto.Team, bool) {
	if i := taskAt(t, id); i < 0 || !inTurn(t.Tasks[i].State) {
		return t, false
	}
	return editTask(t, id, func(x *proto.TeamTask) { x.State, x.Summary = "failed", why })
}

// FailedHandback tells the lead task id failed, and why.
func FailedHandback(id, why string) string {
	return fmt.Sprintf("<anywhere_team_handback task=%q status=\"failed\">\n%s\n</anywhere_team_handback>", id, why)
}

// Find is the live team (running or in review) that worker works for, and its task.
func Find(teams []proto.Team, worker string) (proto.Team, proto.TeamTask, bool) {
	for _, t := range teams {
		if t.State != "running" && t.State != "review" {
			continue
		}
		for _, task := range t.Tasks {
			if worker != "" && task.Worker == worker {
				return t, task, true
			}
		}
	}
	return proto.Team{}, proto.TeamTask{}, false
}

// Allows is whether a running team's plan lets worker run command, asked as a
// shell tool (Claude's Bash, Codex's shell), without asking the user.
func Allows(teams []proto.Team, worker, tool, command string) bool {
	if tool != "Bash" && tool != "shell" {
		return false
	}
	t, task, ok := Find(teams, worker)
	return ok && t.State == "running" && Allowed(task.Allow, command)
}

func taskAt(t proto.Team, id string) int {
	return slices.IndexFunc(t.Tasks, func(x proto.TeamTask) bool { return x.ID == id })
}

// editTask is t with f applied to a copy of task id; false when t has none.
func editTask(t proto.Team, id string, f func(*proto.TeamTask)) (proto.Team, bool) {
	i := taskAt(t, id)
	if i < 0 {
		return t, false
	}
	t.Tasks = slices.Clone(t.Tasks)
	f(&t.Tasks[i])
	return t, true
}

// inTurn is whether a task in state has a worker at work.
func inTurn(state string) bool { return state == "running" || state == "needsYou" }

// LastText is the last assistant text in items, and whether it answers the
// last prompt; while the transcript lags a turn's end it doesn't.
func LastText(items []proto.Item) (string, bool) {
	for i := len(items) - 1; i >= 0; i-- {
		switch items[i].Kind {
		case "assistant":
			return items[i].Text, true
		case "user":
			return "", false
		}
	}
	return "", false
}

// TurnID names the last turn in items by its prompt's id; "" when items hold no prompt.
func TurnID(items []proto.Item) string {
	for i := len(items) - 1; i >= 0; i-- {
		if items[i].Kind == "user" && items[i].ParentToolUseID == "" {
			return items[i].ID
		}
	}
	return ""
}

// Handed records a worker's handback of turn: its task is done, with what it
// said and what it changed. The turn handed back last changes nothing again.
// Once every task is done, a running team is in review.
func Handed(t proto.Team, id, turn, summary, stat string) (proto.Team, bool) {
	if i := taskAt(t, id); i < 0 || t.Tasks[i].State == "merged" || turn != "" && t.Tasks[i].Turn == turn {
		return t, false
	}
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State, x.Summary, x.Stat, x.Turn = "done", summary, stat, turn })
	if t.State == "running" && !slices.ContainsFunc(t.Tasks, func(x proto.TeamTask) bool { return x.State != "done" }) {
		t.State = "review"
	}
	return t, true
}

// Handback is the mail that hands task's work to the lead.
func Handback(task proto.TeamTask, summary, stat string) string {
	return fmt.Sprintf("<anywhere_team_handback task=%q title=%q branch=%q>\nSummary from the worker:\n%s\n\ngit diff --stat:\n%s\n</anywhere_team_handback>",
		task.ID, task.Title, task.Branch, summary, cmp.Or(stat, "(no changes)"))
}

// Ready is whether text says "ready to merge", in any case, other than right
// after "not" or "n't".
func Ready(text string) bool {
	s := strings.ToLower(text)
	for {
		i := strings.Index(s, "ready to merge")
		if i < 0 {
			return false
		}
		before := strings.TrimSpace(s[:i])
		if !strings.HasSuffix(before, "not") && !strings.HasSuffix(before, "n't") && !strings.HasSuffix(before, "n’t") {
			return true
		}
		s = s[i+len("ready to merge"):]
	}
}

// Verdict reads the lead's last text at a turn end while the team is in review.
func Verdict(t proto.Team, text string) (proto.Team, bool) {
	ready := Ready(text)
	if t.State != "review" || t.Ready == ready {
		return t, false
	}
	t.Ready = ready
	return t, true
}

// Stopped stops task id while its worker is at work.
func Stopped(t proto.Team, id string) (proto.Team, error) {
	if i := taskAt(t, id); i < 0 || !inTurn(t.Tasks[i].State) {
		return t, fmt.Errorf("Task %s isn't running", id)
	}
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State = "stopped" })
	return t, nil
}

// Note tells the lead what the user told worker id directly.
func Note(id, text string) string {
	return fmt.Sprintf("<anywhere_team_note task=%q>The user told worker %s: %q</anywhere_team_note>", id, id, text)
}

// ConflictNote tells the lead the merge stopped on branch.
func ConflictNote(branch string, files []string) string {
	return fmt.Sprintf("<anywhere_team_note>Merging %s conflicted in: %s. The merge was aborted; nothing was merged after it.</anywhere_team_note>", branch, strings.Join(files, ", "))
}

// Merging starts a merge of a team in review, or again after a conflict.
func Merging(t proto.Team) (proto.Team, error) {
	if t.State != "review" && t.State != "conflict" {
		return t, errors.New("A team merges once every task has handed back")
	}
	t.State, t.Conflict, t.Error = "merging", nil, ""
	return t, nil
}

// MergeOrder is the tasks a merge takes: those done, in dependency order.
func MergeOrder(t proto.Team) []proto.TeamTask {
	return slices.DeleteFunc(slices.Clone(Order(t.Tasks)), func(x proto.TeamTask) bool { return x.State != "done" })
}

// Merged marks the tasks ids merged; the team is merged once every task is.
func Merged(t proto.Team, ids []string) proto.Team {
	for _, id := range ids {
		t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State = "merged" })
	}
	if !slices.ContainsFunc(t.Tasks, func(x proto.TeamTask) bool { return x.State != "merged" }) {
		t.State = "merged"
	}
	return t
}

// Conflicted ends a merge that stopped on a conflict in files.
func Conflicted(t proto.Team, files []string) proto.Team {
	t.State, t.Conflict = "conflict", files
	return t
}

// MergeFailed ends a merge git refused for another reason, back in review.
func MergeFailed(t proto.Team, why string) proto.Team {
	t.State, t.Error = "review", why
	return t
}
