package proto

import (
	"bytes"
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata/golden")

func ptr[T any](v T) *T { return &v }

func summary() AgentSummary {
	return AgentSummary{ID: "a1", TerminalID: "t1", Title: "fix tests", Cwd: "/w", Provider: "claude", Status: "idle", Attached: true, Epoch: 1, MaxSeq: 3, ProviderSessionID: "s1", CreatedAt: 1, UpdatedAt: 2,
		Project: "pocket", Worktree: "pocket", MainWorktree: true, Branch: "main", TokensUsed: 48000, ContextWindow: 1000000, Origin: "desktop"}
}

func with(f func(*AgentSummary)) AgentSummary {
	s := summary()
	f(&s)
	return s
}

var serverGolden = map[string]any{
	"hello_ok":            NewHelloOK("h1", "mac", 3, []string{"pair.v1"}),
	"agent_list":          NewAgentList("l1", []AgentSummary{summary()}),
	"agent_list_push":     NewAgentList("", nil),
	"agent_update":        NewAgentUpdate(summary()),
	"agent_update_failed": NewAgentUpdate(with(func(s *AgentSummary) { s.Status, s.Failed = "done", true })),
	"agent_update_compacting": NewAgentUpdate(with(func(s *AgentSummary) {
		s.Status, s.Compacting, s.Attached, s.ProviderSessionID = "working", true, false, ""
	})),
	"agent_update_restore_cleared": NewAgentUpdate(with(func(s *AgentSummary) { s.Status = "working" })),
	"agent_list_restored": NewAgentList("l1", []AgentSummary{
		with(func(s *AgentSummary) { s.ID, s.Restore = "a1", RestoreResumed }),
		with(func(s *AgentSummary) { s.ID, s.Restore = "a2", RestoreInterrupted }),
		with(func(s *AgentSummary) { s.ID, s.Restore = "a3", RestoreAccessLowered }),
		with(func(s *AgentSummary) { s.ID, s.Restore, s.Attached = "a4", RestoreFailed, false }),
	}),
	"error_agent_resuming": NewErrorCode("p1", CodeAgentResuming, "This session is still resuming"),
	"stream_user":          NewAgentStream("a1", 1, Item{ID: "i1", Seq: 1, Ts: 10, Kind: "user", Text: "hi"}),
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
	"agent_creating":      NewAgentCreating("c1", "r1", "t1", "/w/fix", true),
	"agent_created":       NewAgentCreated("c1", "r1", "a1", "t1"),
	"agent_progress":      NewAgentProgress("c1", "r1", StepFetch, "Couldn't fetch, using local main"),
	"agent_providers": NewAgentProviders("p1", []ProviderInfo{
		{ID: "claude", Available: true, Efforts: []string{"low", "medium", "high", "xhigh", "max"}, Plan: true},
		{ID: "codex", Available: false, Efforts: []string{}, Plan: false},
	}, "full", "ask"),
	"error_detail":  NewCodedError("create-r1", "spawn_failed", "Setup exited 1", "npm ERR! missing script: setup"),
	"hello_ok_host": helloOKHost(),
	"host_changed":  NewHostChanged(HostState{Tailnet: true, KeepingAwake: true}),
	"error_coded":   NewErrorCode("h1", "client_too_old", "Update Pocket on this phone"),
	"pair_ok":       NewPairOK("p1", "3fa9c1d2e5f60718293a4b5c6d7e8f90", "dG9rZW4tdG9rZW4tdG9rZW4tdG9rZW4tdG9rZW4tdG9"),
	"hello_ok_scopes": func() HelloOK {
		h := NewHelloOK("h1", "mac", 3, []string{"pair.v1", "scopes.v1"})
		h.Scopes = []string{"observe", "drive", "approve", "spawn", "owner"}
		return h
	}(),
	"pair_offer":         NewPairOffer("b1", "anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv", "abcdefghijklmnopqrstuv", 1790000000000),
	"pair_done":          NewPairDone("d1", "iPhone"),
	"error_scope_denied": NewErrorCode("r1", CodeScopeDenied, "permission.resolve needs approve; run it outside Pocket Terminals, or against a scratch pocketd (POCKETD_SOCK)"),
	"project_list": NewProjectList("p1", []Project{{Path: "/Users/me/dev/pocket", Name: "pocket", Worktrees: []Worktree{
		{Name: "pocket", Path: "/Users/me/dev/pocket", Branch: "main", IsMain: true},
		{Name: "calm-otter", Path: "/Users/me/.worktrees/pocket/calm-otter", Branch: ""},
	}}}),
}

func helloOKHost() HelloOK {
	ok := NewHelloOK("h1", "mac", 3, []string{CapHost})
	ok.Host = &HostState{Tailnet: true}
	return ok
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
		`{"type":"agent.view","id":"1"}`,
		`{"type":"agent.view","id":"1","agentIds":null}`,
		`{"type":"agent.seen","id":"1","agentIds":"a1"}`,
		`{"type":"agent.seen","id":"1","agentIds":[1]}`,
		`{"type":"agent.seen","id":"1","agentIds":["a1",null]}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"caps":"pair.v1"}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"caps":[1]}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"protocol":{"min":3}}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"protocol":{"min":3,"max":3.5}}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"protocol":null}`,
		`{"type":"pair","id":"1","name":"iPhone","platform":"ios"}`,
		`{"type":"pair","id":"1","code":"c","name":"iPhone","platform":"windows"}`,
		`{"type":"pair","id":"1","code":"c","platform":"ios"}`,
		`{"type":"project.list"}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w","new":{"name":"n"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"argv":["sh"]}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"gemini","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"prompt":"` + strings.Repeat("x", MaxPrompt+1) + `"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"ACCESS":"full"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"Worktree":"/w"},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","Base":"main"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w","new":null},"provider":"claude","access":"ask","plan":false,"model":null}}`,
		`{"type":"config.set","id":"1","key":"phone.maxAccess","value":"full"}`,
		`{"type":"config.set","id":"1","key":"theme","value":"ask"}`,
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
		`{"type":"agent.view","id":"1","agentIds":[]}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"full","plan":true,"prompt":"` + strings.Repeat("x", MaxPrompt) + `"}}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c","protocolVersion":3,"caps":[],"protocol":{"min":3,"max":3.0}}`,
		`{"type":"pair","id":"1","code":"c","name":"","platform":"android"}`,
		`{"type":"hello","id":"1","clientId":"desktop","protocolVersion":3}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != nil {
			t.Errorf("%s: %v", raw, err)
		}
	}
}
