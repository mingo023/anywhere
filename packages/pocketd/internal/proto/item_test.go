package proto

import (
	"encoding/json"
	"reflect"
	"testing"
)

func TestAnItemReadsBackAsItWasWrittenWhateverItsKind(t *testing.T) {
	ms := int64(0)
	out := "ok"
	for _, it := range []Item{
		{ID: "u", Seq: 1, Ts: 2, Kind: "user", Text: "hi", ParentToolUseID: "t0"},
		{ID: "c", Seq: 2, Kind: "tool", Call: &ToolCall{ToolUseID: "t1", Name: "Bash", Detail: ToolDetail{Kind: "shell", Command: "ls"}, Status: "done", Output: &out, DurationMs: &ms}},
		{ID: "k", Seq: 3, Kind: "tasks", Tasks: []TaskItem{{Text: "write", Status: "done"}}},
		{ID: "p", Seq: 4, Kind: "compact", Trigger: "auto"},
		{ID: "r", Seq: 5, Kind: "result", OK: true, DurationMs: 9, Usage: &TurnUsage{InputTokens: 1, OutputTokens: 2}},
		{ID: "n", Seq: 6, Kind: "notice", Text: "note", Error: "bad"},
		{ID: "d", Seq: 7, Kind: "delivery", Delivery: &Delivery{TaskIDs: []string{"a"}, ChildIDs: []string{"c1"}, Titles: []string{"Tests"}, Agents: []string{"codex"}, Outcome: "asks", Question: "Which?"}},
		{ID: "h", Seq: 8, Kind: "host_link", HostLink: &HostLink{EnvironmentID: "devbox", Hint: "dev", Reason: "run there"}},
	} {
		b, err := json.Marshal(it)
		if err != nil {
			t.Fatal(err)
		}
		var got Item
		if err := json.Unmarshal(b, &got); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got, it) {
			t.Errorf("%s reads back as %+v, want %+v", it.Kind, got, it)
		}
	}
}
