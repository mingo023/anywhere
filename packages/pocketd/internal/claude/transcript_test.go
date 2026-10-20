package claude

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

func kinds(events []timeline.Event) []string {
	var out []string
	for _, e := range events {
		out = append(out, e.Kind)
	}
	return out
}

func TestMap(t *testing.T) {
	cases := []struct {
		name, line string
		want       []string
	}{
		{"prompt", `{"type":"user","message":{"content":"hi"}}`, []string{"user"}},
		{"prompt blocks", `{"type":"user","message":{"content":[{"type":"text","text":"hi"}]}}`, []string{"user"}},
		{"interrupt", `{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}`, []string{"result"}},
		{"slash command", `{"type":"user","message":{"content":"<command-name>/compact</command-name>"}}`, []string{"user"}},
		{"clear", `{"type":"user","message":{"content":"<command-name>/clear</command-name>\n<command-args></command-args>"}}`, nil},
		{"local command", `{"type":"user","message":{"content":"<local-command-stdout>ok</local-command-stdout>"}}`, nil},
		{"typed slash command", `{"type":"user","message":{"content":"/compact"}}`, nil},
		{"prompt starting with a path", `{"type":"user","message":{"content":"/tmp/x.txt is empty"}}`, []string{"user"}},
		{"bash mode input", `{"type":"user","uuid":"b1","message":{"content":"<bash-input>echo hi</bash-input>"}}`, []string{"user", "tool_start"}},
		{"bash mode output", `{"type":"user","parentUuid":"b1","message":{"content":"<bash-stdout>hi</bash-stdout><bash-stderr></bash-stderr>"}}`, []string{"tool_end"}},
		{"task notification", `{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>done</task-notification>"}}`, nil},
		{"background task stops", `{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}`, []string{"tool_end"}},
		{"background task stops mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"task-notification","prompt":"<task-notification>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}`, []string{"tool_end"}},
		{"prompt sent mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"hi"}}`, []string{"user"}},
		{"other queued command", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"bash","prompt":"ls"}}`, nil},
		{"background agent launched", `{"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched successfully."}]}}`, []string{"tool_background"}},
		{"command sent to the background", `{"type":"user","toolUseResult":{"stdout":"","backgroundTaskId":"b1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Command running in background with ID: b1."}]}}`, []string{"tool_background"}},
		{"tool result as a string", `{"type":"user","toolUseResult":"Error: no such file","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"x","is_error":true}]}}`, []string{"tool_end"}},
		{"typed prompt", `{"type":"user","origin":{"kind":"human"},"message":{"content":"hi"}}`, []string{"user"}},
		{"meta", `{"type":"user","isMeta":true,"message":{"content":"x"}}`, nil},
		{"subagent", `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"x"}]}}`, nil},
		{"compact summary", `{"type":"user","isCompactSummary":true,"message":{"content":"x"}}`, nil},
		{"assistant", `{"type":"assistant","message":{"content":[{"type":"thinking","thinking":""},{"type":"thinking","thinking":"hm"},{"type":"text","text":"ok"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}`, []string{"thinking", "assistant_text", "tool_start"}},
		{"tool result", `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"a"}],"is_error":true}]}}`, []string{"tool_end"}},
		{"tool result and text", `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a"},{"type":"text","text":"hi"}]}}`, []string{"tool_end", "user"}},
		{"turn end", `{"type":"system","subtype":"turn_duration","durationMs":7}`, []string{"result"}},
		{"stop hooks in the TUI", `{"type":"system","subtype":"stop_hook_summary","entrypoint":"cli"}`, nil},
		{"compact", `{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"auto"}}`, []string{"compacted"}},
		{"junk", `{"type":"summary"}`, nil},
	}
	for _, c := range cases {
		got := Map([]byte(c.line))
		if !reflect.DeepEqual(kinds(got), c.want) {
			t.Errorf("%s: got %v want %v", c.name, kinds(got), c.want)
		}
	}
}

func TestASlashCommandShowsAsTyped(t *testing.T) {
	ev := Map([]byte(`{"type":"user","message":{"content":"<command-message>grill-me</command-message>\n<command-name>/grill-me</command-name>\n<command-args>it is a test</command-args>"}}`))
	if len(ev) != 1 || ev[0].Kind != "user" || ev[0].Text != "/grill-me it is a test" {
		t.Fatalf("got %+v", ev)
	}
}

func TestMapDetails(t *testing.T) {
	ev := Map([]byte(`{"type":"user","uuid":"b1","message":{"role":"user","content":"<bash-input> ls  -a</bash-input>"}}`))
	if len(ev) != 2 || ev[0].Text != "! ls  -a" || ev[1].ToolUseID != "b1" || ev[1].Name != "Bash" || string(ev[1].Input) != `{"command":"ls  -a"}` {
		t.Errorf("bash input: %+v", ev)
	}
	ev = Map([]byte(`{"type":"user","parentUuid":"b1","message":{"role":"user","content":"<bash-stdout>a.txt</bash-stdout><bash-stderr>ls: x: No such file</bash-stderr>"}}`))
	if len(ev) != 1 || ev[0].ToolUseID != "b1" || !ev[0].OK || *ev[0].Output != "a.txt\nls: x: No such file" {
		t.Errorf("bash output: %+v", ev)
	}
	ev = Map([]byte(`{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"out","is_error":true}]}}`))
	if ev[0].ToolUseID != "t1" || ev[0].OK || *ev[0].Output != "out" {
		t.Errorf("tool_end: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","message":{"content":"[Request interrupted by user for tool use]"}}`))
	if ev[0].OK || ev[0].Error != "interrupted" {
		t.Errorf("interrupt: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent finished</summary>\n<result>Docs &amp; ADRs, then </result> quoted</result>\n</task-notification>"}}`))
	if ev[0].ToolUseID != "t1" || !ev[0].OK || *ev[0].Output != "Docs & ADRs, then </result> quoted" {
		t.Errorf("task end: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"attachment","attachment":{"type":"queued_command","commandMode":"task-notification","prompt":"<task-notification>\n<tool-use-id>t2</tool-use-id>\n<status>completed</status>\n<result>Task 2.2 is done</result>\n</task-notification>"}}`))
	if ev[0].ToolUseID != "t2" || !ev[0].OK || *ev[0].Output != "Task 2.2 is done" {
		t.Errorf("mid-turn task end: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification><tool-use-id>t1</tool-use-id><status>failed</status></task-notification>"}}`))
	if ev[0].OK || ev[0].Output != nil {
		t.Errorf("failed task: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification><tool-use-id>t1</tool-use-id><status>killed</status></task-notification>"}}`))
	if !ev[0].OK {
		t.Errorf("stopped task: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":{"backgroundTaskId":"b1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Command running in background with ID: b1."}]}}`))
	if ev[0].ToolUseID != "t1" || ev[0].TaskID != "b1" || ev[0].Output != nil {
		t.Errorf("background command: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]}}`))
	if ev[0].Kind != "tool_background" || ev[0].ToolUseID != "t1" || ev[0].TaskID != "a1" {
		t.Errorf("background agent: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":{"message":"Successfully stopped task: b1 (bun run dev)","task_id":"b1","task_type":"local_bash","command":"bun run dev"},"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"stopped"}]}}`))
	if len(ev) != 2 || ev[0].ToolUseID != "t2" || ev[1].Kind != "tool_end" || ev[1].TaskID != "b1" || ev[1].ToolUseID != "" || !ev[1].OK {
		t.Errorf("stop ends the stopped task: %+v", ev)
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":"Error: No task found with ID: b9","message":{"content":[{"type":"tool_result","tool_use_id":"t3","content":"Error: No task found with ID: b9","is_error":true}]}}`))
	if len(ev) != 1 || ev[0].ToolUseID != "t3" || ev[0].OK {
		t.Errorf("a failed stop ends nothing else: %+v", ev)
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":{"taskId":"m1","persistent":false,"timeoutMs":60000},"message":{"content":[{"type":"tool_result","tool_use_id":"t4","content":"watching"}]}}`))
	if len(ev) != 1 || ev[0].ToolUseID != "t4" {
		t.Errorf("only a stop ends a task: %+v", ev)
	}
	ev = Map([]byte(`{"type":"system","subtype":"turn_duration","durationMs":7}`))
	if !ev[0].OK || ev[0].DurationMs != 7 {
		t.Errorf("result: %+v", ev[0])
	}
}

func TestAClaudePTurnEndsAtItsStopHooksTimedFromItsFirstEvent(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"queue-operation","timestamp":"2026-10-18T06:12:20.000Z"}`,
		`{"type":"user","entrypoint":"sdk-cli","timestamp":"2026-10-18T06:12:21.000Z","message":{"content":"hi"}}`,
		`{"type":"assistant","entrypoint":"sdk-cli","timestamp":"2026-10-18T06:12:22.000Z","message":{"content":[{"type":"text","text":"ok"}]}}`,
		`{"type":"system","subtype":"stop_hook_summary","entrypoint":"sdk-cli","timestamp":"2026-10-18T06:12:23.500Z"}`,
		`{"type":"last-prompt","timestamp":"2026-10-18T06:12:30.000Z"}`,
		`{"type":"user","entrypoint":"sdk-cli","origin":{"kind":"task-notification"},"timestamp":"2026-10-18T06:12:40.000Z","message":{"content":"<task-notification>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}`,
		`{"type":"system","subtype":"stop_hook_summary","entrypoint":"sdk-cli","timestamp":"2026-10-18T06:12:41.000Z"}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "assistant_text", "result", "tool_end", "result"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
	if !got[2].OK || got[2].DurationMs != 2500 || got[4].DurationMs != 1000 {
		t.Fatalf("ends: %+v %+v", got[2], got[4])
	}
}

func TestAPromptTheAgentTookMidTurnShowsAndTimesTheReplyAfterIt(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"user","entrypoint":"sdk-cli","timestamp":"2026-10-20T04:03:00.000Z","message":{"content":"redo it"}}`,
		`{"type":"assistant","entrypoint":"sdk-cli","timestamp":"2026-10-20T04:12:00.000Z","message":{"content":[{"type":"text","text":"checking"}]}}`,
		`{"type":"attachment","entrypoint":"sdk-cli","timestamp":"2026-10-20T04:13:00.000Z","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"use headless"}}`,
		`{"type":"system","subtype":"stop_hook_summary","entrypoint":"sdk-cli","timestamp":"2026-10-20T04:15:00.000Z"}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "assistant_text", "user", "result"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
	if got[2].Text != "use headless" || got[3].DurationMs != 120_000 {
		t.Fatalf("prompt %+v, end %+v", got[2], got[3])
	}
}

func TestAClaudePTurnThatFailsEndsWithTheErrorAlone(t *testing.T) {
	got := Map([]byte(`{"type":"assistant","entrypoint":"sdk-cli","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"There's an issue with the selected model."}]}}`))
	if len(got) != 1 || got[0].Kind != "result" || got[0].OK || got[0].Error != "There's an issue with the selected model." {
		t.Fatalf("got %+v", got)
	}
}

func TestTailWaitsForFileAndPartialLines(t *testing.T) {
	dir := t.TempDir()
	lines := make(chan string, 10)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	path := filepath.Join(dir, `[p] *?\`, "s.jsonl")
	go Tail(ctx, path, func(b []byte) { lines <- string(b) })

	time.Sleep(3 * poll)
	os.Mkdir(filepath.Dir(path), 0o700)
	f, _ := os.Create(path)
	defer f.Close()
	f.WriteString("one\ntw")
	expectLine(t, lines, "one")
	time.Sleep(3 * poll)
	f.WriteString("o\n")
	expectLine(t, lines, "two")
}

func expectLine(t *testing.T, lines <-chan string, want string) {
	t.Helper()
	select {
	case got := <-lines:
		if got != want {
			t.Fatal(got)
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("%q never arrived", want)
	}
}

func TestTailReadsLastLinesAfterCancel(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s.jsonl")
	f, _ := os.Create(path)
	defer f.Close()
	f.WriteString("one\n")
	lines := make(chan string, 10)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		Tail(ctx, path, func(b []byte) { lines <- string(b) })
		close(done)
	}()
	expectLine(t, lines, "one")
	f.WriteString("two\n")
	cancel()
	<-done
	expectLine(t, lines, "two")
}

func TestModel(t *testing.T) {
	cases := map[string]string{
		`{"type":"assistant","message":{"model":"claude-opus-5","content":[]}}`:          "claude-opus-5",
		`{"type":"assistant","message":{"model":"<synthetic>","content":[]}}`:            "",
		`{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-4-5"}}`: "",
		`{"type":"user","message":{"model":"claude-opus-5","content":"hi"}}`:             "",
	}
	for line, want := range cases {
		if got := Model([]byte(line)); got != want {
			t.Errorf("Model(%s) = %q, want %q", line, got, want)
		}
	}
}

func TestAnOpeningReadsWhetherAClearStartedATranscriptAndItsLatestPrompt(t *testing.T) {
	const clear = `{"type":"user","entrypoint":"cli","message":{"content":"<command-name>/clear</command-name>\n<command-args></command-args>"}}`
	dir := t.TempDir()
	for content, want := range map[string]Opening{
		`{"type":"mode","mode":"normal"}` + "\n" + clear:                                                                              {Cleared: true, Interactive: true},
		clear + "\n" + `{"type":"user","entrypoint":"cli","message":{"content":"fix the bug"}}`:                                       {Cleared: true, Prompt: "fix the bug", Interactive: true},
		`{"type":"user","message":{"content":"/clear"}}`:                                                                              {Cleared: true},
		`{"type":"user","entrypoint":"cli","message":{"content":"what does /clear do?"}}`:                                             {Prompt: "what does /clear do?", Interactive: true},
		`{"type":"user","message":{"content":"<command-name>/compact</command-name>"}}`:                                               {},
		`{"type":"queue-operation"}` + "\n" + `{"type":"user","entrypoint":"sdk-cli","message":{"content":"Write a commit message"}}`: {Prompt: "Write a commit message"},
		`{"type":"user","entrypoint":"cli","message":{"content":"first"}}` + "\n" + clear:                                             {Prompt: "first", Interactive: true},
		`{"type":"user","entrypoint":"cli","message":{"content":"first"}}` + "\n" + `{"type":"user","message":{"content":"then"}}`:    {Prompt: "then", Interactive: true},
	} {
		path := filepath.Join(dir, "s.jsonl")
		os.WriteFile(path, []byte(content+"\n"), 0o600)
		got := Opening{}.Read(path)
		got.read = 0
		if got != want {
			t.Errorf("Read(%s) = %+v, want %+v", content, got, want)
		}
	}
}

func TestAnOpeningReadsOnFromWhereItGotTo(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s.jsonl")
	write := func(s string) {
		f, _ := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
		f.WriteString(s)
		f.Close()
	}
	write(`{"type":"user","entrypoint":"cli","message":{"content":"first"}}` + "\n" + `{"type":"user","message":{"content":"sec`)
	o := Opening{}.Read(path)
	if o.Prompt != "first" {
		t.Fatalf("prompt = %q, before the line being written ends", o.Prompt)
	}
	write(`ond"}}` + "\n" + `{"type":"user","message":{"content":"` + strings.Repeat("x", longestLine) + `"}}` + "\n")
	if o = o.Read(path); o.Prompt != "second" {
		t.Fatalf("prompt = %q, want the one whose line ended, past one too long to read", o.Prompt)
	}
	if fi, _ := os.Stat(path); o.read != fi.Size() {
		t.Fatalf("read %d of %d", o.read, fi.Size())
	}
}

func TestGoalFollowsWhatClaudeWritesForEachGoalStep(t *testing.T) {
	cases := []struct {
		name, line string
		want       proto.Goal
		ok         bool
	}{
		{"set", `{"type":"attachment","attachment":{"type":"goal_status","met":false,"sentinel":true,"condition":"tests pass"}}`, proto.Goal{Objective: "tests pass", Status: "active"}, true},
		{"unmet check", `{"type":"attachment","attachment":{"type":"goal_status","met":false,"condition":"tests pass","iterations":2}}`, proto.Goal{Objective: "tests pass", Status: "active"}, true},
		{"met", `{"type":"attachment","attachment":{"type":"goal_status","met":true,"condition":"tests pass","iterations":3}}`, proto.Goal{Objective: "tests pass", Status: "complete"}, true},
		{"given up", `{"type":"attachment","attachment":{"type":"goal_status","met":false,"failed":true,"condition":"tests pass"}}`, proto.Goal{Objective: "tests pass", Status: proto.GoalFailed}, true},
		{"cleared", `{"type":"system","subtype":"local_command","content":"<local-command-stdout>Goal cleared</local-command-stdout>","commandRun":{"command":"goal","args":"clear"}}`, proto.Goal{}, true},
		{"asked", `{"type":"system","subtype":"local_command","content":"<local-command-stdout>No goal set</local-command-stdout>","commandRun":{"command":"goal","args":""}}`, proto.Goal{}, false},
		{"other attachment", `{"type":"attachment","attachment":{"type":"todo_reminder"}}`, proto.Goal{}, false},
		{"subagent", `{"type":"attachment","isSidechain":true,"attachment":{"type":"goal_status","met":true,"condition":"x"}}`, proto.Goal{}, false},
	}
	for _, c := range cases {
		got, ok := Goal([]byte(c.line))
		if got != c.want || ok != c.ok {
			t.Errorf("%s: got %+v %v, want %+v %v", c.name, got, ok, c.want, c.ok)
		}
	}
}
