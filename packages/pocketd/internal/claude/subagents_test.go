package claude

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	"pocketd/internal/proto"
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

func TestASubagentsNarrationShowsAsText(t *testing.T) {
	line := []byte(`{"type":"assistant","isSidechain":true,"thinkingDisplay":"updates","message":{"content":[{"type":"thinking","thinking":"Reading the parser."}]}}`)
	got := MapSubagent(line, "toolu_spawn")
	if len(got) != 1 || got[0].Kind != "assistant_text" || got[0].Text != "Reading the parser." {
		t.Fatalf("%+v", got)
	}
}

// writeWorkflow lays out a session whose Workflow call toolu_wf launched run wf_1: probe-1 finished, probe-2 still runs.
func writeWorkflow(t *testing.T, dir string) (transcript string) {
	transcript = filepath.Join(dir, "s1.jsonl")
	write(t, transcript,
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_wf","name":"Workflow","input":{"script":"export const meta = {}"}}]}}`,
		`{"type":"user","toolUseResult":{"status":"async_launched","taskId":"w1","taskType":"local_workflow","runId":"wf_1"},"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_wf","content":"Workflow launched in background."}]}}`)
	run := filepath.Join(dir, "s1", "subagents", "workflows", "wf_1")
	write(t, filepath.Join(run, "journal.jsonl"),
		`{"type":"launched"}`,
		`{"type":"started","agentId":"a1","label":"probe-1","phase":"Probe"}`,
		`{"type":"started","agentId":"a2","label":"probe-2","phase":"Probe"}`,
		`{"type":"result","agentId":"a1","result":"ok"}`)
	write(t, filepath.Join(run, "agent-a1.meta.json"), `{"agentType":"workflow-subagent","description":"probe-1","workflowPhase":"Probe"}`)
	write(t, filepath.Join(run, "agent-a1.jsonl"),
		`{"type":"user","isSidechain":true,"agentId":"a1","message":{"role":"user","content":"Run sleep"}}`,
		`{"type":"assistant","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"text","text":"ok"}]}}`)
	write(t, filepath.Join(run, "agent-a2.meta.json"), `{"agentType":"workflow-subagent","description":"probe-2","workflowPhase":"Probe"}`)
	write(t, filepath.Join(run, "agent-a2.jsonl"), `{"type":"user","isSidechain":true,"agentId":"a2","message":{"role":"user","content":"Run echo"}}`)
	return transcript
}

// follow plays the daemon's part: a call's sub-agents are read once it is in, and a workflow's launch names its run.
func follow(subs *Subagents, got *[]timeline.Event, mu *sync.Mutex) func([]timeline.Event) {
	var record func([]timeline.Event)
	record = func(events []timeline.Event) {
		for _, e := range events {
			mu.Lock()
			*got = append(*got, e)
			mu.Unlock()
			if e.Kind == "tool_start" {
				subs.Spawned(e.ToolUseID, record)
			}
			if e.Kind == "tool_background" && e.Run != "" {
				subs.Workflow(e.ToolUseID, e.Run)
			}
		}
	}
	return record
}

func TestAWorkflowsAgentsRunAsSubAgentsUnderItsCall(t *testing.T) {
	transcript := writeWorkflow(t, t.TempDir())
	subs := NewSubagents(transcript)
	var mu sync.Mutex
	var got []timeline.Event
	record := follow(subs, &got, &mu)
	record([]timeline.Event{{Kind: "tool_background", ToolUseID: "toolu_wf", TaskID: "w1", Run: "wf_1"}})
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	subs.Tail(ctx, nil, record)

	type row struct{ Kind, ToolUseID, Parent, Text string }
	var rows []row
	for _, e := range got[1:] {
		r := row{e.Kind, e.ToolUseID, e.ParentToolUseID, e.Text}
		if e.Detail != nil {
			r.Text = e.Detail.Description + "/" + e.Detail.Phase
		}
		if e.Output != nil {
			r.Text = *e.Output
		}
		rows = append(rows, r)
	}
	want := []row{
		{"tool_start", "a1", "toolu_wf", "probe-1/Probe"},
		{"user", "", "a1", "Run sleep"},
		{"assistant_text", "", "a1", "ok"},
		{"tool_start", "a2", "toolu_wf", "probe-2/Probe"},
		{"user", "", "a2", "Run echo"},
		{"tool_end", "a1", "toolu_wf", "ok"},
	}
	if !reflect.DeepEqual(rows, want) {
		t.Fatalf("got  %+v\nwant %+v", rows, want)
	}
}

func TestARestoredWorkflowAddsOnlyTheAgentsItsTimelineDoesNotShow(t *testing.T) {
	transcript := writeWorkflow(t, t.TempDir())
	tl := timeline.New()
	apply := func(events []timeline.Event) {
		for _, e := range events {
			tl.Apply(e, 1)
		}
	}
	// What a timeline showed when pocketd stopped: the run's first agent, started and finished.
	apply([]timeline.Event{
		{Kind: "tool_start", ToolUseID: "toolu_wf", Name: "Workflow"},
		{Kind: "tool_background", ToolUseID: "toolu_wf", TaskID: "w1"},
		{Kind: "tool_start", ToolUseID: "a1", Name: "Agent", Detail: &proto.ToolDetail{Kind: "task", Description: "probe-1", Phase: "Probe"}, ParentToolUseID: "toolu_wf"},
		{Kind: "user", Text: "Run sleep", ParentToolUseID: "a1"},
		{Kind: "assistant_text", Text: "ok", ParentToolUseID: "a1"},
		{Kind: "tool_end", ToolUseID: "a1", OK: true, Output: new("ok")},
	})
	shown, _ := tl.PageWithSubagents(0, 100)

	subs := NewSubagents(transcript)
	var mu sync.Mutex
	var got []timeline.Event
	record := follow(subs, &got, &mu)
	subs.Restored(shown, record)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	subs.Tail(ctx, nil, record)

	var added []string
	for _, e := range got {
		added = append(added, e.Kind+" "+e.ToolUseID+e.ParentToolUseID)
	}
	if want := []string{"tool_start a2toolu_wf", "user a2"}; !reflect.DeepEqual(added, want) {
		t.Fatalf("got %q want %q", added, want)
	}
}

func TestARestoredAgentStoppedWithItsWorkflowTakesItsResultFromTheJournal(t *testing.T) {
	transcript := writeWorkflow(t, t.TempDir())
	tl := timeline.New()
	for _, e := range []timeline.Event{
		{Kind: "tool_start", ToolUseID: "toolu_wf", Name: "Workflow"},
		{Kind: "tool_start", ToolUseID: "a1", Name: "Agent", Detail: &proto.ToolDetail{Kind: "task"}, ParentToolUseID: "toolu_wf"},
		{Kind: "tool_end", ToolUseID: "a1", OK: true, Output: new(timeline.WorkflowStopped)},
		{Kind: "tool_start", ToolUseID: "a2", Name: "Agent", Detail: &proto.ToolDetail{Kind: "task"}, ParentToolUseID: "toolu_wf"},
	} {
		tl.Apply(e, 1)
	}
	shown, _ := tl.PageWithSubagents(0, 100)
	subs := NewSubagents(transcript)
	var mu sync.Mutex
	var got []timeline.Event
	record := follow(subs, &got, &mu)
	subs.Restored(shown, record)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	subs.Tail(ctx, nil, record)
	var ends []string
	for _, e := range got {
		if e.Kind == "tool_end" {
			ends = append(ends, e.ToolUseID+" "+*e.Output)
		}
	}
	if want := []string{"a1 ok"}; !reflect.DeepEqual(ends, want) {
		t.Fatalf("got %q want %q", ends, want)
	}
}

func TestAnAgentsResultIsItsTextOrTheJSONItAnsweredWith(t *testing.T) {
	subs := NewSubagents(filepath.Join(t.TempDir(), "s.jsonl"))
	for line, want := range map[string]string{
		`{"type":"result","agentId":"a1","result":"found it"}`:         "found it",
		`{"type":"result","agentId":"a1","result":{"files":["a.go"]}}`: "{\n  \"files\": [\n    \"a.go\"\n  ]\n}",
	} {
		got := subs.journaled([]byte(line), "wf")
		if len(got) != 1 || *got[0].Output != want {
			t.Fatalf("%s: got %+v", line, got)
		}
	}
}
