package proto

import (
	"bytes"
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata/golden")

func ptr[T any](v T) *T { return &v }

func summary() AgentSummary {
	return AgentSummary{ID: "a1", Title: "fix tests", Cwd: "/w", Provider: "claude", Status: "idle", Epoch: 1, MaxSeq: 3, ProviderSessionID: "s1", CreatedAt: 1, UpdatedAt: 2}
}

var serverGolden = map[string]any{
	"hello_ok":        NewHelloOK("h1", "mac"),
	"agent_list":      NewAgentList("l1", []AgentSummary{summary()}),
	"agent_list_push": NewAgentList("", nil),
	"agent_update":    NewAgentUpdate(summary()),
	"stream_user":     NewAgentStream("a1", 1, Item{ID: "i1", Seq: 1, Ts: 10, Kind: "user", Text: "hi"}),
	"stream_tool_running": NewAgentStream("a1", 1, Item{ID: "i2", Seq: 2, Ts: 11, Kind: "tool", Call: &ToolCall{
		ToolUseID: "t1", Name: "Bash", Status: "running", Detail: ToolDetail{Kind: "shell", Command: "ls"},
	}}),
	"stream_tool_done": NewAgentStream("a1", 1, Item{ID: "i2", Seq: 2, Ts: 11, Kind: "tool", Call: &ToolCall{
		ToolUseID: "t2", Name: "Edit", Status: "ok", Output: ptr("done"), DurationMs: ptr(int64(5)),
		Detail: ToolDetail{Kind: "edit", Path: "a.go", Diff: &FileDiff{Lines: []DiffLine{{Number: 1, Kind: "del", Text: "a"}, {Number: 1, Kind: "add", Text: "b"}}, Additions: 1, Deletions: 1}},
	}}),
	"stream_tool_other": NewAgentStream("a1", 1, Item{ID: "i3", Seq: 3, Ts: 12, Kind: "tool", Call: &ToolCall{
		ToolUseID: "t3", Name: "mcp__x", Status: "error", Detail: ToolDetail{Kind: "other", Name: "mcp__x", Input: json.RawMessage(`{"q":1}`)},
	}}),
	"stream_tasks":   NewAgentStream("a1", 1, Item{ID: "i4", Seq: 4, Ts: 13, Kind: "tasks", Tasks: []TaskItem{{Text: "x", Status: "in_progress"}}}),
	"stream_compact": NewAgentStream("a1", 1, Item{ID: "i5", Seq: 5, Ts: 14, Kind: "compact", Trigger: "manual"}),
	"stream_result":  NewAgentStream("a1", 1, Item{ID: "i6", Seq: 6, Ts: 15, Kind: "result", OK: false, Error: "interrupted"}),
	"timeline": NewAgentTimeline("t1", "a1", 1, []Item{
		{ID: "i1", Seq: 1, Ts: 10, Kind: "assistant", Text: "hey"},
		{ID: "i2", Seq: 2, Ts: 11, Kind: "result", OK: true, DurationMs: 5, Usage: &TurnUsage{InputTokens: 1, OutputTokens: 2}},
	}, false, 2),
	"permission_request": NewPermissionRequest(PermissionRequest{RequestID: "r1", AgentID: "a1", ToolName: "Bash", Detail: ToolDetail{Kind: "shell", Command: "rm x", Description: "remove"},
		Options: []PermissionOption{{ID: "always", Label: "Yes, and always allow access to /w"}}, Feedback: true}),
	"permission_resolved": NewPermissionResolved("r1", "allow"),
	"ack":                 NewAck("p1"),
	"error":               NewError("p1", "Unknown agent: zz"),
}

// TestServerGolden pins the exact JSON the phone decodes.
func TestServerGolden(t *testing.T) {
	for name, msg := range serverGolden {
		got, err := json.MarshalIndent(msg, "", "  ")
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		path := filepath.Join("testdata", "golden", "server", name+".json")
		if *update {
			if err := os.WriteFile(path, append(got, '\n'), 0o644); err != nil {
				t.Fatal(err)
			}
			continue
		}
		want, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("%s: %v (run with -update)", name, err)
		}
		if !bytes.Equal(bytes.TrimSpace(want), got) {
			t.Errorf("%s:\n got %s\nwant %s", name, got, want)
		}
	}
}

// TestClientGolden proves every message the phone may send decodes here.
func TestClientGolden(t *testing.T) {
	files, _ := filepath.Glob(filepath.Join("testdata", "golden", "client", "*.json"))
	if len(files) == 0 {
		t.Fatal("no client golden files")
	}
	for _, f := range files {
		raw, _ := os.ReadFile(f)
		if _, err := DecodeClient(raw); err != nil {
			t.Errorf("%s: %v", filepath.Base(f), err)
		}
	}
}

func TestDecodeClientRejects(t *testing.T) {
	for _, raw := range []string{
		`not json`,
		`{"type":"agent.list"}`,
		`{"type":"agent.create","id":"1","cwd":"/","profileId":"p","prompt":"x"}`,
		`{"type":"agent.prompt","id":"1","agentId":"a"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":501}`,
		`{"type":"permission.resolve","id":"1","requestId":"r","decision":"maybe"}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":0}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":-5}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":null}`,
		`{"type":"agent.prompt","id":"1","agentId":"a","text":null}`,
		`{"type":"agent.close","id":null,"agentId":"a"}`,
		`null`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestDecodeClientAcceptsWhatTheSchemaAccepts(t *testing.T) {
	for _, raw := range []string{
		`{"type":"agent.list","id":""}`,
		`{"type":"agent.close","id":"1","agentId":""}`,
		`{"type":"agent.list","id":"1","agentId":5}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","sinceSeq":1.5,"limit":2.5}`,
		`{"type":"hello","id":"1","token":"","clientId":"","protocolVersion":2.0}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != nil {
			t.Errorf("%s: %v", raw, err)
		}
	}
}
