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
