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
	TailSubagents(ctx, filepath.Join(dir, "s1.jsonl"), nil, func(id string) bool { return id == "toolu_spawn" }, func(events []timeline.Event) {
		mu.Lock()
		got = append(got, events...)
		mu.Unlock()
	})

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
	synthetic := []byte(`{"type":"assistant","isSidechain":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"x"}]}}`)
	if got := MapSubagent(synthetic, "toolu_spawn"); len(got) != 1 {
		t.Fatalf("%+v", got)
	}
}
