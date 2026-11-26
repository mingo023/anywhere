package proto

import (
	"bytes"
	"encoding/json"
	"flag"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/providers"
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

// providerInfos is every provider as agent.providers sends it, with codex not installed.
func providerInfos() []ProviderInfo {
	var list []ProviderInfo
	for _, p := range providers.All {
		list = append(list, ProviderInfo{ID: p.ID, Available: p != &providers.Codex, Efforts: p.Efforts, Plan: p.Plan, NeverAsks: p.NeverAsks, Chat: p.Chat})
	}
	return list
}

var serverGolden = map[string]any{
	"hello_ok":            NewHelloOK("h1", "mac", 3, []string{"pair.v1"}),
	"agent_list":          NewAgentList("l1", []AgentSummary{summary()}),
	"agent_list_push":     NewAgentList("", nil),
	"agent_update":        NewAgentUpdate(summary()),
	"agent_update_failed": NewAgentUpdate(with(func(s *AgentSummary) { s.Status, s.Failed, s.Error = "done", true, "API Error: 529 overloaded" })),
	"agent_update_compacting": NewAgentUpdate(with(func(s *AgentSummary) {
		s.Status, s.Compacting, s.Attached, s.ProviderSessionID = "working", true, false, ""
	})),
	"agent_update_restore_cleared": NewAgentUpdate(with(func(s *AgentSummary) { s.Status = "working" })),
	"agent_update_model":           NewAgentUpdate(with(func(s *AgentSummary) { s.Model, s.Effort, s.Access = "claude-opus-5-5", "high", "edits" })),
	"agent_update_options":         NewAgentUpdate(with(func(s *AgentSummary) { s.Options = map[string]string{"fast": "true"} })),
	"agent_update_pinned":          NewAgentUpdate(with(func(s *AgentSummary) { s.Pinned = true })),
	"agent_update_activity":        NewAgentUpdate(with(func(s *AgentSummary) { s.Status, s.Activity = "needsYou", "Approve Bash: rm marker" })),
	"agent_update_account":         NewAgentUpdate(with(func(s *AgentSummary) { s.Account = "3f9a0c1b2d4e" })),
	"agent_update_switching":       NewAgentUpdate(with(func(s *AgentSummary) { s.Account = "3f9a0c1b2d4e"; s.Switching = "builtin" })),
	"agent_update_resets":          NewAgentUpdate(with(func(s *AgentSummary) { s.Resets, s.ProviderSessionID = 1, "s2" })),
	"agent_update_chat_only":       NewAgentUpdate(with(func(s *AgentSummary) { s.ChatOnly = true })),
	"agent_update_recap": NewAgentUpdate(with(func(s *AgentSummary) {
		s.Status, s.Recap = "done", Recap{Asked: "fix it", Read: 2, Edited: 1, Said: "Fixed."}
	})),
	"agent_update_goal": NewAgentUpdate(with(func(s *AgentSummary) {
		s.Status, s.Goal = "working", Goal{Objective: "tests pass", Status: "active", TokensUsed: 16510, TokenBudget: 50000}
	})),
	"agent_commands": NewAgentCommands("p9", "a1", []Command{
		{Name: "goal", Description: "Set a goal — keep working until the condition is met"},
		{Name: "review", Description: "Review a pull request", ArgumentHint: "<pr>"},
	}),
	"skills_list": NewSkillsList("s1", "claude", "3f9a0c1b2d4e", "/Users/me/dev/pocket", []Command{
		{Name: "fix-root-cause", Description: "Trace each symptom to its root cause"},
		{Name: "skill-creator:skill-creator", ArgumentHint: "<skill>"},
	}),
	"agent_btw":                NewAgentBtw("b1", "a1", "It reads the config once at start."),
	"agent_remote_control":     NewAgentRemoteControl("r1", "a1", "https://claude.ai/code/session_01La8mPYSeyy7PpSsj7a8Yjq"),
	"agent_remote_control_off": NewAgentRemoteControl("r2", "a1", ""),
	"agent_mcp": NewAgentMcp("m1", "a1", []McpServer{
		{Name: "fff", Status: "connected", Scope: "user"},
		{Name: "claude.ai Linear", Status: "needsAuth", Scope: "claude.ai"},
	}, ""),
	"agent_mcp_auth": NewAgentMcp("m2", "a1", []McpServer{{Name: "linear", Status: "needsAuth", Scope: "user"}}, "https://mcp.linear.app/authorize?state=s1"),
	"agent_list_restored": NewAgentList("l1", []AgentSummary{
		with(func(s *AgentSummary) { s.ID, s.Restore = "a1", RestoreResumed }),
		with(func(s *AgentSummary) { s.ID, s.Restore = "a2", RestoreInterrupted }),
		with(func(s *AgentSummary) { s.ID, s.Restore = "a3", RestoreAccessLowered }),
		with(func(s *AgentSummary) { s.ID, s.Restore, s.Attached = "a4", RestoreFailed, false }),
	}),
	"error_agent_resuming": NewErrorCode("p1", CodeAgentResuming, "This session is still resuming"),
	"stream_user":          NewAgentStream("a1", 1, Item{ID: "i1", Seq: 1, Ts: 10, Kind: "user", Text: "hi"}),
	"stream_user_wake":     NewAgentStream("a1", 1, Item{ID: "i1", Seq: 1, Ts: 10, Kind: "user", Text: "Delegated task t1 reached a terminal state.", Origin: "wake"}),
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
	"stream_tool_described": NewAgentStream("a1", 1, Item{ID: "i3", Seq: 3, Ts: 12, Kind: "tool", Call: &ToolCall{
		ToolUseID: "t3", Name: "TaskStop", Status: "running", Detail: ToolDetail{Kind: "other", Name: "TaskStop", Input: json.RawMessage(`{"task_id":"a1"}`), Description: "Standards review"},
	}}),
	"stream_subagent": NewAgentStream("a1", 1, Item{ID: "i7", Seq: 7, Ts: 16, Kind: "tool", ParentToolUseID: "t4", Call: &ToolCall{
		ToolUseID: "t5", Name: "Read", Status: "running", Detail: ToolDetail{Kind: "read", Path: "a.go"},
	}}),
	"stream_workflow_agent": NewAgentStream("a1", 1, Item{ID: "i9", Seq: 9, Ts: 18, Kind: "tool", ParentToolUseID: "t6", Call: &ToolCall{
		ToolUseID: "a7", Name: "Agent", Status: "running", Detail: ToolDetail{Kind: "task", Description: "probe-1", Model: "claude-haiku-4-5", Phase: "Probe"},
	}}),
	"stream_tasks":        NewAgentStream("a1", 1, Item{ID: "i4", Seq: 4, Ts: 13, Kind: "tasks", Tasks: []TaskItem{{Text: "x", Status: "in_progress"}}}),
	"stream_compact":      NewAgentStream("a1", 1, Item{ID: "i5", Seq: 5, Ts: 14, Kind: "compact", Trigger: "manual"}),
	"stream_result":       NewAgentStream("a1", 1, Item{ID: "i6", Seq: 6, Ts: 15, Kind: "result", OK: false, Error: "interrupted"}),
	"stream_notice":       NewAgentStream("a1", 1, Item{ID: "i7", Seq: 7, Ts: 16, Kind: "notice", Text: "Stop hook: 3 tests fail"}),
	"stream_notice_error": NewAgentStream("a1", 1, Item{ID: "i8", Seq: 8, Ts: 17, Kind: "notice", Error: "API Error: 500"}),
	"timeline": NewAgentTimeline("t1", "a1", 1, []Item{
		{ID: "i1", Seq: 1, Ts: 10, Kind: "assistant", Text: "hey"},
		{ID: "i2", Seq: 2, Ts: 11, Kind: "result", OK: true, DurationMs: 5, Usage: &TurnUsage{InputTokens: 1, OutputTokens: 2}},
	}, false, 2),
	"permission_request": NewPermissionRequest(PermissionRequest{RequestID: "r1", AgentID: "a1", ToolName: "Bash", Detail: ToolDetail{Kind: "shell", Command: "rm x", Description: "remove"},
		Options: []PermissionOption{{ID: "always", Label: "Yes, and always allow access to /w"}}, Feedback: true}),
	"agent_update_delegated": NewAgentUpdate(with(func(s *AgentSummary) {
		s.ID, s.Parent, s.Relationship, s.CreatedBy = "a2", "a1", "subagent", "agent:a1"
		s.Delegation = &DelegationRef{TaskID: "task1", Status: "working", WorkState: "working"}
	})),
	"agent_delegations": NewAgentDelegations("d1", "a1", []Delegation{
		{TaskID: "task1", ParentID: "a1", ChildID: "a2", Title: "Write the tests", Agent: "codex", Model: "gpt-5.5", Status: "working", CreatedAt: 1},
		{TaskID: "task2", ParentID: "a1", ChildID: "a3", Title: "Review", Agent: "claude", Status: "completed", Summary: "Two findings", CreatedAt: 1, SettledAt: 2},
	}),
	"delegation_update": NewDelegationUpdate("a1", Delegation{TaskID: "task1", ParentID: "a1", ChildID: "a2", Title: "Write the tests", Agent: "codex", Status: "working", Question: "Which test runner?", CreatedAt: 1}),
	"automations_agent_made": NewAutomations([]Automation{
		{ID: "au2", Name: "Nightly tests", Prompt: "run the tests", Provider: "claude", Folder: "/Users/me/app", Schedule: Schedule{Kind: "webhook"}, Enabled: true,
			CreatedBy: "a1", Bound: "a1", Webhook: true, WebhookURL: "http://127.0.0.1:7123/hooks/au2", WebhookSecret: "s3cret"},
		{ID: "au3", Name: "Weekday check", Prompt: "check the build", Provider: "codex", Folder: "/Users/me/app", Schedule: Schedule{Kind: "days", Days: []int{1, 2, 3, 4, 5}, Time: "09:00"}, Enabled: true,
			NextRunAt: 1791450000000, CreatedBy: "a1", Webhook: true, WebhookURL: "http://127.0.0.1:7123/hooks/au3"},
	}, nil),
	"queue_list": NewQueueList("q1", "a1", []QueueEntry{
		{ID: "e1", Text: "also fix the lint", Origin: "user", State: "pending", Order: 1},
		{ID: "e2", Text: "Write the tests finished.", Origin: "wake", State: "pending", Order: 2},
	}),
	"queue_update": NewQueueUpdate("a1", nil),
	"stream_delivery": NewAgentStream("a1", 1, Item{ID: "i9", Seq: 9, Ts: 18, Kind: "delivery", Delivery: &Delivery{
		TaskIDs: []string{"task1"}, ChildIDs: []string{"a2"}, Titles: []string{"Write the tests"}, Agents: []string{"codex"}, Outcome: "done"}}),
	"permission_resolved":   NewPermissionResolved("r1", "allow"),
	"ack":                   NewAck("p1"),
	"agent_delivery":        NewAgentDelivery("prompt-k3x9-1", "a1", DeliveryStarted, ""),
	"agent_delivery_failed": NewAgentDelivery("prompt-k3x9-2", "a1", DeliveryFailed, "This session is still resuming"),
	"error":                 NewError("p1", "Unknown agent: zz"),
	"agent_creating":        NewAgentCreating("c1", "r1", "t1", "/w/fix", true),
	"agent_created":         NewAgentCreated("c1", "r1", "a1", "t1"),
	"agent_progress":        NewAgentProgress("c1", "r1", StepFetch, "Couldn't fetch, using local main"),
	"agent_providers":       NewAgentProviders("p1", providerInfos(), "full", "ask", "edits"),
	"error_detail":          NewCodedError("create-r1", "spawn_failed", "Setup exited 1", "npm ERR! missing script: setup"),
	"hello_ok_host":         helloOKHost(),
	"host_changed":          NewHostChanged(HostState{Tailnet: true, KeepingAwake: true, Terminals: true}),
	"error_coded":           NewErrorCode("h1", "client_too_old", "Update Pocket on this phone"),
	"pair_ok":               NewPairOK("p1", "3fa9c1d2e5f60718293a4b5c6d7e8f90", "dG9rZW4tdG9rZW4tdG9rZW4tdG9rZW4tdG9rZW4tdG9"),
	"hello_ok_scopes": func() HelloOK {
		h := NewHelloOK("h1", "mac", 3, []string{"pair.v1", "scopes.v1"})
		h.Scopes = []string{"observe", "drive", "approve", "spawn", "owner"}
		return h
	}(),
	"hello_ok_version": func() HelloOK {
		h := NewHelloOK("h1", "mac", 3, []string{"pair.v1"})
		h.Version = "0.1.0"
		return h
	}(),
	"pair_offer":         NewPairOffer("b1", "anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv", "abcdefghijklmnopqrstuv", 1790000000000),
	"pair_done":          NewPairDone("d1", "iPhone"),
	"error_scope_denied": NewErrorCode("r1", CodeScopeDenied, "permission.resolve needs approve; run it outside Pocket Terminals, or against a scratch pocketd (POCKETD_SOCK)"),
	"project_list": NewProjectList("p1", []Project{{ID: "p1a2b3c4d5e6f7a8", Path: "/Users/me/dev/pocket", Name: "pocket", Worktrees: []Worktree{
		{Name: "pocket", Path: "/Users/me/dev/pocket", Branch: "main", IsMain: true},
		{Name: "calm-otter", Path: "/Users/me/.worktrees/pocket/calm-otter", Branch: ""},
	}}}),
	"worktree_names":   NewWorktreeNames(map[string]string{"/Users/me/wt/calm-otter": "Fix login"}),
	"naming_failed":    NewNamingFailed("a1"),
	"local_list":       NewLocalList([]Local{{ID: "l1", Project: "/Users/me/dev/pocket", Name: "Local 2"}}),
	"local_list_empty": NewLocalList(nil),
	"agent_update_placed": NewAgentUpdate(with(func(s *AgentSummary) {
		s.ProjectID, s.Tree = "p1a2b3c4d5e6f7a8", "/Users/me/.anywhere/worktrees/pocket/calm-otter"
	})),
	"registry_list_empty": NewRegistryList(nil, nil, SharedSettings{}),
	"registry_list": NewRegistryList([]RegistryProject{
		{ID: "p1a2b3c4d5e6f7a8", Path: "/Users/me/app", Repo: RepoConfig{
			Name: "App", Color: 4286578816, Setup: "make setup", Teardown: "make clean", DevURL: "http://localhost:3000",
			Launch:    json.RawMessage(`{"provider":"claude","model":"opus","effort":"","access":"full","account":""}`),
			Icon:      json.RawMessage(`{"emoji":"A"}`),
			GhAccount: "work", Accounts: map[string]string{"claude": "3f9a0c1b2d4e"}, Hidden: []string{"codex"}}},
		{ID: "q9b8c7d6e5f4a3b2", Path: "/Users/me/other"},
	}, []string{"/Users/me/.anywhere/worktrees/app/cute-boa"},
		SharedSettings{Git: GitSettings{Remote: "fork", CommitAll: ptr(false), ForceWithLease: true}, Access: "edits", WorktreeRoot: "/Users/me/.anywhere/worktrees"}),
}

func helloOKHost() HelloOK {
	ok := NewHelloOK("h1", "mac", 3, []string{CapHost})
	ok.Host = &HostState{Tailnet: true, Terminals: true}
	return ok
}

func all(goldens ...map[string]any) map[string]any {
	out := map[string]any{}
	for _, g := range goldens {
		maps.Copy(out, g)
	}
	return out
}

// TestServerGolden pins the exact JSON the phone decodes.
func TestServerGolden(t *testing.T) {
	for name, msg := range all(serverGolden, phoneServerGolden, hostsServerGolden) {
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
		`{"type":"project.add","id":"1"}`,
		`{"type":"project.add","id":"1","path":""}`,
		`{"type":"project.add","id":"1","path":"/p","repo":null}`,
		`{"type":"project.add","id":"1","path":"/p","repo":{"color":"red"}}`,
		`{"type":"project.remove","id":"1"}`,
		`{"type":"project.set","id":"1","path":"/p"}`,
		`{"type":"project.set","id":"1","repo":{}}`,
		`{"type":"worktree.track","id":"1","path":"/p"}`,
		`{"type":"worktree.track","id":"1","tracked":true}`,
		`{"type":"settings.shared","id":"1"}`,
		`{"type":"settings.shared","id":"1","settings":{"git":"fork"}}`,
		`not json`,
		`{"type":"agent.list"}`,
		`{"type":"agent.create","id":"1","cwd":"/","profileId":"p","prompt":"x"}`,
		`{"type":"agent.prompt","id":"1","agentId":"a"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":501}`,
		`{"type":"agent.view","id":"1","agentIds":[],"front":"yes"}`,
		`{"type":"permission.resolve","id":"1","requestId":"r","decision":"maybe"}`,
		`{"type":"hello","id":"1","token":"t","clientId":"c"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":0}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":-5}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","limit":null}`,
		`{"type":"agent.prompt","id":"1","agentId":"a","text":null}`,
		`{"type":"agent.close","id":null,"agentId":"a"}`,
		`{"type":"agent.view","id":"1"}`,
		`{"type":"agent.pin","id":"1","agentId":"a"}`,
		`{"type":"skills.list","id":"1","provider":"claude"}`,
		`{"type":"skills.list","id":"1","provider":"claude","project":""}`,
		`{"type":"agent.settings","id":"1","agentId":"a"}`,
		`{"type":"agent.mcp","id":"1","agentId":"a","action":"auth"}`,
		`{"type":"agent.mcp","id":"1","agentId":"a","action":"delete","server":"x"}`,
		`{"type":"agent.mcp","id":"1","agentId":"a","server":"x"}`,
		`{"type":"agent.settings","id":"1","agentId":"a","access":"settings"}`,
		`{"type":"agent.settings","id":"1","agentId":"a","access":"root"}`,
		`{"type":"agent.settings","id":"1","agentId":"a","options":{}}`,
		`{"type":"agent.settings","id":"1","agentId":"a","options":{"fast":true}}`,
		`{"type":"agent.pin","id":"1","agentId":"a","pinned":"yes"}`,
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
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"branch":"b","base":"main"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"7","base":"main"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"7","branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","Branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w","new":null},"provider":"claude","access":"ask","plan":false,"model":null}}`,
		`{"type":"config.set","id":"1","key":"phone.maxAccess","value":"settings"}`,
		`{"type":"config.set","id":"1","key":"theme","value":"ask"}`,
		`{"type":"worktree.rename","id":"1","path":"","title":"x"}`,
		`{"type":"worktree.rename","id":"1","path":"/p"}`,
		`{"type":"worktree.rename","id":"1","path":"/p","title":null}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","branch":"b","autoName":true}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"1","autoName":true}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"local":"l1","autoName":true},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n"},"autoName":true},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"local.create","id":"1","localId":"/x","project":"/p","name":"n"}`,
		`{"type":"local.create","id":"1","localId":"","project":"/p","name":"n"}`,
		`{"type":"local.create","id":"1","localId":"a b","project":"/p","name":"n"}`,
		`{"type":"local.create","id":"1","localId":"` + strings.Repeat("a", 65) + `","project":"/p","name":"n"}`,
		`{"type":"local.create","id":"1","localId":"l1","project":"/p"}`,
		`{"type":"local.create","id":"1","localId":"l1","project":"","name":"n"}`,
		`{"type":"local.create","id":"1","localId":"l1","name":"n"}`,
		`{"type":"local.rename","id":"1","localId":"l1","title":null}`,
		`{"type":"local.rename","id":"1","title":"x"}`,
		`{"type":"local.delete","id":"1"}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w","local":"l1"},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"local":"/x"},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"local":""},"provider":"claude","access":"ask","plan":false}}`,
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
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"#7"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"worktree.rename","id":"1","path":"/p","title":""}`,
		`{"type":"local.rename","id":"1","localId":"l1","title":""}`,
		`{"type":"local.create","id":"1","localId":"` + strings.Repeat("a", 64) + `","project":"/p","name":""}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"local":"Az09_-"},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","base":"main","autoName":true}},"provider":"claude","access":"ask","plan":false,"prompt":"x"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w","autoName":true},"provider":"claude","access":"ask","plan":false,"prompt":"x"}}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != nil {
			t.Errorf("%s: %v", raw, err)
		}
	}
}

func TestTeamVerbsAreUnknown(t *testing.T) {
	for _, raw := range []string{
		`{"type":"team.list","id":"1"}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":[]}`,
		`{"type":"team.stop","id":"1","lead":"a1","task":"t1"}`,
		`{"type":"team.merge","id":"1","lead":"a1"}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: err = %v, want ErrMalformed", raw, err)
		}
	}
}

func TestASpecCannotNameATeamOrParent(t *testing.T) {
	for _, extra := range []string{`"team":true`, `"parent":"x"`} {
		raw := `{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,` + extra + `}}`
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: err = %v, want ErrMalformed", extra, err)
		}
	}
}

func TestServerCapsLackTeam(t *testing.T) {
	if slices.Contains(ServerCaps, "team.v1") {
		t.Fatalf("ServerCaps = %v", ServerCaps)
	}
}
