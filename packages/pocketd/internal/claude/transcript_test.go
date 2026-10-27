package claude

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strconv"
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
		{"typed prompt sent mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"hi","origin":{"kind":"human"}}}`, []string{"user"}},
		{"sub-agent hand-back mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"<agent-message from=\"a1\">\n[Subagent hand-back] report\n</agent-message>","origin":{"kind":"peer","from":"a1"}}}`, nil},
		{"goal continuation mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"Goal set: tests pass","origin":{"kind":"auto-continuation"}}}`, []string{"notice"}},
		{"other queued command", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"bash","prompt":"ls"}}`, nil},
		{"background agent launched", `{"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched successfully."}]}}`, []string{"tool_background"}},
		{"command sent to the background", `{"type":"user","toolUseResult":{"stdout":"","backgroundTaskId":"b1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Command running in background with ID: b1."}]}}`, []string{"tool_background"}},
		{"tool result as a string", `{"type":"user","toolUseResult":"Error: no such file","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"x","is_error":true}]}}`, []string{"tool_end"}},
		{"typed prompt", `{"type":"user","origin":{"kind":"human"},"message":{"content":"hi"}}`, []string{"user"}},
		{"meta", `{"type":"user","isMeta":true,"message":{"content":"x"}}`, nil},
		{"subagent", `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"x"}]}}`, nil},
		{"compact summary", `{"type":"user","isCompactSummary":true,"message":{"content":"x"}}`, nil},
		{"synthetic filler", `{"type":"assistant","message":{"model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}}`, nil},
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
	ev = Map([]byte(`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification><tool-use-id>t1</tool-use-id><status>killed</status><summary>Background command &quot;dev&quot; was stopped</summary></task-notification>"}}`))
	if !ev[0].OK || *ev[0].Output != `Background command "dev" was stopped` {
		t.Errorf("killed task: %+v", ev[0])
	}
	ev = Map([]byte(`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification><tool-use-id>t1</tool-use-id><status>stopped</status><summary>Agent \"Look\" was stopped by user</summary></task-notification>"}}`))
	if !ev[0].OK || *ev[0].Output != `Agent "Look" was stopped by user` {
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
	if len(ev) != 2 || ev[0].ToolUseID != "t2" || ev[1].Kind != "tool_end" || ev[1].TaskID != "b1" || ev[1].ToolUseID != "" || !ev[1].OK || *ev[1].Output != "Stopped" {
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

func TestACommandClaudeRunsItselfAnswersWithItsOutputAndOneThatAsksTheModelDoesNot(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"user","isMeta":true,"entrypoint":"sdk-cli","timestamp":"2026-10-20T09:02:03.940Z","message":{"content":"<local-command-caveat>The command below was run directly in Claude Code, not sent to you as a request.</local-command-caveat>"}}`,
		`{"type":"user","entrypoint":"sdk-cli","timestamp":"2026-10-20T09:02:03.939Z","message":{"content":"<command-name>/goal</command-name>\n            <command-message>goal</command-message>\n            <command-args>clear</command-args>"}}`,
		`{"type":"system","subtype":"local_command","entrypoint":"sdk-cli","timestamp":"2026-10-20T09:02:03.940Z","content":"<local-command-stdout>Goal cleared</local-command-stdout>","commandRun":{"command":"goal","args":"clear"}}`,
		`{"type":"user","entrypoint":"sdk-cli","timestamp":"2026-10-20T09:03:00.000Z","message":{"content":"<command-name>/goal</command-name>\n<command-args>tests pass</command-args>"}}`,
		`{"type":"system","subtype":"local_command","entrypoint":"sdk-cli","timestamp":"2026-10-20T09:03:00.000Z","content":"<local-command-stdout>Goal set: tests pass</local-command-stdout>","commandRun":{"command":"goal","args":"tests pass"}}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "assistant_text", "result", "user"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
	if got[0].Text != "/goal clear" || got[1].Text != "Goal cleared" || !got[2].OK {
		t.Fatalf("command %+v, reply %+v, end %+v", got[0], got[1], got[2])
	}
}

func TestAClaudePTurnThatFailsEndsWithTheErrorAlone(t *testing.T) {
	got := Map([]byte(`{"type":"assistant","entrypoint":"sdk-cli","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"There's an issue with the selected model."}]}}`))
	if len(got) != 1 || got[0].Kind != "result" || got[0].OK || got[0].Error != "There's an issue with the selected model." {
		t.Fatalf("got %+v", got)
	}
}

func TestAnAPIErrorFailsTheTurnWhateverStartedClaude(t *testing.T) {
	for _, entrypoint := range []string{"cli", "claude-desktop", "sdk-cli"} {
		got := Map([]byte(`{"type":"assistant","entrypoint":"` + entrypoint + `","isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error: 500 Internal server error."}]}}`))
		if len(got) != 1 || got[0].Kind != "result" || got[0].OK || got[0].Error != "API Error: 500 Internal server error." {
			t.Errorf("%s: got %+v", entrypoint, got)
		}
	}
}

func TestAUsageLimitEndsTheTurnAtTheAccountsLimit(t *testing.T) {
	got := Map([]byte(`{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","apiError":"usage_limit_reached","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 9:20am"}]}}`))
	if len(got) != 1 || got[0].Kind != "result" || got[0].OK || !got[0].Limit {
		t.Fatalf("got %+v", got)
	}
	other := Map([]byte(`{"type":"assistant","isApiErrorMessage":true,"error":"server_error","message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error: 500"}]}}`))
	if len(other) != 1 || other[0].Limit {
		t.Fatalf("other %+v", other)
	}
}

func TestAnInterruptedTurnShowsNoReplyClaudeFilledIn(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"user","entrypoint":"cli","message":{"content":"grep the colors"}}`,
		`{"type":"assistant","entrypoint":"cli","message":{"model":"claude-opus-5","content":[{"type":"text","text":"Searching"}]}}`,
		`{"type":"user","entrypoint":"cli","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}`,
		`{"type":"queue-operation","operation":"dequeue"}`,
		`{"type":"assistant","entrypoint":"cli","message":{"model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}}`,
		`{"type":"user","entrypoint":"cli","message":{"content":"continue"}}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "assistant_text", "result", "user"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
}

func TestACLIPromptShowsItsPastedImagesAndText(t *testing.T) {
	var turns Turns
	tl := timeline.New()
	for _, l := range []string{
		`{"type":"user","entrypoint":"cli","promptId":"p1","imagePasteIds":[1,2],"message":{"content":[{"type":"text","text":"Why is Save misaligned? [Image #1] [Image #2]\n\n<pasted_content id=\"6c20\">\nwarn: overflow\n</pasted_content id=\"6c20\">\n\nThe log is above."},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}}`,
		`{"type":"attachment","entrypoint":"cli","attachment":{"type":"total_tokens_reminder"}}`,
		`{"type":"user","entrypoint":"cli","promptId":"p1","isMeta":true,"message":{"content":[{"type":"text","text":"[Image: source: /tmp/s/images/1.png]"},{"type":"text","text":"[Image: source: /tmp/s/images/2.png]"}]}}`,
	} {
		for _, e := range turns.Map([]byte(l)) {
			tl.Apply(e, 1)
		}
	}
	items, _ := tl.Page(0, 10)
	want := "Why is Save misaligned? [Image #1] [Image #2]\n\nwarn: overflow\n\nThe log is above.\n\n# Attached images\n\n- /tmp/s/images/1.png\n- /tmp/s/images/2.png"
	if len(items) != 1 || items[0].Kind != "user" || items[0].Text != want {
		t.Fatalf("got %+v", items)
	}
}

func TestAPasteTagOnlyUnwrapsWhereItCloses(t *testing.T) {
	got := Map([]byte(`{"type":"user","message":{"content":"the bug report sits in <pasted_content id=\"c859\">, then"}}`))
	if len(got) != 1 || got[0].Text != `the bug report sits in <pasted_content id="c859">, then` {
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

// replay maps transcript lines in order onto a timeline, each prompt starting a new epoch as an agent does.
func replay(lines ...string) *timeline.Timeline {
	tl := timeline.New()
	var turns Turns
	for n, l := range lines {
		for _, e := range turns.Map([]byte(l)) {
			if e.Kind == "user" {
				tl.StartEpoch()
			}
			tl.Apply(e, int64(n))
		}
	}
	return tl
}

func callOf(t *testing.T, tl *timeline.Timeline, id string) proto.ToolCall {
	t.Helper()
	items, _ := tl.Page(0, 1000)
	for _, it := range items {
		if it.Call != nil && it.Call.ToolUseID == id {
			return *it.Call
		}
	}
	t.Fatalf("no call %s", id)
	return proto.ToolCall{}
}

func output(c proto.ToolCall) string {
	if c.Output == nil {
		return "<nil>"
	}
	return *c.Output
}

const (
	asyncAgent    = `{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Review the branch","run_in_background":true}}]}}`
	agentLaunched = `{"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched successfully."}]}}`
	nextPrompt    = `{"type":"user","message":{"content":"and the docs?"}}`
)

func notification(body string) string {
	b, _ := json.Marshal("<task-notification>\n" + body + "\n</task-notification>")
	return `{"type":"user","origin":{"kind":"task-notification"},"message":{"content":` + string(b) + `}}`
}

func TestANotificationNamingOnlyItsTaskEndsItsCallAfterTheNextPrompt(t *testing.T) {
	tl := replay(asyncAgent, agentLaunched, nextPrompt,
		notification("<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>No issues.</result>"))
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "No issues." {
		t.Fatalf("%+v %s", c, output(c))
	}
	if tl.Lasting() != nil || tl.BackgroundAgents() != 0 {
		t.Fatalf("still lasting: %v", tl.Lasting())
	}
}

func TestWhatSetClaudeGoingWithoutAPromptIsANotice(t *testing.T) {
	cases := []struct{ name, line, want string }{
		{"stop hook", `{"type":"user","isMeta":true,"message":{"content":"Stop hook feedback:\n[make test]: 3 tests fail.\nFix them first."}}`, "Stop hook: 3 tests fail."},
		{"goal check-in", `{"type":"user","isMeta":true,"message":{"content":"Goal check-in: «tests pass» is still active\nKeep going."}}`, "Goal check-in: «tests pass» is still active"},
		{"scheduled", `{"type":"user","isMeta":true,"turnOrigin":"scheduled","message":{"content":"Report progress\nin one line"}}`, "Scheduled: Report progress"},
		{"usage limit reset", `{"type":"user","isMeta":true,"message":{"content":"Your claude.ai usage limit has reset. Continue the task you were working on when the limit was reached; do not repeat work that is already complete."}}`, "Usage limit reset"},
		{"goal continuation", `{"type":"user","origin":{"kind":"auto-continuation"},"message":{"content":"Goal set: tests pass"}}`, "Goal set: tests pass"},
		{"goal continuation mid-turn", `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"Goal set: tests pass","origin":{"kind":"auto-continuation"}}}`, "Goal set: tests pass"},
		{"monitor event", notification("<task-id>m1</task-id>\n<summary>Monitor event: \"build log\"</summary>\n<event>warning: unused import\nin rows.rs</event>"), "Monitor: build log · warning: unused import"},
		{"goal check-in notification", notification("<summary>Goal check-in: background work still running</summary>"), "Goal check-in: background work still running"},
	}
	for _, c := range cases {
		got := Map([]byte(c.line))
		if len(got) != 1 || got[0].Kind != "notice" || got[0].Text != c.want {
			t.Errorf("%s: got %+v", c.name, got)
		}
	}
}

func TestAStopHookThatKeepsClaudeGoingEndsTheRunItStoppedAndTimesEachRunOnItsOwn(t *testing.T) {
	for _, entrypoint := range []string{"cli", "sdk-cli"} {
		var turns Turns
		var got []timeline.Event
		for _, l := range []string{
			`{"type":"user","timestamp":"2026-10-21T10:00:00.000Z","message":{"content":"migrate"}}`,
			`{"type":"assistant","timestamp":"2026-10-21T10:00:02.000Z","message":{"content":[{"type":"text","text":"Migrated."}]}}`,
			`{"type":"user","isMeta":true,"timestamp":"2026-10-21T10:00:03.000Z","message":{"content":"Stop hook feedback:\n[make test]: 3 tests fail."}}`,
			`{"type":"system","subtype":"stop_hook_summary","entrypoint":"` + entrypoint + `","timestamp":"2026-10-21T10:00:03.100Z"}`,
			`{"type":"assistant","timestamp":"2026-10-21T10:00:05.000Z","message":{"content":[{"type":"text","text":"Fixed them."}]}}`,
			`{"type":"system","subtype":"stop_hook_summary","entrypoint":"` + entrypoint + `","timestamp":"2026-10-21T10:00:07.000Z"}`,
			`{"type":"system","subtype":"turn_duration","durationMs":7000,"timestamp":"2026-10-21T10:00:07.000Z"}`,
		} {
			if entrypoint == "sdk-cli" && strings.Contains(l, "turn_duration") {
				continue
			}
			got = append(got, turns.Map([]byte(l))...)
		}
		if want := []string{"user", "assistant_text", "result", "notice", "assistant_text", "result"}; !reflect.DeepEqual(kinds(got), want) {
			t.Fatalf("%s: got %v want %v", entrypoint, kinds(got), want)
		}
		if got[2].DurationMs != 3000 || got[5].DurationMs != 4000 {
			t.Errorf("%s: ends %+v %+v", entrypoint, got[2], got[5])
		}
	}
}

func TestAnAPIErrorClaudeRetriesLeavesTheTurnGoing(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"user","timestamp":"2026-10-21T10:00:00.000Z","message":{"content":"fix the tests"}}`,
		`{"type":"assistant","timestamp":"2026-10-21T10:00:01.000Z","message":{"content":[{"type":"text","text":"Looking."}]}}`,
		`{"type":"assistant","isApiErrorMessage":true,"timestamp":"2026-10-21T10:00:02.000Z","message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error: Your computer went to sleep mid-response."}]}}`,
		`{"type":"attachment","timestamp":"2026-10-21T10:00:03.000Z","attachment":{"type":"todo_reminder"}}`,
		`{"type":"assistant","timestamp":"2026-10-21T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"make test"}}]}}`,
		`{"type":"system","subtype":"stop_hook_summary","entrypoint":"sdk-cli","timestamp":"2026-10-21T10:00:06.000Z"}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "assistant_text", "result", "retried", "tool_start", "result"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
	if got[5].DurationMs != 6000 {
		t.Fatalf("end %+v", got[5])
	}
}

func TestAnAPIErrorThatEndsTheTurnIsNotRetriedByTheNextOne(t *testing.T) {
	var turns Turns
	var got []timeline.Event
	for _, l := range []string{
		`{"type":"user","message":{"content":"fix the tests"}}`,
		`{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"API Error: 500"}]}}`,
		`{"type":"system","subtype":"turn_duration","durationMs":7}`,
		notification("<task-id>b1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>"),
		`{"type":"assistant","message":{"content":[{"type":"text","text":"The build finished."}]}}`,
	} {
		got = append(got, turns.Map([]byte(l))...)
	}
	if want := []string{"user", "result", "result", "tool_end", "assistant_text"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
}

const stillWorking = "<note>This agent stopped with background work of its own still running. It may resume on its own when that work completes or reports, and the same task-id notifies again if it does; the result below may be interim.</note>"

func TestAnAgentWaitingOnItsOwnBackgroundWorkRunsOnUntilItNotifiesAgain(t *testing.T) {
	for name, waiting := range map[string]string{
		"summary":          notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" is waiting on its own background work</summary>\n<result>Waiting for the e2e run to finish.</result>"),
		"note":             notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n" + stillWorking + "\n<result>Waiting for the e2e run to finish.</result>"),
		"not reported yet": notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n" + stillWorking + "\n<result>This agent has not reported yet: it is waiting on its own background work and will deliver its report through SubagentHandback when that finishes.\n</result>"),
	} {
		tl := replay(asyncAgent, agentLaunched, waiting, nextPrompt)
		if c := callOf(t, tl, "t1"); c.Status != "running" || tl.BackgroundAgents() != 1 || !reflect.DeepEqual(tl.Lasting(), []string{"t1"}) {
			t.Errorf("%s: waiting ended it: %+v", name, c)
		}
		tl = replay(asyncAgent, agentLaunched, waiting, nextPrompt,
			notification("<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>All green: the e2e run passed and lint is clean.</result>"))
		if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "All green: the e2e run passed and lint is clean." {
			t.Errorf("%s: %+v %s", name, c, output(c))
		}
	}
}

func TestAReportAnAgentGaveBeforeItsBackgroundWorkEndedStaysAboveAShorterLastWord(t *testing.T) {
	const report = "**APPROVED.** The design works on an auth-enabled deployment, and the two findings below don't block it."
	tl := replay(asyncAgent, agentLaunched,
		notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n"+stillWorking+"\n<result>"+report+"</result>"),
		nextPrompt,
		notification("<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>That notice is for a test run I stopped.</result>"))
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != report+"\n\nThat notice is for a test run I stopped." {
		t.Fatalf("%+v %q", c, output(c))
	}
}

func TestANotificationAfterACallEndedLeavesWhatItShowed(t *testing.T) {
	tl := replay(asyncAgent, agentLaunched,
		notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>No issues.</result>"),
		nextPrompt,
		notification("<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>That notice was a stale background command being killed, which changes nothing in my review.</result>"))
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "No issues." {
		t.Errorf("agent: %+v %q", c, output(c))
	}
	lines := append(backgroundBash("t2", "b2"),
		notification("<task-id>b2</task-id>\n<tool-use-id>t2</tool-use-id>\n<status>completed</status>\n<summary>Background command \"pnpm dev\" completed (exit code 0)</summary>"),
		nextPrompt,
		notification("<task-id>b2</task-id>\n<task-id>__orphan_summary__:shell</task-id>\n<status>stopped</status>\n<summary>1 background shell command task didn't finish before the previous session ended. Task ids: b2.</summary>"))
	tl = replay(lines...)
	if c := callOf(t, tl, "t2"); c.Status != "ok" || output(c) != "Background command \"pnpm dev\" completed (exit code 0)" {
		t.Errorf("command: %+v %q", c, output(c))
	}
}

func backgroundBash(id, task string) []string {
	return []string{
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"` + id + `","name":"Bash","input":{"command":"pnpm dev","run_in_background":true}}]}}`,
		`{"type":"user","toolUseResult":{"backgroundTaskId":"` + task + `"},"message":{"content":[{"type":"tool_result","tool_use_id":"` + id + `","content":"Command running in background with ID: ` + task + `."}]}}`,
	}
}

func TestAnOrphanSummaryEndsEveryTaskItNames(t *testing.T) {
	lines := append(backgroundBash("t1", "b1"), backgroundBash("t2", "b2")...)
	lines = append(lines, nextPrompt, notification("<task-id>b1</task-id>\n<task-id>b2</task-id>\n<task-id>__orphan_summary__:shell</task-id>\n<status>stopped</status>\n<summary>2 background shell command tasks didn't finish before the previous session ended. Task ids: b1, b2.</summary>"))
	tl := replay(lines...)
	for _, id := range []string{"t1", "t2"} {
		if c := callOf(t, tl, id); c.Status != "ok" {
			t.Errorf("%s: %+v", id, c)
		}
	}
	if tl.Lasting() != nil {
		t.Fatalf("still lasting: %v", tl.Lasting())
	}
}

func TestKillingEveryAgentEndsTheBackgroundAgentsAndLeavesCommandsRunning(t *testing.T) {
	lines := []string{asyncAgent, agentLaunched,
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Agent","input":{"description":"Done already","run_in_background":true}}]}}`,
		`{"type":"user","toolUseResult":{"isAsync":true,"agentId":"a2"},"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"Async agent launched successfully."}]}}`,
		notification("<task-id>a2</task-id>\n<tool-use-id>t2</tool-use-id>\n<status>failed</status>"),
	}
	lines = append(lines, backgroundBash("t3", "b3")...)
	lines = append(lines, nextPrompt,
		`{"type":"system","subtype":"agents_killed"}`,
		`{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"Background agent \"Review the branch\" was stopped by the user."}}`)
	tl := replay(lines...)
	if c := callOf(t, tl, "t1"); c.Status != "ok" || tl.BackgroundAgents() != 0 {
		t.Fatalf("agent: %+v", c)
	}
	if c := callOf(t, tl, "t2"); c.Status != "error" {
		t.Fatalf("an agent that had ended changed: %+v", c)
	}
	if got := tl.Lasting(); !reflect.DeepEqual(got, []string{"t3"}) {
		t.Fatalf("lasting %v", got)
	}
}

func resumeOf(task string) []string {
	return []string{
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"s-` + task + `","name":"SendMessage","input":{"to":"` + task + `","message":"Resume"}}]}}`,
		`{"type":"user","toolUseResult":{"success":true,"message":"Resuming agent ` + task + `","resumedAgentId":"` + task + `"},"message":{"content":[{"type":"tool_result","tool_use_id":"s-` + task + `","content":"{}"}]}}`,
	}
}

func failed(call, task string) string {
	return notification("<task-id>" + task + "</task-id>\n<tool-use-id>" + call + "</tool-use-id>\n<status>failed</status>\n<summary>Agent \"Review the branch\" failed: rate limit</summary>")
}

func TestAResumeSetsTheAgentGoingAgainAndAMessageToARunningOneDoesNot(t *testing.T) {
	ev := Map([]byte(resumeOf("a1")[1]))
	if len(ev) != 2 || ev[1].Kind != "tool_resumed" || ev[1].TaskID != "a1" {
		t.Fatalf("resume: %+v", ev)
	}
	ev = Map([]byte(`{"type":"user","toolUseResult":{"success":true,"message":"Message queued for the agent"},"message":{"content":[{"type":"tool_result","tool_use_id":"s1","content":"{}"}]}}`))
	if len(ev) != 1 || ev[0].Kind != "tool_end" {
		t.Fatalf("queued: %+v", ev)
	}
}

func TestAFailedAgentClaudeResumesRunsInTheBackgroundUntilItsNextNotification(t *testing.T) {
	lines := append([]string{asyncAgent, agentLaunched, failed("t1", "a1"), nextPrompt}, resumeOf("a1")...)
	tl := replay(lines...)
	if c := callOf(t, tl, "t1"); c.Status != "running" || c.DurationMs != nil || c.Output != nil || tl.BackgroundAgents() != 1 {
		t.Fatalf("resumed: %+v", c)
	}
	tl = replay(append(lines, nextPrompt, notification("<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>No issues.</result>"))...)
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "No issues." || tl.BackgroundAgents() != 0 {
		t.Fatalf("ended: %+v %q", c, output(c))
	}
}

func TestAResumedAgentsRunEndsItsCallThoughItsNotificationNamesTheResumingMessage(t *testing.T) {
	done := notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<result>Done.</result>")
	lines := append([]string{asyncAgent, agentLaunched, done, nextPrompt}, resumeOf("a1")...)
	lines = append(lines, notification("<task-id>a1</task-id>\n<tool-use-id>s-a1</tool-use-id>\n<status>completed</status>\n<result>Tests restored.</result>"))
	tl := replay(lines...)
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "Tests restored." || tl.BackgroundAgents() != 0 {
		t.Fatalf("still running: %+v %q", c, output(c))
	}
	if c := callOf(t, tl, "s-a1"); output(c) == "Tests restored." {
		t.Fatalf("the report went to the message that resumed the agent: %+v", c)
	}
}

func TestStopEndsEveryResumedAgentStillRunning(t *testing.T) {
	lines := []string{asyncAgent, agentLaunched, failed("t1", "a1"),
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Agent","input":{"description":"Docs","run_in_background":true}}]}}`,
		`{"type":"user","toolUseResult":{"isAsync":true,"agentId":"a2"},"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"Async agent launched successfully."}]}}`,
		failed("t2", "a2"), nextPrompt}
	lines = append(append(lines, resumeOf("a1")...), resumeOf("a2")...)
	lines = append(lines, notification("<task-id>a1</task-id>\n<status>completed</status>\n<result>Done.</result>"), `{"type":"system","subtype":"agents_killed"}`)
	tl := replay(lines...)
	if c := callOf(t, tl, "t2"); c.Status != "ok" || tl.BackgroundAgents() != 0 {
		t.Fatalf("still running: %+v", c)
	}
}

func TestABackgroundAgentsHandBackIsItsReportWhicheverArrivesFirst(t *testing.T) {
	const preamble = "[Subagent hand-back] The text below is the final report of a subagent this session delegated to. The harness indents every line of the report. The report follows:\n"
	body, _ := json.Marshal(preamble + "  ## Findings\n  \n  - one\n    - nested")
	handback := `{"type":"user","isMeta":true,"origin":{"kind":"peer","from":"a1","senderTaskId":"a1","handback":true,"body":` + string(body) + `},"message":{"content":"Another Claude session sent a message: ..."}}`
	queued := `{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","isMeta":true,"prompt":"<agent-message from=\"a1\">...</agent-message>","origin":{"kind":"peer","from":"a1","senderTaskId":"a1","handback":true,"body":` + string(body) + `,"handbackReportText":"## Findings\n\n- one\n  - nested"}}}`
	done := notification("<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"Review the branch\" finished</summary>\n<result>This agent's report was delivered to you as a message from \"a1\" (its SubagentHandback call). Read it there; it is not repeated here.\n</result>\n<usage><subagent_tokens>5</subagent_tokens></usage>")
	for name, lines := range map[string][]string{
		"hand-back first":       {asyncAgent, agentLaunched, handback, done},
		"notification first":    {asyncAgent, agentLaunched, done, handback},
		"queued mid-turn":       {asyncAgent, agentLaunched, queued, done},
		"after the next prompt": {asyncAgent, agentLaunched, done, nextPrompt, handback},
	} {
		tl := replay(lines...)
		if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "## Findings\n\n- one\n  - nested" {
			t.Errorf("%s: %+v %q", name, c, output(c))
		}
		items, _ := tl.Page(0, 100)
		for _, it := range items {
			if it.Kind == "user" && it.Text != "and the docs?" {
				t.Errorf("%s: the hand-back showed as a prompt", name)
			}
		}
	}
}

func TestASubAgentsReportLosesTheHarnessTrailerAndPreamble(t *testing.T) {
	const trailer = "agentId: a1 (use SendMessage with to: 'a1', summary: '<5-10 word recap>' to continue this agent)\n<usage>subagent_tokens: 67502\ntool_uses: 9\nduration_ms: 77279</usage>"
	for name, content := range map[string]string{
		"blocks":    `[{"type":"text","text":"First part."},{"type":"text","text":"Two judgement calls."},{"type":"text","text":` + strconv.Quote(trailer) + `}]`,
		"glued":     strconv.Quote("First part.\n\nTwo judgement calls.\n" + trailer),
		"hand-back": strconv.Quote("[Subagent hand-back] The text below is the final report of a subagent. The report follows:\n  First part.\n  \n  Two judgement calls.\n" + trailer),
	} {
		ev := Map([]byte(`{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":` + content + `}]}}`))
		if len(ev) != 1 || output(proto.ToolCall{Output: ev[0].Output}) != "First part.\n\nTwo judgement calls." {
			t.Errorf("%s: %+v", name, ev)
		}
	}
}

func TestAFinishedBackgroundCommandShowsWhatBecameOfItAndTheEndOfItsOutput(t *testing.T) {
	dir := t.TempDir()
	log := filepath.Join(dir, "b1.output")
	os.WriteFile(log, []byte(strings.Repeat("compiling\n", 1000)+"error[E0425]: cannot find value `x`\n"), 0o600)
	ev := Map([]byte(notification("<task-id>b1</task-id>\n<tool-use-id>t1</tool-use-id>\n<output-file>" + log + "</output-file>\n<status>failed</status>\n<summary>Background command \"cargo test 2&gt;&amp;1\" failed with exit code 101</summary>")))
	out := output(proto.ToolCall{Output: ev[0].Output})
	if ev[0].OK || !strings.HasPrefix(out, "Background command \"cargo test 2>&1\" failed with exit code 101\n\ncompiling\n") || !strings.HasSuffix(out, "compiling\nerror[E0425]: cannot find value `x`") || len(out) > 2200 {
		t.Fatalf("%q", out)
	}
	// An agent's output file links to its transcript, which is no output.
	transcript := filepath.Join(dir, "agent-a1.jsonl")
	os.WriteFile(transcript, []byte(`{"type":"user"}`+"\n"), 0o600)
	os.Symlink(transcript, filepath.Join(dir, "a1.output"))
	ev = Map([]byte(notification("<task-id>a1</task-id>\n<tool-use-id>t2</tool-use-id>\n<output-file>" + filepath.Join(dir, "a1.output") + "</output-file>\n<status>failed</status>\n<summary>Agent \"Review\" failed: API Error</summary>")))
	if out := output(proto.ToolCall{Output: ev[0].Output}); out != "Agent \"Review\" failed: API Error" {
		t.Fatalf("%q", out)
	}
}

func TestAMonitorWatchesInTheBackgroundUntilItsStreamEndsOrItExpires(t *testing.T) {
	monitor := func(id, task string) []string {
		return []string{
			`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"` + id + `","name":"Monitor","input":{"command":"tail -f /tmp/build.log","description":"build log","timeout_ms":1800000}}]}}`,
			`{"type":"user","toolUseResult":{"taskId":"` + task + `","timeoutMs":1800000,"persistent":false},"message":{"content":[{"type":"tool_result","tool_use_id":"` + id + `","content":"Monitor started (task ` + task + `, expires in 30m)."}]}}`,
		}
	}
	lines := append(monitor("t1", "m1"), monitor("t2", "m2")...)
	lines = append(lines, nextPrompt, notification("<task-id>m1</task-id>\n<summary>Monitor event: \"build log\"</summary>\n<event>warning: unused</event>"))
	tl := replay(lines...)
	if c := callOf(t, tl, "t1"); c.Status != "running" || c.Detail.Kind != "shell" || c.Detail.Command != "tail -f /tmp/build.log" {
		t.Fatalf("watching: %+v", c)
	}
	if got := tl.Lasting(); !reflect.DeepEqual(got, []string{"t1", "t2"}) {
		t.Fatalf("lasting %v", got)
	}
	lines = append(lines,
		notification("<task-id>m1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Monitor \"build log\" stream ended</summary>"),
		notification("<task-id>m2</task-id>\n<summary>Monitor event: \"build log\"</summary>\n<event>[Monitor expired after 30m with 1 event delivered. Re-arm it if you still need the watch.]</event>"))
	tl = replay(lines...)
	if c := callOf(t, tl, "t1"); c.Status != "ok" || output(c) != "Monitor \"build log\" stream ended" {
		t.Fatalf("ended: %+v %q", c, output(c))
	}
	if c := callOf(t, tl, "t2"); c.Status != "ok" || !strings.HasPrefix(output(c), "[Monitor expired after 30m") {
		t.Fatalf("expired: %+v %q", c, output(c))
	}
	if tl.Lasting() != nil {
		t.Fatalf("still lasting: %v", tl.Lasting())
	}
}
