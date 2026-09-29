package claude

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

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
		{"slash command", `{"type":"user","message":{"content":"<command-name>/compact</command-name>"}}`, nil},
		{"local command", `{"type":"user","message":{"content":"<local-command-stdout>ok</local-command-stdout>"}}`, nil},
		{"meta", `{"type":"user","isMeta":true,"message":{"content":"x"}}`, nil},
		{"subagent", `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"x"}]}}`, nil},
		{"compact summary", `{"type":"user","isCompactSummary":true,"message":{"content":"x"}}`, nil},
		{"assistant", `{"type":"assistant","message":{"content":[{"type":"thinking","thinking":""},{"type":"thinking","thinking":"hm"},{"type":"text","text":"ok"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}`, []string{"thinking", "assistant_text", "tool_start"}},
		{"tool result", `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"a"}],"is_error":true}]}}`, []string{"tool_end"}},
		{"tool result and text", `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a"},{"type":"text","text":"hi"}]}}`, []string{"tool_end", "user"}},
		{"turn end", `{"type":"system","subtype":"turn_duration","durationMs":7}`, []string{"result"}},
		{"compact", `{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"auto"}}`, []string{"compacted"}},
		{"junk", `{"type":"summary"}`, nil},
	}
	for _, c := range cases {
		got, _ := Map([]byte(c.line))
		if !reflect.DeepEqual(kinds(got), c.want) {
			t.Errorf("%s: got %v want %v", c.name, kinds(got), c.want)
		}
	}
}

func TestMapDetails(t *testing.T) {
	ev, _ := Map([]byte(`{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"out","is_error":true}]}}`))
	if ev[0].ToolUseID != "t1" || ev[0].OK || *ev[0].Output != "out" {
		t.Errorf("tool_end: %+v", ev[0])
	}
	ev, _ = Map([]byte(`{"type":"user","message":{"content":"[Request interrupted by user for tool use]"}}`))
	if ev[0].OK || ev[0].Error != "interrupted" {
		t.Errorf("interrupt: %+v", ev[0])
	}
	ev, _ = Map([]byte(`{"type":"system","subtype":"turn_duration","durationMs":7}`))
	if !ev[0].OK || ev[0].DurationMs != 7 {
		t.Errorf("result: %+v", ev[0])
	}
	if _, title := Map([]byte(`{"type":"ai-title","aiTitle":"Fix tests"}`)); title != "Fix tests" {
		t.Errorf("title: %q", title)
	}
	if _, title := Map([]byte(`{"type":"summary","summary":"Fix tests","leafUuid":"u1"}`)); title != "Fix tests" {
		t.Errorf("summary title: %q", title)
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
