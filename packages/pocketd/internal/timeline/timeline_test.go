package timeline

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func str(s string) *string { return &s }

func TestTextMergesUntilSomethingElseArrives(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "assistant_text", Text: "a"}, 1)
	it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "b"}, 2)
	if it.Seq != 1 || it.Text != "ab" {
		t.Fatalf("merge: %+v", it)
	}
	tl.Apply(Event{Kind: "thinking", Text: "hm"}, 3)
	it, _ = tl.Apply(Event{Kind: "assistant_text", Text: "c"}, 4)
	if it.Seq != 3 || it.ID != "i3" || it.Text != "c" {
		t.Fatalf("new item after thinking: %+v", it)
	}
}

func TestToolEndUpdatesTheStartItemWithoutMutatingIt(t *testing.T) {
	tl := New()
	start, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"ls"}`)}, 10)
	end, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true, Output: str("a.go")}, 15)
	if !ok || end.Seq != start.Seq || end.Call.Status != "ok" || *end.Call.Output != "a.go" || *end.Call.DurationMs != 5 {
		t.Fatalf("%+v", end.Call)
	}
	if start.Call.Status != "running" {
		t.Fatal("published start item was mutated")
	}
	if _, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "unknown"}, 16); ok {
		t.Fatal("unknown tool_end produced an item")
	}
}

func TestTodoWriteBecomesTasksAndSwallowsItsResult(t *testing.T) {
	tl := New()
	it, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "TodoWrite", Input: json.RawMessage(`{"todos":[{"content":"x","status":"in_progress"},{"content":"y","status":"weird"}]}`)}, 1)
	if it.Kind != "tasks" || it.Tasks[0].Status != "in_progress" || it.Tasks[1].Status != "pending" {
		t.Fatalf("%+v", it)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true}, 2); ok {
		t.Fatal("folded tool_end produced an item")
	}
}

func TestPrebuiltDetailWins(t *testing.T) {
	tl := New()
	d := proto.ToolDetail{Kind: "shell", Command: "go test"}
	it, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "c1", Name: "commandExecution", Detail: &d}, 1)
	if it.Call.Detail.Command != "go test" {
		t.Fatalf("%+v", it.Call.Detail)
	}
}

func TestStartEpochStopsMerging(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "assistant_text", Text: "a"}, 1)
	if tl.StartEpoch() != 1 {
		t.Fatal("epoch")
	}
	it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "b"}, 2)
	if it.Seq != 2 {
		t.Fatalf("merged across epochs: %+v", it)
	}
}

func TestClearDropsItemsButKeepsCounting(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash"}, 1)
	tl.Apply(Event{Kind: "assistant_text", Text: "a"}, 2)
	tl.Clear()
	if items, _ := tl.Page(0, 200); len(items) != 0 {
		t.Fatalf("items survived: %v", items)
	}
	if epoch, seq := tl.State(); epoch != 1 || seq != 2 {
		t.Fatalf("state: epoch %d seq %d", epoch, seq)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true}, 3); ok {
		t.Fatal("tool from the old conversation updated")
	}
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "b"}, 4); it.Seq != 3 || it.Text != "b" {
		t.Fatalf("after clear: %+v", it)
	}
}

func TestOutputIsClamped(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash"}, 1)
	it, _ := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true, Output: str(strings.Repeat("x", proto.ToolOutputLimit+10))}, 2)
	if !strings.HasSuffix(*it.Call.Output, "\n… truncated") {
		t.Fatal("not truncated")
	}
}

func TestPage(t *testing.T) {
	tl := New()
	for n := range 5 {
		tl.Apply(Event{Kind: "user", Text: string(rune('a' + n))}, int64(n))
	}
	items, older := tl.Page(0, 2)
	if len(items) != 2 || items[0].Seq != 4 || !older {
		t.Fatalf("tail page: %v %v", items, older)
	}
	items, older = tl.Page(3, 200)
	if len(items) != 2 || items[0].Seq != 4 || !older {
		t.Fatalf("since page: %v %v", items, older)
	}
	if _, max := tl.State(); max != 5 {
		t.Fatal("maxSeq")
	}
}

func TestPageWithoutRoomReturnsNothing(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "a"}, 1)
	for _, limit := range []int{0, -1} {
		if items, older := tl.Page(0, limit); len(items) != 0 || !older {
			t.Fatalf("limit %d: %v %v", limit, items, older)
		}
	}
}

func TestTodoWriteToleratesOddEntries(t *testing.T) {
	tl := New()
	it, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "TodoWrite", Input: json.RawMessage(`{"todos":[{"content":1,"status":"completed"},"x",null]}`)}, 1)
	want := []proto.TaskItem{{Text: "", Status: "completed"}, {Text: "", Status: "pending"}, {Text: "", Status: "pending"}}
	if it.Kind != "tasks" || !reflect.DeepEqual(it.Tasks, want) {
		t.Fatalf("%+v", it)
	}
}

func TestContinueStartsANewEpochPastTheOldSeq(t *testing.T) {
	tl := New()
	tl.Continue(3, 12)
	if epoch, seq := tl.State(); epoch != 4 || seq != 12 {
		t.Fatalf("state = %d, %d", epoch, seq)
	}
	it, _ := tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	if it.Seq != 13 || it.ID != "i13" {
		t.Fatalf("first item after continue: %+v", it)
	}
}

func TestASubagentsItemsCarryTheirParentAndPageOnlyWithSubagents(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "assistant_text", Text: "a"}, 1)
	sub, _ := tl.Apply(Event{Kind: "assistant_text", Text: "b", ParentToolUseID: "t1"}, 2)
	if sub.Seq != 2 || sub.Text != "b" || sub.ParentToolUseID != "t1" {
		t.Fatalf("a sub-agent's text merged into the agent's: %+v", sub)
	}
	if items, _ := tl.Page(0, 10); len(items) != 1 || items[0].Text != "a" {
		t.Fatalf("Page: %+v", items)
	}
	if items, _ := tl.PageWithSubagents(0, 10); len(items) != 2 {
		t.Fatalf("PageWithSubagents: %+v", items)
	}
}

func TestASubagentsToolStillEndsAfterTheNextPrompt(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "Read", Input: json.RawMessage(`{"file_path":"a.go"}`), ParentToolUseID: "t1"}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t3", Name: "Read", Input: json.RawMessage(`{"file_path":"b.go"}`)}, 1)
	tl.StartEpoch()
	if it, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t2", OK: true}, 2); !ok || it.Call.Status != "ok" {
		t.Fatalf("%+v", it)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t3", OK: true}, 2); ok {
		t.Fatal("the agent's own tool outlived its epoch")
	}
}

func TestABackgroundCommandEndsAfterTheNextPromptAndNamesWhatStopsIt(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"bun run dev","run_in_background":true}`)}, 1)
	if _, ok := tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "b1"}, 2); ok {
		t.Fatal("going to the background changed what shows")
	}
	tl.StartEpoch()
	stop, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "TaskStop", Input: json.RawMessage(`{"task_id":"b1"}`)}, 3)
	if d := stop.Call.Detail; d.Kind != "shell" || d.Command != "bun run dev" {
		t.Fatalf("stop: %+v", d)
	}
	it, ok := tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true}, 9)
	if !ok || it.Call.Status != "ok" || *it.Call.DurationMs != 8 {
		t.Fatalf("end: %+v", it)
	}
	for _, c := range []struct{ name, input string }{{"TaskStop", `{"task_id":"b9"}`}, {"BashOutput", `{"bash_id":"b9"}`}} {
		other, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: c.name, Name: c.name, Input: json.RawMessage(c.input)}, 10)
		if other.Call.Detail.Kind != "other" {
			t.Fatalf("%s on an unknown task: %+v", c.name, other.Call.Detail)
		}
	}
}

func TestAStopOnABackgroundAgentNamesItsTask(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Agent", Input: json.RawMessage(`{"description":"Standards review","run_in_background":true}`)}, 1)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "a1"}, 2)
	stop, _ := tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "TaskStop", Input: json.RawMessage(`{"task_id":"a1"}`)}, 3)
	want := proto.ToolDetail{Kind: "other", Name: "TaskStop", Input: json.RawMessage(`{"task_id":"a1"}`), Description: "Standards review"}
	if !reflect.DeepEqual(stop.Call.Detail, want) {
		t.Fatalf("stop: %+v", stop.Call.Detail)
	}
}

func TestOnlyAResumeSetsAnEndedBackgroundAgentRunningAgain(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Agent", Input: json.RawMessage(`{"description":"Review","run_in_background":true}`)}, 1)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "a1"}, 2)
	out := "Done."
	tl.Apply(Event{Kind: "tool_end", ToolUseID: "t1", OK: true, Output: &out}, 3)
	tl.StartEpoch()
	waiting := "Still waiting on tests."
	if _, ok := tl.Apply(Event{Kind: "tool_background", TaskID: "a1", Output: &waiting}, 4); ok {
		t.Fatal("a stray notification set the agent running")
	}
	it, ok := tl.Apply(Event{Kind: "tool_resumed", TaskID: "a1"}, 5)
	if !ok || it.Call.Status != "running" || it.Call.DurationMs != nil || *it.Call.Output != "Done." || tl.BackgroundAgents() != 1 {
		t.Fatalf("resumed: %+v", it.Call)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_resumed", TaskID: "a1"}, 6); ok {
		t.Fatal("resuming a running agent changed it")
	}
}

func TestABackgroundCallTheAgentStoppedEndsAtTheStopsResult(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"npx wrangler login","run_in_background":true}`)}, 1)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "b1"}, 2)
	tl.StartEpoch()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "TaskStop", Input: json.RawMessage(`{"task_id":"b1"}`)}, 3)
	tl.Apply(Event{Kind: "tool_end", ToolUseID: "t2", OK: true}, 4)
	it, ok := tl.Apply(Event{Kind: "tool_end", TaskID: "b1", OK: true}, 4)
	if !ok || it.Call.ToolUseID != "t1" || it.Call.Status != "ok" || *it.Call.DurationMs != 3 {
		t.Fatalf("stopped: %+v", it)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_end", TaskID: "b9", OK: true}, 5); ok {
		t.Fatal("an unknown task ended something")
	}
}

func TestAPromptIsUnansweredUntilTheAgentWritesAndASubAgentOrCompactionIsNoAnswer(t *testing.T) {
	tl := New()
	if tl.Unanswered() {
		t.Fatal("an empty timeline is unanswered")
	}
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "s1", Name: "Read", ParentToolUseID: "t1"}, 2)
	tl.Apply(Event{Kind: "compacted", Trigger: "auto"}, 3)
	if !tl.Unanswered() {
		t.Fatal("a sub-agent's tool or a compaction answered the prompt")
	}
	tl.Apply(Event{Kind: "assistant_text", Text: "Once"}, 4)
	if tl.Unanswered() {
		t.Fatal("a reply left the prompt unanswered")
	}
}

func TestASecondInterruptInARowEndsNothing(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	tl.Apply(Event{Kind: "result", Error: "interrupted"}, 2)
	if _, ok := tl.Apply(Event{Kind: "result", Error: "interrupted"}, 3); ok {
		t.Fatal("a turn ended twice")
	}
}

func TestATurnAnInterruptOrErrorEndedIsNotTimedAsDone(t *testing.T) {
	for _, end := range []Event{{Kind: "result", Error: "interrupted", DurationMs: 9000}, {Kind: "result", Error: "API Error: 500"}} {
		tl := New()
		tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
		tl.Apply(end, 2)
		if _, ok := tl.Apply(Event{Kind: "result", OK: true, DurationMs: 963584}, 3); ok {
			t.Errorf("%s: Claude's turn_duration ended the turn again", end.Error)
		}
		if items, _ := tl.Page(0, 10); len(items) != 2 || items[1].OK || items[1].DurationMs != end.DurationMs {
			t.Errorf("%s: got %+v", end.Error, items)
		}
	}
}

func TestAnInterruptAfterClaudeTimedTheTurnEndsItInterrupted(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	done, _ := tl.Apply(Event{Kind: "result", OK: true, DurationMs: 5000}, 2)
	end, ok := tl.Apply(Event{Kind: "result", Error: "interrupted"}, 3)
	if !ok || end.ID != done.ID || end.OK || end.Error != "interrupted" || end.DurationMs != 5000 {
		t.Fatalf("got %+v from %+v", end, done)
	}
	if items, _ := tl.Page(0, 10); len(items) != 2 {
		t.Fatalf("got %+v", items)
	}
}

func TestAnErrorAfterADoneTurnEndsATurnOfItsOwn(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	tl.Apply(Event{Kind: "result", OK: true}, 2)
	tl.Apply(Event{Kind: "result", Error: "You've hit your session limit"}, 3)
	tl.Apply(Event{Kind: "result", OK: true}, 4)
	if items, _ := tl.Page(0, 10); len(items) != 3 || !items[1].OK || items[2].OK {
		t.Fatalf("got %+v", items)
	}
}

func TestAnErrorClaudeRetriedBecomesANoticeInTheTurnItLeftGoing(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	failed, _ := tl.Apply(Event{Kind: "result", Error: "API Error: Your computer went to sleep mid-response."}, 2)
	notice, ok := tl.Apply(Event{Kind: "retried"}, 3)
	if !ok || notice.ID != failed.ID || notice.Kind != "notice" || notice.Error != failed.Error {
		t.Fatalf("got %+v from %+v", notice, failed)
	}
	tl.Apply(Event{Kind: "assistant_text", Text: "Picking up where I left off."}, 4)
	tl.Apply(Event{Kind: "result", OK: true, DurationMs: 9000}, 5)
	if items, _ := tl.Page(0, 10); len(items) != 4 || items[3].Kind != "result" || !items[3].OK {
		t.Fatalf("got %+v", items)
	}
}

func TestAnErrorClaudeRetriedBecomesANoticeThoughItsStreamDraftedTheRetryFirst(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	failed, _ := tl.Apply(Event{Kind: "result", Error: "API Error: Your computer went to sleep mid-response."}, 2)
	draft, _ := tl.Apply(Event{Kind: "assistant_draft", Text: "Picking", Draft: "2:0"}, 3)
	if notice, ok := tl.Apply(Event{Kind: "retried"}, 4); !ok || notice.ID != failed.ID || notice.Kind != "notice" {
		t.Fatalf("got %+v from %+v", notice, failed)
	}
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "Picking up where I left off."}, 4); it.ID != draft.ID {
		t.Fatalf("the retry's text left its draft: %+v", it)
	}
	tl.Apply(Event{Kind: "result", OK: true, DurationMs: 9000}, 5)
	items, _ := tl.Page(0, 10)
	if len(items) != 4 || items[1].Kind != "notice" || items[2].Text != "Picking up where I left off." || !items[3].OK {
		t.Fatalf("got %+v", items)
	}
}

func TestOnlyATurnAnErrorEndedIsRetried(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	if _, ok := tl.Apply(Event{Kind: "retried"}, 2); ok {
		t.Fatal("retried a prompt")
	}
	tl.Apply(Event{Kind: "result", Error: "interrupted"}, 3)
	if _, ok := tl.Apply(Event{Kind: "retried"}, 4); ok {
		t.Fatal("retried an interrupt")
	}
}

func TestANoticeSaysWhyTheAgentWentOn(t *testing.T) {
	tl := New()
	it, ok := tl.Apply(Event{Kind: "notice", Text: "Stop hook: tests fail"}, 1)
	if !ok || it.Kind != "notice" || it.Text != "Stop hook: tests fail" {
		t.Fatalf("got %+v", it)
	}
}

func TestPastedImagesJoinTheNewestPrompt(t *testing.T) {
	const images = "# Attached images\n\n- /tmp/1.png"
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "brief", ParentToolUseID: "t1"}, 1)
	if _, ok := tl.Apply(Event{Kind: "user_images", Text: images}, 1); ok {
		t.Fatal("images joined no prompt, or a sub-agent's")
	}
	for prompt, want := range map[string]string{"look": "look\n\n" + images, "": images} {
		sent, _ := tl.Apply(Event{Kind: "user", Text: prompt}, 2)
		got, ok := tl.Apply(Event{Kind: "user_images", Text: images}, 3)
		if !ok || got.ID != sent.ID || got.Text != want {
			t.Errorf("%q: got %+v", prompt, got)
		}
	}
}

func TestPastedImagesJoinOnlyThePromptTheyFollow(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "fix the screenshots"}, 1)
	tl.Apply(Event{Kind: "assistant_text", Text: "done"}, 2)
	tl.Apply(Event{Kind: "result", OK: true}, 3)
	// An image-only prompt, or a slash command typed with an image, writes no prompt of its own.
	if got, ok := tl.Apply(Event{Kind: "user_images", Text: "# Attached images\n\n- /tmp/6.webp"}, 4); ok {
		t.Fatalf("images joined an earlier turn's prompt: %+v", got)
	}
	if items, _ := tl.Page(0, 10); items[0].Text != "fix the screenshots" {
		t.Fatalf("got %+v", items[0])
	}
}

func TestADraftGrowsUntilTheTranscriptWritesItsBlock(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	think, _ := tl.Apply(Event{Kind: "thinking_draft", Text: "Let", Draft: "1:0"}, 2)
	tl.Apply(Event{Kind: "thinking_draft", Text: " me see", Draft: "1:0"}, 2)
	first, _ := tl.Apply(Event{Kind: "assistant_draft", Text: "Hel", Draft: "1:1"}, 3)
	grown, _ := tl.Apply(Event{Kind: "assistant_draft", Text: "lo", Draft: "1:1"}, 3)
	if grown.ID != first.ID || grown.Text != "Hello" {
		t.Fatalf("grown: %+v", grown)
	}
	second, _ := tl.Apply(Event{Kind: "assistant_draft", Text: "And", Draft: "2:0"}, 4)
	if second.ID == first.ID {
		t.Fatal("a new block merged into the last")
	}
	if it, _ := tl.Apply(Event{Kind: "thinking", Text: "Let me see."}, 5); it.ID != think.ID || it.Text != "Let me see." {
		t.Fatalf("thinking: %+v", it)
	}
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "Hello!"}, 5); it.ID != first.ID || it.Text != "Hello!" {
		t.Fatalf("first: %+v", it)
	}
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "And done."}, 6); it.ID != second.ID || it.Text != "And done." {
		t.Fatalf("second: %+v", it)
	}
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "More"}, 7); it.ID == second.ID {
		t.Fatal("text after the drafts merged into a written one")
	}
	if items, _ := tl.Page(0, 10); len(items) != 5 {
		t.Fatalf("%d items", len(items))
	}
}

func TestATurnsEndLeavesADraftTheTranscriptNeverWrote(t *testing.T) {
	tl := New()
	draft, _ := tl.Apply(Event{Kind: "assistant_draft", Text: "Half", Draft: "1:0"}, 1)
	tl.Apply(Event{Kind: "result", OK: true}, 2)
	if it, _ := tl.Apply(Event{Kind: "assistant_text", Text: "Next turn"}, 3); it.ID == draft.ID {
		t.Fatal("the next turn's text replaced the old draft")
	}
}

func TestASubagentsModelShowsOnItsCall(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Agent", Input: json.RawMessage(`{"description":"Look","model":"haiku"}`)}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "Bash", Input: json.RawMessage(`{"command":"ls"}`)}, 1)
	before, _ := tl.Page(0, 10)
	if before[0].Call.Detail.Model != "haiku" {
		t.Fatalf("from input: %+v", before[0].Call.Detail)
	}
	tl.StartEpoch()
	it, ok := tl.Apply(Event{Kind: "tool_model", ToolUseID: "t1", Name: "claude-haiku-4-5"}, 2)
	if !ok || it.Call.Detail.Model != "claude-haiku-4-5" || before[0].Call.Detail.Model != "haiku" {
		t.Fatalf("%+v", it)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_model", ToolUseID: "t1", Name: "claude-haiku-4-5"}, 3); ok {
		t.Fatal("the same model changed what shows")
	}
	if _, ok := tl.Apply(Event{Kind: "tool_model", ToolUseID: "t2", Name: "claude-haiku-4-5"}, 3); ok {
		t.Fatal("a command got a model")
	}
}

func TestASubagentsContextShowsOnItsCall(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Agent", Input: json.RawMessage(`{"description":"Look"}`)}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "Bash", Input: json.RawMessage(`{"command":"ls"}`)}, 1)
	it, ok := tl.Apply(Event{Kind: "tool_tokens", ToolUseID: "t1", Tokens: 32900}, 2)
	if !ok || it.Call.Detail.Tokens != 32900 {
		t.Fatalf("%+v", it)
	}
	if _, ok := tl.Apply(Event{Kind: "tool_tokens", ToolUseID: "t1", Tokens: 32900}, 3); ok {
		t.Fatal("the same count changed what shows")
	}
	if _, ok := tl.Apply(Event{Kind: "tool_tokens", ToolUseID: "t2", Tokens: 10}, 3); ok {
		t.Fatal("a command got a count")
	}
}

func TestLastingNamesTheBackgroundCallsAndCountsItsAgents(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"bun run dev","run_in_background":true}`)}, 1)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "b1"}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "Agent", Input: json.RawMessage(`{"description":"Look","run_in_background":true}`)}, 2)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t2", TaskID: "a2"}, 2)
	tl.StartEpoch()
	if got := tl.Lasting(); !reflect.DeepEqual(got, []string{"t1", "t2"}) || tl.BackgroundAgents() != 1 {
		t.Fatalf("%v, %d agents", got, tl.BackgroundAgents())
	}
	tl.Apply(Event{Kind: "tool_end", TaskID: "a2", OK: true}, 3)
	if got := tl.Lasting(); !reflect.DeepEqual(got, []string{"t1"}) || tl.BackgroundAgents() != 0 {
		t.Fatalf("%v, %d agents", got, tl.BackgroundAgents())
	}
}

func TestLastingAgentsNamesTheTasksOfBackgroundSubagentsOnly(t *testing.T) {
	tl := New()
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t1", Name: "Bash", Input: json.RawMessage(`{"command":"bun run dev","run_in_background":true}`)}, 1)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t1", TaskID: "b1"}, 1)
	tl.Apply(Event{Kind: "tool_start", ToolUseID: "t2", Name: "Agent", Input: json.RawMessage(`{"description":"Look","run_in_background":true}`)}, 2)
	tl.Apply(Event{Kind: "tool_background", ToolUseID: "t2", TaskID: "a2"}, 2)
	if got := tl.LastingAgents(); !reflect.DeepEqual(got, []string{"a2"}) {
		t.Fatalf("%v", got)
	}
	tl.Apply(Event{Kind: "tool_end", TaskID: "a2", OK: true}, 3)
	if got := tl.LastingAgents(); len(got) != 0 {
		t.Fatalf("%v", got)
	}
}
