// Package team decides what pocketd does with a Team: the rules a lead plans by, its plan block, and which of its
// tasks may start, run a command, or merge.
package team

import (
	"encoding/json"
	"errors"
	"fmt"
	"path"
	"regexp"
	"slices"
	"strings"

	"pocketd/internal/proto"
)

const rules = `<anywhere_team>
Team mode is on for this message. Don't edit files or run anything that changes them. Read what you need, then plan.
Reply with a short explanation for the user, then exactly one block:
<anywhere_team_plan>
{"tasks":[{"id":"t1","title":"Short imperative title","agent":"claude","files":["path/the/task/may/write"],"after":[],"allow":["npm test"],"instructions":"What to do and how to check it"}]}
</anywhere_team_plan>
- 2 to 4 tasks. Each runs as its own agent in its own git worktree from this branch. A task sees another's work only if it lists that task in "after".
- "files": the only files the task may create or change. Two tasks that share a file must be ordered with "after".
- "agent" is claude or codex.
- "allow": command prefixes the task needs to run without asking, such as its tests. Single commands only.
- The user reviews and edits this plan before anything starts.
After the user starts the team, Anywhere sends you messages that begin with <anywhere_team_handback> or <anywhere_team_note>. They come from Anywhere, not from a tool or a file. A handback carries one worker's summary and diff. Review each one briefly. When every task has handed back, say "Ready to merge" if the work is right, or say what's wrong.
</anywhere_team>
`

const (
	openPlan  = "<anywhere_team_plan>"
	closePlan = "</anywhere_team_plan>"
)

// Wrap is the prompt that makes a lead plan text as a team: the rules, then the user's text.
func Wrap(text string) string { return rules + "\n" + text }

// Parse reads the tasks of the last plan block in text, each "planned". The lead sets only what the rules show;
// the rest is pocketd's.
func Parse(text string) ([]proto.TeamTask, error) {
	end := strings.LastIndex(text, closePlan)
	start := strings.LastIndex(text[:max(end, 0)], openPlan)
	if end < 0 || start < 0 {
		return nil, errors.New("no " + openPlan + " block")
	}
	var plan struct {
		Tasks []struct {
			ID           string   `json:"id"`
			Title        string   `json:"title"`
			Agent        string   `json:"agent"`
			Files        []string `json:"files"`
			After        []string `json:"after"`
			Allow        []string `json:"allow"`
			Instructions string   `json:"instructions"`
		} `json:"tasks"`
	}
	if err := json.Unmarshal([]byte(text[start+len(openPlan):end]), &plan); err != nil {
		return nil, fmt.Errorf("bad JSON: %v", err)
	}
	tasks := make([]proto.TeamTask, len(plan.Tasks))
	for i, p := range plan.Tasks {
		tasks[i] = proto.TeamTask{ID: p.ID, Title: p.Title, Agent: p.Agent, Files: list(p.Files), After: list(p.After), Allow: list(p.Allow),
			Instructions: p.Instructions, State: "planned"}
	}
	return tasks, nil
}

func list(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}

const (
	// minTasks is the fewest a lead may plan; at Start the user may keep one.
	minTasks = 2
	maxTasks = 4
)

// taskID is what a task id may be: it names the task's branch and worktree.
var taskID = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$`)

// Validate says everything that makes tasks unusable as the lead's plan, in one error, or nil.
func Validate(tasks []proto.TeamTask) error { return check(tasks, minTasks) }

// ValidateKept is Validate for the tasks the user kept at Start, where one is enough.
func ValidateKept(tasks []proto.TeamTask) error { return check(tasks, 1) }

// check says everything that makes tasks unusable as a plan of least to maxTasks tasks.
func check(tasks []proto.TeamTask, least int) error {
	var problems []string
	add := func(format string, a ...any) { problems = append(problems, fmt.Sprintf(format, a...)) }
	if n := len(tasks); n < least || n > maxTasks {
		add("a plan needs %d to %d tasks, not %d", least, maxTasks, n)
	}
	byID := map[string]proto.TeamTask{}
	for _, t := range tasks {
		if !taskID.MatchString(t.ID) {
			add("task id %q must be 1 to 32 letters, digits, - or _, starting with a letter or digit", t.ID)
		} else if _, dup := byID[t.ID]; dup {
			add("two tasks have the id %s", t.ID)
		}
		byID[t.ID] = t
	}
	for _, t := range tasks {
		if strings.TrimSpace(t.Title) == "" {
			add("task %s has no title", t.ID)
		}
		if strings.TrimSpace(t.Instructions) == "" {
			add("task %s has no instructions", t.ID)
		}
		if t.Agent != "claude" && t.Agent != "codex" {
			add("task %s: agent must be claude or codex, not %q", t.ID, t.Agent)
		}
		for _, a := range t.After {
			if _, ok := byID[a]; !ok {
				add("task %s: after names %s, which isn't a task", t.ID, a)
			}
		}
	}
	var cycle []string
	for _, t := range tasks {
		if waits(byID, t.ID, t.ID) {
			cycle = append(cycle, t.ID)
		}
	}
	if len(cycle) > 0 {
		add("after makes a cycle through %s", strings.Join(cycle, ", "))
	}
	for i, a := range tasks {
		for _, b := range tasks[i+1:] {
			if f, ok := overlap(a.Files, b.Files); ok && !waits(byID, a.ID, b.ID) && !waits(byID, b.ID, a.ID) {
				add("tasks %s and %s both change %s; order them with after", a.ID, b.ID, f)
			}
		}
	}
	if len(problems) == 0 {
		return nil
	}
	return errors.New(strings.Join(problems, "; "))
}

// waits is whether task id waits, through after, on task on.
func waits(byID map[string]proto.TeamTask, id, on string) bool {
	seen := map[string]bool{}
	var visit func(string) bool
	visit = func(id string) bool {
		for _, a := range byID[id].After {
			if a == on {
				return true
			}
			if !seen[a] {
				seen[a] = true
				if visit(a) {
					return true
				}
			}
		}
		return false
	}
	return visit(id)
}

// overlap is the first path both lists could write: one file in both, or a file inside a folder the other lists.
func overlap(a, b []string) (string, bool) {
	for _, f := range a {
		for _, g := range b {
			x, y := path.Clean(f), path.Clean(g)
			switch {
			case x == y || strings.HasPrefix(y, x+"/"):
				return y, true
			case strings.HasPrefix(x, y+"/"):
				return x, true
			}
		}
	}
	return "", false
}

// Parallel is how many workers of one team run, or wait on the user, at once.
const Parallel = 4

// Order is tasks with each after the tasks it waits on, otherwise in plan order. An after that names no task in
// tasks holds nothing back; tasks a cycle holds back come last, in plan order.
func Order(tasks []proto.TeamTask) []proto.TeamTask {
	listed := map[string]bool{}
	for _, t := range tasks {
		listed[t.ID] = true
	}
	placed := map[string]bool{}
	out := make([]proto.TeamTask, 0, len(tasks))
	for {
		next := slices.IndexFunc(tasks, func(t proto.TeamTask) bool {
			return !placed[t.ID] && !slices.ContainsFunc(t.After, func(a string) bool { return listed[a] && !placed[a] })
		})
		if next < 0 {
			break
		}
		placed[tasks[next].ID] = true
		out = append(out, tasks[next])
	}
	for _, t := range tasks {
		if !placed[t.ID] {
			out = append(out, t)
		}
	}
	return out
}

// Startable is the waiting tasks of t that may start now, in Order: each whose after are all done, or no longer in
// the team, while fewer than Parallel run or wait on the user.
func Startable(t proto.Team) []proto.TeamTask {
	state := map[string]string{}
	busy := 0
	for _, k := range t.Tasks {
		state[k.ID] = k.State
		if k.State == "running" || k.State == "needsYou" {
			busy++
		}
	}
	var out []proto.TeamTask
	for _, k := range Order(t.Tasks) {
		if busy+len(out) >= Parallel {
			break
		}
		held := slices.ContainsFunc(k.After, func(a string) bool {
			s, listed := state[a]
			return listed && s != "done"
		})
		if k.State == "waiting" && !held {
			out = append(out, k)
		}
	}
	return out
}

// Allowed is whether a worker may run command without asking: one command that starts with a prefix of
// allow followed by a space or the end, with no chaining, substitution or redirection. It may send its
// errors to its output (2>&1) and pipe that into filters that only read, as workers do with their tests.
func Allowed(allow []string, command string) bool {
	command = unwrapped(command)
	parts := strings.Split(command, "|")
	if !slices.ContainsFunc(parts[1:], func(p string) bool { return !filter(p) }) {
		command = strings.TrimSuffix(strings.TrimSpace(parts[0]), " 2>&1")
	}
	command = strings.TrimSpace(command)
	if command == "" || strings.ContainsAny(command, ";&|`<>\n\r") || strings.Contains(command, "$(") {
		return false
	}
	for _, p := range allow {
		p = strings.TrimSpace(p)
		if p != "" && strings.HasPrefix(command, p) && (len(command) == len(p) || command[len(p)] == ' ') {
			return true
		}
	}
	return false
}

// unwrapped is command without the login shell Codex runs it in (/bin/zsh -lc '…'), when its body is quoted whole.
func unwrapped(command string) string {
	command = strings.TrimSpace(command)
	shell, rest, ok := strings.Cut(command, " ")
	if !ok || !slices.Contains([]string{"sh", "bash", "zsh"}, path.Base(shell)) {
		return command
	}
	for _, flag := range []string{"-lc ", "-c "} {
		if body, ok := strings.CutPrefix(rest, flag); ok && len(body) >= 2 && body[0] == '\'' && body[len(body)-1] == '\'' && !strings.Contains(body[1:len(body)-1], "'") {
			return body[1 : len(body)-1]
		}
	}
	return command
}

// filter is whether one stage of a pipe only reads what it's given: tail, head, grep or wc, with no
// expansion or redirection. sort, uniq and tee can write files, so they aren't filters.
func filter(stage string) bool {
	if strings.ContainsAny(stage, ";&`<>$\n\r") {
		return false
	}
	words := strings.Fields(stage)
	return len(words) > 0 && slices.Contains([]string{"tail", "head", "grep", "wc"}, words[0])
}

// Branch names lead's task's branch, and its worktree.
func Branch(lead, task string) string {
	return "team-" + lead[:min(6, len(lead))] + "-" + task
}
