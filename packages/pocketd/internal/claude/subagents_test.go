package claude

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	"pocketd/internal/timeline"
)

func write(t *testing.T, path string, lines ...string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(strings.Join(lines, "\n")+"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestSidechainLinesCarryTheirTaskToolUseID(t *testing.T) {
	dir := t.TempDir()
	subs := filepath.Join(dir, "s1", "subagents")
	write(t, filepath.Join(subs, "agent-a1.meta.json"), `{"agentType":"Explore","description":"Find it","toolUseId":"toolu_spawn"}`)
	write(t, filepath.Join(subs, "agent-a1.jsonl"),
		`{"type":"user","isSidechain":true,"agentId":"a1","message":{"role":"user","content":"Find the parser"}}`,
		`{"type":"assistant","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"tool_use","id":"toolu_grep","name":"Grep","input":{"pattern":"parse"}}]}}`,
		`{"type":"user","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_grep","content":"parser.go"}]}}`,
		`{"type":"user","isSidechain":true,"isMeta":true,"agentId":"a1","message":{"content":"x"}}`,
		`{"type":"assistant","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"text","text":"It is in parser.go"}]}}`)
	write(t, filepath.Join(subs, "agent-fork.meta.json"), `{"agentType":"fork","isFork":true}`)
	write(t, filepath.Join(subs, "agent-fork.jsonl"), `{"type":"user","isSidechain":true,"message":{"content":"forked"}}`)
	write(t, filepath.Join(subs, "agent-other.meta.json"), `{"toolUseId":"toolu_unseen"}`)
	write(t, filepath.Join(subs, "agent-other.jsonl"), `{"type":"user","isSidechain":true,"message":{"content":"elsewhere"}}`)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	var mu sync.Mutex
	var got []timeline.Event
	collect := func(events []timeline.Event) {
		mu.Lock()
		got = append(got, events...)
		mu.Unlock()
	}
	agents := NewSubagents(filepath.Join(dir, "s1.jsonl"))
	agents.Spawned("toolu_spawn", collect)
	agents.Tail(ctx, nil, collect)

	if want := []string{"user", "tool_start", "tool_end", "assistant_text"}; !reflect.DeepEqual(kinds(got), want) {
		t.Fatalf("got %v want %v", kinds(got), want)
	}
	for _, e := range got {
		if e.ParentToolUseID != "toolu_spawn" {
			t.Errorf("%s has parent %q", e.Kind, e.ParentToolUseID)
		}
	}
	if got[0].Text != "Find the parser" || got[1].ToolUseID != "toolu_grep" || got[3].Text != "It is in parser.go" {
		t.Errorf("%+v", got)
	}
}

func TestTheMainTranscriptStillDropsSidechainLines(t *testing.T) {
	line := []byte(`{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"x"}]}}`)
	if got := Map(line); got != nil {
		t.Fatalf("%+v", got)
	}
	if got := MapSubagent(line, "toolu_spawn"); len(got) != 1 || got[0].ParentToolUseID != "toolu_spawn" {
		t.Fatalf("%+v", got)
	}
}

func TestASubagentsAnswerNamesItsModelOnTheParentsCall(t *testing.T) {
	line := []byte(`{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"x"}]}}`)
	got := MapSubagent(line, "toolu_spawn")
	if len(got) != 2 || !reflect.DeepEqual(got[0], timeline.Event{Kind: "tool_model", ToolUseID: "toolu_spawn", Name: "claude-haiku-4-5"}) || got[1].ParentToolUseID != "toolu_spawn" {
		t.Fatalf("%+v", got)
	}
	synthetic := []byte(`{"type":"assistant","isSidechain":true,"isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"Prompt is too long"}]}}`)
	if got := MapSubagent(synthetic, "toolu_spawn"); len(got) != 1 {
		t.Fatalf("%+v", got)
	}
}

func TestASubagentsAnswerCountsTheContextItWasGiven(t *testing.T) {
	line := []byte(`{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":3,"cache_creation_input_tokens":900,"cache_read_input_tokens":32000},"content":[{"type":"text","text":"x"}]}}`)
	got := MapSubagent(line, "toolu_spawn")
	if len(got) != 3 || !reflect.DeepEqual(got[1], timeline.Event{Kind: "tool_tokens", ToolUseID: "toolu_spawn", Tokens: 32903}) {
		t.Fatalf("%+v", got)
	}
}

func TestAForkThatReplaysTheCallThatSpawnedItStillEndsThatCall(t *testing.T) {
	tl := timeline.New()
	apply := func(events []timeline.Event) {
		for _, e := range events {
			tl.Apply(e, 1)
		}
	}
	apply([]timeline.Event{{Kind: "tool_start", ToolUseID: "toolu_spawn", Name: "Agent"}})
	apply(MapSubagent([]byte(`{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"toolu_spawn","name":"Agent","input":{"subagent_type":"fork"}}]}}`), "toolu_spawn"))
	apply(MapSubagent([]byte(`{"type":"user","isSidechain":true,"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_spawn","content":"Fork started — that fork is you."},{"type":"text","text":"Handle PR 4"}]}}`), "toolu_spawn"))
	apply([]timeline.Event{{Kind: "tool_background", ToolUseID: "toolu_spawn", TaskID: "a1"}})
	apply([]timeline.Event{{Kind: "tool_end", TaskID: "a1", OK: true}})

	items, _ := tl.PageWithSubagents(0, 100)
	var calls []string
	for _, it := range items {
		if it.Call != nil {
			calls = append(calls, it.Call.Status)
		}
	}
	if !reflect.DeepEqual(calls, []string{"ok"}) {
		t.Fatalf("calls %v", calls)
	}
}

func TestTailGoesOnFromTheWholeLinesSpawnedReadWithNoneTwice(t *testing.T) {
	dir := t.TempDir()
	subs := filepath.Join(dir, "s1", "subagents")
	write(t, filepath.Join(subs, "agent-a1.meta.json"), `{"toolUseId":"toolu_spawn"}`)
	line := func(text string) string {
		return `{"type":"user","isSidechain":true,"message":{"role":"user","content":"` + text + `"}}`
	}
	half := len(line("two")) / 2
	os.WriteFile(filepath.Join(subs, "agent-a1.jsonl"), []byte(line("one")+"\n"+line("two")[:half]), 0o644)

	var mu sync.Mutex
	var got []string
	collect := func(events []timeline.Event) {
		mu.Lock()
		defer mu.Unlock()
		for _, e := range events {
			got = append(got, e.Text)
		}
	}
	agents := NewSubagents(filepath.Join(dir, "s1.jsonl"))
	agents.Spawned("toolu_spawn", collect)
	agents.Spawned("toolu_spawn", collect)
	f, _ := os.OpenFile(filepath.Join(subs, "agent-a1.jsonl"), os.O_APPEND|os.O_WRONLY, 0)
	f.WriteString(line("two")[half:] + "\n" + line("three") + "\n")
	f.Close()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	agents.Tail(ctx, nil, collect)

	if want := []string{"one", "two", "three"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q want %q", got, want)
	}
}
