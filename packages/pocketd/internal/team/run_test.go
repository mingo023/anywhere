package team

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

const testLead = "abcdef123456"

// planOfThree is a plan waiting for Start: t1 and t2 side by side, t3 after both.
func planOfThree() proto.Team {
	return proto.Team{Lead: testLead, State: "proposed", UpdatedAt: 1, Tasks: []proto.TeamTask{
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate.", State: "planned"},
		{ID: "t2", Title: "Add titleCase", Agent: "claude", Files: []string{"src/title.js"}, After: []string{}, Allow: []string{}, Instructions: "Write titleCase.", State: "planned"},
		{ID: "t3", Title: "Export both", Agent: "codex", Files: []string{"src/index.js", "README.md"}, After: []string{"t1", "t2"}, Allow: []string{}, Instructions: "Export them.", State: "planned"},
	}}
}

// startedPlan is planOfThree started as planned.
func startedPlan(t *testing.T) proto.Team {
	t.Helper()
	tm, err := Started(planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "claude", Allow: []string{"npm test"}}, {ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}})
	if err != nil {
		t.Fatal(err)
	}
	return tm
}

// t1Working is startedPlan with t1's worker w1 at work.
func t1Working(t *testing.T) proto.Team {
	t.Helper()
	return Launched(Starting(startedPlan(t), "t1", nil), "t1", "w1", "/wt/team-abcdef-t1")
}

func TestStartKeepsThePickedTasksWithTheirEditsInPlanOrder(t *testing.T) {
	got, err := Started(planOfThree(), []proto.TaskPick{
		{ID: "t2", Agent: "codex", Model: "gpt-6.1-sol", Allow: []string{"npm test", "npx eslint"}},
		{ID: "t1", Agent: "claude", Model: "sonnet"},
	})
	want := planOfThree()
	want.State, want.Tasks = "running", want.Tasks[:2]
	want.Tasks[0].Model, want.Tasks[0].Allow, want.Tasks[0].State = "sonnet", []string{}, "waiting"
	want.Tasks[1].Agent, want.Tasks[1].Model, want.Tasks[1].Allow, want.Tasks[1].State = "codex", "gpt-6.1-sol", []string{"npm test", "npx eslint"}, "waiting"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v\nwant %+v", got, err, want)
	}
}

func TestStartRefusesWhatTheUserCannotPick(t *testing.T) {
	notProposed := planOfThree()
	notProposed.State = "running"
	all := []proto.TaskPick{{ID: "t1", Agent: "claude"}, {ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}}
	for name, c := range map[string]struct {
		team  proto.Team
		picks []proto.TaskPick
	}{
		"a team that isn't proposed":   {notProposed, all},
		"a task the plan doesn't have": {planOfThree(), append(all[:2:2], proto.TaskPick{ID: "t9", Agent: "claude"})},
		"a task picked twice":          {planOfThree(), append(all[:3:3], proto.TaskPick{ID: "t1", Agent: "claude"})},
		"no task left":                 {planOfThree(), nil},
		"an agent pocketd can't run":   {planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "gemini"}, all[1], all[2]}},
	} {
		if got, err := Started(c.team, c.picks); err == nil {
			t.Errorf("%s: started %+v", name, got)
		}
	}
}

func TestStartCanKeepASingleTask(t *testing.T) {
	got, err := Started(planOfThree(), []proto.TaskPick{{ID: "t2", Agent: "claude"}})
	if err != nil || got.State != "running" || len(got.Tasks) != 1 || got.Tasks[0].ID != "t2" || got.Tasks[0].State != "waiting" {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestStartRefusesToDropATaskAKeptOneStartsFrom(t *testing.T) {
	_, err := Started(planOfThree(), []proto.TaskPick{{ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}})
	if err == nil || err.Error() != "Task t3 starts from t1's work; keep t1 or remove t3 too" {
		t.Fatalf("got %v", err)
	}
}

func TestABranchTakesTheFirstNameThatIsFreeAndWhoseBaseIsFree(t *testing.T) {
	for _, c := range []struct {
		taken []string
		want  string
	}{
		{nil, "team-abcdef-t1"},
		{[]string{"main", "team-abcdef-t2"}, "team-abcdef-t1"},
		{[]string{"team-abcdef-t1"}, "team-abcdef-t1-2"},
		{[]string{"Team-ABCDEF-T1"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1/old"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1-base"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1", "team-abcdef-t1-2-base", "team-abcdef-t1-3"}, "team-abcdef-t1-4"},
	} {
		if got := Free("team-abcdef-t1", c.taken); got != c.want {
			t.Errorf("%q: got %q, want %q", c.taken, got, c.want)
		}
	}
}

func TestATaskStartsFromTheLeadsHeadItsOneDependencyOrTheMergeOfSeveral(t *testing.T) {
	tm := planOfThree()
	tm.Tasks[0].Branch, tm.Tasks[1].Branch, tm.Tasks[2].Branch = "team-abcdef-t1-2", "team-abcdef-t2", "team-abcdef-t3"
	one := proto.TeamTask{ID: "t4", After: []string{"t1"}}
	got := []string{Base(tm, tm.Tasks[0], "c0ffee"), Base(tm, one, "c0ffee"), Base(tm, tm.Tasks[2], "c0ffee")}
	if want := []string{"c0ffee", "team-abcdef-t1-2", "team-abcdef-t3-base"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q", got)
	}
	if got, want := Branches(tm, tm.Tasks[2].After), []string{"team-abcdef-t1-2", "team-abcdef-t2"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q", got)
	}
}

func TestAWorkerLaunchesAsAnEditingChatInItsOwnWorktreeUnderTheLead(t *testing.T) {
	task := planOfThree().Tasks[2]
	task.Model, task.Branch = "gpt-6.1-sol", "team-abcdef-t3-2"
	got := WorkerSpec("/repo", testLead, task, "team-abcdef-t3-2-base")
	want := proto.LaunchSpec{Project: "/repo", Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "team-abcdef-t3-2", Base: "team-abcdef-t3-2-base"}},
		Provider: "codex", Model: "gpt-6.1-sol", Access: "edits", Chat: true, Parent: testLead}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestTheAssignmentGivesTheWorkerItsTaskAndTheFilesItMayChange(t *testing.T) {
	want := `<anywhere_team_assignment task="t3">
You're one worker in a team that a lead agent planned. Your task: Export both.
Export them.
Change only these files: src/index.js, README.md. Don't commit, and don't create branches or worktrees.
When you're done, reply with a short summary of what you changed and how you checked it.
</anywhere_team_assignment>`
	if got := Assignment(planOfThree().Tasks[2]); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestAStartingTaskRunsOnItsBranchAndThenNamesItsWorker(t *testing.T) {
	before := startedPlan(t)
	got := Launched(Starting(before, "t1", nil), "t1", "w1", "/wt/team-abcdef-t1")
	want := startedPlan(t)
	want.Tasks[0].State, want.Tasks[0].Branch, want.Tasks[0].Worker, want.Tasks[0].Worktree = "running", "team-abcdef-t1", "w1", "/wt/team-abcdef-t1"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
	if before.Tasks[0].State != "waiting" {
		t.Fatal("Starting changed the team it was given")
	}
}

func TestAStartingTaskSkipsNamesTheProjectOrItsTeamHasTaken(t *testing.T) {
	tm := startedPlan(t)
	tm.Tasks[1].Branch = "team-abcdef-t1-2"
	got := Starting(tm, "t1", []string{"team-abcdef-t1"})
	if b := got.Tasks[0].Branch; b != "team-abcdef-t1-3" {
		t.Fatalf("got %q", b)
	}
}

func TestAWorkersStatusShowsOnItsTaskWhileItRuns(t *testing.T) {
	asking, changed := WorkerStatus(t1Working(t), "w1", "needsYou")
	if !changed || asking.Tasks[0].State != "needsYou" {
		t.Fatalf("asking: %t %+v", changed, asking.Tasks[0])
	}
	back, changed := WorkerStatus(asking, "w1", "working")
	if !changed || back.Tasks[0].State != "running" {
		t.Fatalf("back: %t %+v", changed, back.Tasks[0])
	}
	failed, _ := Failed(back, "t1", "x")
	for name, c := range map[string]struct {
		team           proto.Team
		worker, status string
	}{
		"the same status":   {back, "w1", "working"},
		"another worker":    {back, "w9", "needsYou"},
		"a turn's end":      {back, "w1", "done"},
		"a task that ended": {failed, "w1", "working"},
	} {
		if _, changed := WorkerStatus(c.team, c.worker, c.status); changed {
			t.Errorf("%s changed the team", name)
		}
	}
}

func TestAFailedTurnFailsOnlyATaskThatIsRunning(t *testing.T) {
	got, changed := Failed(t1Working(t), "t1", "The worker's turn failed")
	want := t1Working(t)
	want.Tasks[0].State, want.Tasks[0].Summary = "failed", "The worker's turn failed"
	if !changed || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %t %+v", changed, got)
	}
	if _, changed := Failed(got, "t1", "again"); changed {
		t.Fatal("a failed task failed again")
	}
	if _, changed := Failed(startedPlan(t), "t2", "x"); changed {
		t.Fatal("a waiting task failed")
	}
}

func TestAFailedHandbackCarriesWhy(t *testing.T) {
	want := "<anywhere_team_handback task=\"t3\" status=\"failed\">\nMerging team-abcdef-t2 conflicted in: src/index.js\n</anywhere_team_handback>"
	if got := FailedHandback("t3", "Merging team-abcdef-t2 conflicted in: src/index.js"); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestOnlyALiveTeamsWorkersAreFound(t *testing.T) {
	live := t1Working(t)
	if tm, task, ok := Find([]proto.Team{live}, "w1"); !ok || tm.Lead != testLead || task.ID != "t1" {
		t.Fatalf("got %v %+v", ok, task)
	}
	if _, _, ok := Find([]proto.Team{live}, ""); ok {
		t.Fatal("found a task with no worker")
	}
	merged := t1Working(t)
	merged.State = "merged"
	if _, _, ok := Find([]proto.Team{merged}, "w1"); ok {
		t.Fatal("found a merged team's worker")
	}
}

func TestARunningTeamAllowsItsWorkersListedCommandsOnly(t *testing.T) {
	running := []proto.Team{t1Working(t)}
	review := t1Working(t)
	review.State = "review"
	for _, c := range []struct {
		teams                 []proto.Team
		worker, tool, command string
		want                  bool
	}{
		{running, "w1", "Bash", "npm test", true},
		{running, "w1", "shell", "npm test -- --run", true},
		{running, "w1", "Bash", "npm test && rm -rf src", false},
		{running, "w1", "Bash", "npm run build", false},
		{running, "w1", "Write", "npm test", false},
		{running, "w9", "Bash", "npm test", false},
		{[]proto.Team{review}, "w1", "Bash", "npm test", false},
	} {
		if got := Allows(c.teams, c.worker, c.tool, c.command); got != c.want {
			t.Errorf("%s %s %q: %t", c.worker, c.tool, c.command, got)
		}
	}
}

func TestTheLastTextIsTheReplyToTheLastPrompt(t *testing.T) {
	items := []proto.Item{{Kind: "user", Text: "go"}, {Kind: "assistant", Text: "Looking."}, {Kind: "assistant", Text: "Added truncate; npm test passes."}, {Kind: "notice", Text: "Allowed by the team plan: npm test"}}
	if text, ok := LastText(items); !ok || text != "Added truncate; npm test passes." {
		t.Fatalf("got %q %t", text, ok)
	}
	if text, ok := LastText(append(items, proto.Item{Kind: "user", Text: "more"})); ok || text != "" {
		t.Fatalf("an unanswered prompt read %q", text)
	}
	if _, ok := LastText(nil); ok {
		t.Fatal("an empty timeline has a reply")
	}
}

func TestATurnIsNamedByItsPrompt(t *testing.T) {
	items := []proto.Item{{Kind: "user", ID: "u1"}, {Kind: "assistant", ID: "a1"}, {Kind: "user", ID: "s1", ParentToolUseID: "toolu_1"}, {Kind: "assistant", ID: "a2"}}
	if got := TurnID(items); got != "u1" {
		t.Fatalf("got %q", got)
	}
	if got := TurnID(nil); got != "" {
		t.Fatalf("no prompt named %q", got)
	}
}

func TestATurnHandsBackOnce(t *testing.T) {
	first, _ := Handed(t1Working(t), "t1", "u1", "Added truncate.", " src/truncate.js | 9 +")
	for name, c := range map[string]struct {
		turn    string
		changed bool
	}{
		"the same turn again":           {"u1", false},
		"the worker's next turn":        {"u2", true},
		"a turn whose prompt has no id": {"", true},
	} {
		if _, changed := Handed(first, "t1", c.turn, "Again.", " src/truncate.js | 10 +"); changed != c.changed {
			t.Errorf("%s: changed %t", name, changed)
		}
	}
}

func TestAHandbackFinishesItsTaskAndTheLastOneStartsTheReview(t *testing.T) {
	pair, err := Started(planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "claude"}, {ID: "t2", Agent: "claude"}})
	if err != nil {
		t.Fatal(err)
	}
	pair = Launched(Starting(pair, "t1", nil), "t1", "w1", "/wt1")
	pair = Launched(Starting(pair, "t2", nil), "t2", "w2", "/wt2")
	one, changed := Handed(pair, "t1", "u1", "Added truncate.", " src/truncate.js | 9 +")
	want := pair
	want.Tasks = append([]proto.TeamTask(nil), pair.Tasks...)
	want.Tasks[0].State, want.Tasks[0].Summary, want.Tasks[0].Stat, want.Tasks[0].Turn = "done", "Added truncate.", " src/truncate.js | 9 +", "u1"
	if !changed || !reflect.DeepEqual(one, want) {
		t.Fatalf("got %+v", one)
	}
	both, _ := Handed(one, "t2", "v1", "Added titleCase.", " src/title.js | 7 +")
	if both.State != "review" {
		t.Fatalf("state %q", both.State)
	}
	again, changed := Handed(both, "t1", "u2", "Fixed the edge case.", " src/truncate.js | 10 +")
	if !changed || again.State != "review" || again.Tasks[0].Summary != "Fixed the edge case." {
		t.Fatalf("a second handback: %+v", again)
	}
	if _, changed := Handed(both, "t9", "x1", "", ""); changed {
		t.Fatal("an unknown task handed back")
	}
	merged := both
	merged.Tasks = append([]proto.TeamTask(nil), both.Tasks...)
	merged.Tasks[0].State = "merged"
	if _, changed := Handed(merged, "t1", "u3", "late", ""); changed {
		t.Fatal("a merged task handed back")
	}
}

func TestTheHandbackTellsTheLeadWhatTheWorkerDidOnItsBranch(t *testing.T) {
	task := t1Working(t).Tasks[0]
	want := `<anywhere_team_handback task="t1" title="Add truncate" branch="team-abcdef-t1">
Summary from the worker:
Added truncate.

git diff --stat:
 src/truncate.js | 9 +
 1 file changed, 9 insertions(+)
</anywhere_team_handback>`
	if got := Handback(task, "Added truncate.", " src/truncate.js | 9 +\n 1 file changed, 9 insertions(+)"); got != want {
		t.Fatalf("got\n%s", got)
	}
	want = `<anywhere_team_handback task="t1" title="Add truncate" branch="team-abcdef-t1">
Summary from the worker:
Nothing needed changing.

git diff --stat:
(no changes)
</anywhere_team_handback>`
	if got := Handback(task, "Nothing needed changing.", ""); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestOnlyAnUnnegatedReadyToMergeIsReady(t *testing.T) {
	for text, want := range map[string]bool{
		"Ready to merge.":                                 true,
		"All three look right. READY TO MERGE":            true,
		"Not ready to merge: t3 doesn't export truncate.": false,
		"This isn't ready to merge yet.":                  false,
		"t3 needs another pass.":                          false,
	} {
		if got := Ready(text); got != want {
			t.Errorf("%q: %t", text, got)
		}
	}
}

func TestTheLeadsVerdictCountsOnlyInReview(t *testing.T) {
	review := t1Working(t)
	review.State = "review"
	ready, changed := Verdict(review, "Ready to merge.")
	want := review
	want.Ready = true
	if !changed || !reflect.DeepEqual(ready, want) {
		t.Fatalf("got %t %+v", changed, ready)
	}
	if _, changed := Verdict(ready, "Ready to merge, still."); changed {
		t.Fatal("the same verdict changed the team")
	}
	if again, changed := Verdict(ready, "Not ready to merge."); !changed || again.Ready {
		t.Fatalf("a new verdict: %t %+v", changed, again)
	}
	if _, changed := Verdict(t1Working(t), "Ready to merge."); changed {
		t.Fatal("a running team took a verdict")
	}
}

// reviewedPair is a reviewed team listed t2 first, though t2 is after t1.
func reviewedPair() proto.Team {
	return proto.Team{Lead: testLead, State: "review", Tasks: []proto.TeamTask{
		{ID: "t2", Title: "Use truncate", Agent: "claude", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, State: "done", Branch: "team-abcdef-t2"},
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{}, State: "done", Branch: "team-abcdef-t1"},
	}}
}

func TestStopStopsOnlyATaskWhoseWorkerIsAtWork(t *testing.T) {
	got, err := Stopped(t1Working(t), "t1")
	want := t1Working(t)
	want.Tasks[0].State = "stopped"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
	if _, err := Stopped(got, "t1"); err == nil {
		t.Fatal("stopped a stopped task")
	}
	if _, err := Stopped(startedPlan(t), "t2"); err == nil {
		t.Fatal("stopped a waiting task")
	}
}

func TestATaskStoppedWhileItsWorkerLaunchesGetsNoAssignment(t *testing.T) {
	stopped, err := Stopped(t1Working(t), "t1")
	if err != nil {
		t.Fatal(err)
	}
	if !Assigns(t1Working(t), "t1") || Assigns(stopped, "t1") || Assigns(stopped, "t9") {
		t.Fatal("assigned a task that isn't running, or didn't assign one that is")
	}
}

func TestTheNotesSayWhoSaidWhat(t *testing.T) {
	if got, want := Note("t1", "use tabs"), `<anywhere_team_note task="t1">The user told worker t1: "use tabs"</anywhere_team_note>`; got != want {
		t.Fatalf("got %s", got)
	}
	want := `<anywhere_team_note>Merging team-abcdef-t2 conflicted in: src/a.go, src/b.go. The merge was aborted; nothing was merged after it.</anywhere_team_note>`
	if got := ConflictNote("team-abcdef-t2", []string{"src/a.go", "src/b.go"}); got != want {
		t.Fatalf("got %s", got)
	}
}

func TestMergingWaitsForTheReviewAndClearsTheLastConflict(t *testing.T) {
	got, err := Merging(Conflicted(reviewedPair(), []string{"src/index.js"}))
	want := reviewedPair()
	want.State = "merging"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
	if _, err := Merging(reviewedPair()); err != nil {
		t.Fatal(err)
	}
	if _, err := Merging(t1Working(t)); err == nil {
		t.Fatal("a running team merged")
	}
}

func TestTheMergeTakesTheDoneTasksInDependencyOrder(t *testing.T) {
	if got := ids(MergeOrder(reviewedPair())); !reflect.DeepEqual(got, []string{"t1", "t2"}) {
		t.Fatalf("got %q", got)
	}
	if got := ids(MergeOrder(Merged(reviewedPair(), []string{"t1"}))); !reflect.DeepEqual(got, []string{"t2"}) {
		t.Fatalf("after t1 merged: %q", got)
	}
}

func TestAFullMergeMergesTheTeam(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := Merged(merging, []string{"t1", "t2"})
	want := reviewedPair()
	want.State, want.Tasks[0].State, want.Tasks[1].State = "merged", "merged", "merged"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestAConflictKeepsWhatMergedAndNamesTheFiles(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := Conflicted(Merged(merging, []string{"t1"}), []string{"src/index.js"})
	want := reviewedPair()
	want.State, want.Conflict, want.Tasks[1].State = "conflict", []string{"src/index.js"}, "merged"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestAMergeGitRefusesGoesBackToReviewSayingWhy(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := MergeFailed(merging, "This checkout has uncommitted changes; commit or stash them first")
	want := reviewedPair()
	want.Error = "This checkout has uncommitted changes; commit or stash them first"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}
