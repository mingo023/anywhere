package team

import (
	"reflect"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
	"pocketd/internal/teamrules"
)

// twoTasks is a usable plan block: t2 exports what t1 writes.
const twoTasks = `<anywhere_team_plan>
{"tasks":[
{"id":"t1","title":"Add truncate","agent":"claude","files":["src/truncate.js"],"after":[],"allow":["npm test"],"instructions":"Write truncate and its test"},
{"id":"t2","title":"Export truncate","agent":"codex","files":["src/index.js"],"after":["t1"],"allow":[],"instructions":"Export it from src/index.js"}
]}
</anywhere_team_plan>`

func TestWrapPutsTheRulesBeforeTheUsersText(t *testing.T) {
	got := Wrap("Add truncate")
	if !strings.HasPrefix(got, "<anywhere_team>\nTeam mode is on for this message.") ||
		!strings.HasSuffix(got, `say "Ready to merge" if the work is right, or say what's wrong.`+"\n</anywhere_team>\n\nAdd truncate") {
		t.Fatalf("got %q", got)
	}
}

func TestTheRulesExampleIsAPlanBlock(t *testing.T) {
	tasks, err := Parse(Wrap("Add truncate"))
	if err != nil || len(tasks) != 1 || tasks[0].ID != "t1" || tasks[0].Agent != "claude" || !reflect.DeepEqual(tasks[0].Allow, []string{"npm test"}) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseReadsTheLastPlanBlock(t *testing.T) {
	text := "A first try:\n<anywhere_team_plan>{\"tasks\":[]}</anywhere_team_plan>\nBetter, the export after the function:\n" + twoTasks + "\nStart when ready."
	tasks, err := Parse(text)
	want := []proto.TeamTask{
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate and its test", State: "planned"},
		{ID: "t2", Title: "Export truncate", Agent: "codex", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, Instructions: "Export it from src/index.js", State: "planned"},
	}
	if err != nil || !reflect.DeepEqual(tasks, want) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseFillsWhatTheLeadLeftOutAndDropsWhatItCantSet(t *testing.T) {
	tasks, err := Parse(`<anywhere_team_plan>{"tasks":[{"id":"t1","title":"A","agent":"claude","instructions":"Do A","model":"opus","state":"done","worker":"a9","branch":"main"}]}</anywhere_team_plan>`)
	want := []proto.TeamTask{{ID: "t1", Title: "A", Agent: "claude", Files: []string{}, After: []string{}, Allow: []string{}, Instructions: "Do A", State: "planned"}}
	if err != nil || !reflect.DeepEqual(tasks, want) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseSkipsAMentionOfTheTagBeforeTheBlock(t *testing.T) {
	tasks, err := Parse("I'll put the plan in an <anywhere_team_plan> block.\n" + twoTasks)
	if err != nil || len(tasks) != 2 {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestABlockWithoutTasksParsesToNone(t *testing.T) {
	tasks, err := Parse("<anywhere_team_plan>{}</anywhere_team_plan>")
	if err != nil || len(tasks) != 0 {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseRefusesTextWithoutAPlanBlock(t *testing.T) {
	for _, text := range []string{
		"I'd split it in two.",
		`<anywhere_team_plan>{"tasks":[]}`,
		`{"tasks":[]}</anywhere_team_plan>`,
	} {
		if _, err := Parse(text); err == nil || err.Error() != "no <anywhere_team_plan> block" {
			t.Errorf("%q: %v", text, err)
		}
	}
}

func TestParseRefusesABlockThatIsntJSON(t *testing.T) {
	for _, text := range []string{
		`<anywhere_team_plan>{"tasks":[</anywhere_team_plan>`,
		"<anywhere_team_plan>```json\n{\"tasks\":[]}\n```</anywhere_team_plan>",
		`<anywhere_team_plan>{"tasks":[{"id":1}]}</anywhere_team_plan>`,
	} {
		if _, err := Parse(text); err == nil || !strings.HasPrefix(err.Error(), "bad JSON: ") {
			t.Errorf("%q: %v", text, err)
		}
	}
}

// tk is a usable task that writes only <id>.go.
func tk(id string, after ...string) proto.TeamTask {
	return proto.TeamTask{ID: id, Title: "Do " + id, Agent: "claude", Files: []string{id + ".go"}, After: append([]string{}, after...), Allow: []string{},
		Instructions: "Do it and test it", State: "planned"}
}

func with(t proto.TeamTask, f func(*proto.TeamTask)) proto.TeamTask {
	f(&t)
	return t
}

func writes(t proto.TeamTask, files ...string) proto.TeamTask {
	return with(t, func(t *proto.TeamTask) { t.Files = files })
}

func plan(tasks ...proto.TeamTask) []proto.TeamTask { return tasks }

func TestValidateSaysEveryWayAPlanCantBeUsed(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  string
	}{
		{"two independent tasks", plan(tk("t1"), tk("t2")), ""},
		{"four tasks in a diamond", plan(tk("t1"), tk("t2", "t1"), tk("t3", "t1"), tk("t4", "t2", "t3")), ""},
		{"no tasks", nil, "a plan needs 2 to 4 tasks, not 0"},
		{"one task", plan(tk("t1")), "a plan needs 2 to 4 tasks, not 1"},
		{"five tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4"), tk("t5")), "a plan needs 2 to 4 tasks, not 5"},
		{"two tasks with one id", plan(writes(tk("t1"), "a.go"), writes(tk("t1"), "b.go")), "two tasks have the id t1"},
		{"an id that can't name a branch", plan(tk("t 1"), tk("t2")), `task id "t 1" must be 1 to 32 letters, digits, - or _, starting with a letter or digit`},
		{"an empty id", plan(writes(tk(""), "a.go"), tk("t2")), `task id "" must be 1 to 32 letters, digits, - or _, starting with a letter or digit`},
		{"no title", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Title = " " }), tk("t2")), "task t1 has no title"},
		{"no instructions", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Instructions = "" }), tk("t2")), "task t1 has no instructions"},
		{"an unknown agent", plan(tk("t1"), with(tk("t2"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `task t2: agent must be claude or codex, not "gemini"`},
		{"an after that names no task", plan(tk("t1"), tk("t2", "t9")), "task t2: after names t9, which isn't a task"},
		{"a task after itself", plan(tk("t1", "t1"), tk("t2")), "after makes a cycle through t1"},
		{"two tasks after each other", plan(tk("t1", "t2"), tk("t2", "t1")), "after makes a cycle through t1, t2"},
		{"a shared file with no order", plan(writes(tk("t1"), "README.md"), writes(tk("t2"), "README.md")), "tasks t1 and t2 both change README.md; order them with after"},
		{"a shared file in order", plan(writes(tk("t1"), "README.md"), writes(tk("t2", "t1"), "README.md")), ""},
		{"a shared file ordered through a third task", plan(writes(tk("t1"), "a.go"), writes(tk("t2", "t1"), "b.go"), writes(tk("t3", "t2"), "a.go")), ""},
		{"siblings sharing a file", plan(tk("t1"), writes(tk("t2", "t1"), "s.go"), writes(tk("t3", "t1"), "s.go")), "tasks t2 and t3 both change s.go; order them with after"},
		{"a folder and a file in it", plan(writes(tk("t1"), "src/"), writes(tk("t2"), "src/a.go")), "tasks t1 and t2 both change src/a.go; order them with after"},
		{"one file named two ways", plan(writes(tk("t1"), "./a.go"), writes(tk("t2"), "a.go")), "tasks t1 and t2 both change a.go; order them with after"},
		{"a folder whose name starts another's", plan(writes(tk("t1"), "src"), writes(tk("t2"), "srcs/a.go")), ""},
		{"several problems at once", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `a plan needs 2 to 4 tasks, not 1; task t1: agent must be claude or codex, not "gemini"`},
	} {
		err := Validate(c.tasks)
		got := ""
		if err != nil {
			got = err.Error()
		}
		if got != c.want {
			t.Errorf("%s:\n got %q\nwant %q", c.name, got, c.want)
		}
	}
}

func TestTheTasksKeptAtStartMayBeOneButFollowEveryOtherRule(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  string
	}{
		{"one task", plan(tk("t1")), ""},
		{"four tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4")), ""},
		{"no tasks", nil, "a plan needs 1 to 4 tasks, not 0"},
		{"five tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4"), tk("t5")), "a plan needs 1 to 4 tasks, not 5"},
		{"one task with an unknown agent", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `task t1: agent must be claude or codex, not "gemini"`},
		{"one task after a task that's gone", plan(tk("t2", "t1")), "task t2: after names t1, which isn't a task"},
	} {
		err := ValidateKept(c.tasks)
		got := ""
		if err != nil {
			got = err.Error()
		}
		if got != c.want {
			t.Errorf("%s:\n got %q\nwant %q", c.name, got, c.want)
		}
	}
}

func ids(tasks []proto.TeamTask) []string {
	var out []string
	for _, t := range tasks {
		out = append(out, t.ID)
	}
	return out
}

func in(state string, t proto.TeamTask) proto.TeamTask {
	t.State = state
	return t
}

func TestOrderPutsEachTaskAfterWhatItWaitsOnAndKeepsThePlansOrderOtherwise(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  []string
	}{
		{"already in order", plan(tk("t1"), tk("t2", "t1")), []string{"t1", "t2"}},
		{"independent tasks keep the plan's order", plan(tk("t2"), tk("t1")), []string{"t2", "t1"}},
		{"a task waits on one planned after it", plan(tk("t2", "t3"), tk("t1"), tk("t3")), []string{"t1", "t3", "t2"}},
		{"a diamond", plan(tk("t4", "t2", "t3"), tk("t3", "t1"), tk("t2", "t1"), tk("t1")), []string{"t1", "t3", "t2", "t4"}},
		{"an after no longer in the team holds nothing back", plan(tk("t2", "t9"), tk("t1")), []string{"t2", "t1"}},
		{"a cycle's tasks come last", plan(tk("t1", "t2"), tk("t2", "t1"), tk("t3")), []string{"t3", "t1", "t2"}},
	} {
		if got := ids(Order(c.tasks)); !slices.Equal(got, c.want) {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestStartableIsTheWaitingTasksWhoseAfterAreDoneUpToParallel(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  []string
	}{
		{"a proposed plan starts nothing", plan(tk("t1"), tk("t2")), nil},
		{"waiting tasks with no after start", plan(in("waiting", tk("t1")), in("waiting", tk("t2"))), []string{"t1", "t2"}},
		{"a task waits while its after runs", plan(in("running", tk("t1")), in("waiting", tk("t2", "t1"))), nil},
		{"a task starts once its after is done", plan(in("done", tk("t1")), in("waiting", tk("t2", "t1"))), []string{"t2"}},
		{"a failed after holds it back", plan(in("failed", tk("t1")), in("waiting", tk("t2", "t1"))), nil},
		{"every after must be done", plan(in("done", tk("t1")), in("needsYou", tk("t2")), in("waiting", tk("t3", "t1", "t2"))), nil},
		{"an after no longer in the team counts as done", plan(in("waiting", tk("t2", "t1"))), []string{"t2"}},
		{"running and asking tasks fill Parallel's slots", plan(in("running", tk("r1")), in("needsYou", tk("r2")), in("running", tk("r3")), in("waiting", tk("w1")), in("waiting", tk("w2"))), []string{"w1"}},
		{"four busy tasks leave no slot", plan(in("running", tk("r1")), in("running", tk("r2")), in("needsYou", tk("r3")), in("running", tk("r4")), in("waiting", tk("w1"))), nil},
		{"done, failed and stopped tasks free their slots", plan(in("done", tk("d1")), in("failed", tk("f1")), in("stopped", tk("s1")), in("waiting", tk("w1")), in("waiting", tk("w2"))), []string{"w1", "w2"}},
	} {
		got := ids(Startable(proto.Team{Lead: "a1", State: "running", Tasks: c.tasks}))
		if !slices.Equal(got, c.want) {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestAllowedTakesOnlyASingleCommandThatStartsWithAnAllowedPrefix(t *testing.T) {
	allow := []string{"npm test", "go test ./..."}
	for _, c := range []struct {
		command string
		want    bool
	}{
		{"npm test", true},
		{"npm test -- -t foo", true},
		{"   npm test  ", true},
		{"go test ./...", true},
		{"npm testx", false},
		{"npm", false},
		{"npm  test", false},
		{"npm test\t-t foo", false},
		{"go test ./internal/team", false},
		{"rm -rf /", false},
		{"", false},
		{"npm test; rm -rf /", false},
		{"npm test && rm -rf /", false},
		{"npm test || rm -rf /", false},
		{"npm test | sh", false},
		{"npm test & rm -rf /", false},
		{"npm test $(rm -rf /)", false},
		{"npm test `rm -rf /`", false},
		{"npm test\nrm -rf /", false},
		{"npm test\r\nrm -rf /", false},
		{"npm test > package.json", false},
		{"npm test < /etc/passwd", false},
	} {
		if got := Allowed(allow, c.command); got != c.want {
			t.Errorf("%q: got %v, want %v", c.command, got, c.want)
		}
	}
}

func TestAnAllowedCommandMayPipeIntoFiltersThatOnlyRead(t *testing.T) {
	allow := []string{"npm test"}
	for _, c := range []struct {
		command string
		want    bool
	}{
		{"npm test 2>&1 | tail -n 20", true},
		{"npm test | grep -v skipped | head -50", true},
		{"npm test 2>&1", true},
		{"npm test | wc -l", true},
		{"npm test | sh", false},
		{"npm test | tee out.txt", false},
		{"npm test | sort -o package.json", false},
		{"npm test | tail -n 20 > out.txt", false},
		{"npm test | tail $(rm -rf /)", false},
		{"npm test | tail -n $N", false},
		{"npm test |& tail", false},
		{"npm test || tail", false},
		{"npm test |", false},
		{"npm test 2>&1 rm", false},
		{"npm test 2>out.txt | tail", false},
		{"rm -rf / | tail", false},
	} {
		if got := Allowed(allow, c.command); got != c.want {
			t.Errorf("%q: got %v, want %v", c.command, got, c.want)
		}
	}
}

func TestCodexsShellWrapperIsReadThrough(t *testing.T) {
	allow := []string{"npm test"}
	for _, c := range []struct {
		command string
		want    bool
	}{
		{"/bin/zsh -lc 'npm test'", true},
		{"bash -c 'npm test 2>&1 | tail -n 5'", true},
		{"/bin/sh -lc 'npm test; rm -rf /'", false},
		{"/bin/zsh -lc 'npm test' extra", false},
		{`/bin/zsh -lc 'npm test'\''x'`, false},
		{"/usr/bin/python3 -c 'npm test'", false},
	} {
		if got := Allowed(allow, c.command); got != c.want {
			t.Errorf("%q: got %v, want %v", c.command, got, c.want)
		}
	}
}

func TestABlankPrefixAllowsNothing(t *testing.T) {
	for _, allow := range [][]string{nil, {""}, {"  "}} {
		for _, command := range []string{"", "ls", " ls"} {
			if Allowed(allow, command) {
				t.Errorf("%q allowed %q", allow, command)
			}
		}
	}
}

func TestAnAllowedPrefixIsReadWithoutItsSpaces(t *testing.T) {
	if !Allowed([]string{" npm test "}, "npm test -- -t foo") {
		t.Fatal("refused")
	}
}

func TestABranchIsNamedForTheLeadsFirstSixCharactersAndTheTask(t *testing.T) {
	if got := Branch("3f9a0c1b-2d4e-4f60-8a7b-9c0d1e2f3a4b", "t1"); got != "team-3f9a0c-t1" {
		t.Errorf("got %q", got)
	}
	if got := Branch("ab", "t1"); got != "team-ab-t1" {
		t.Errorf("a short id: got %q", got)
	}
}

func TestATeamPromptStrippedOfItsRulesIsWhatTheUserTyped(t *testing.T) {
	if got := teamrules.Strip(Wrap("Add truncate\nwith a test")); got != "Add truncate\nwith a test" {
		t.Fatalf("got %q", got)
	}
}
