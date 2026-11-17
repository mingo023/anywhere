package mcp

import (
	"encoding/json"
	"fmt"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

const parentLink = "anywhere://session/parent"

func (f *fixture) call(t *testing.T, tool, args string) (map[string]any, bool) {
	t.Helper()
	_, out := f.rpc(t, "/mcp", f.token, fmt.Sprintf(`{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":%q,"arguments":%s}}`, tool, args))
	res, ok := out["result"].(map[string]any)
	if !ok {
		t.Fatalf("%s: no result in %v", tool, out)
	}
	sc, _ := res["structuredContent"].(map[string]any)
	return sc, res["isError"] == true
}

// ok is call that fails the test on a tool error.
func (f *fixture) ok(t *testing.T, tool, args string) map[string]any {
	t.Helper()
	sc, isErr := f.call(t, tool, args)
	if isErr {
		t.Fatalf("%s %s failed: %v", tool, args, sc)
	}
	return sc
}

func errorCode(sc map[string]any) string {
	e, _ := sc["error"].(map[string]any)
	code, _ := e["code"].(string)
	return code
}

func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func (f *fixture) seed(t *testing.T, s proto.AgentSummary) {
	t.Helper()
	k := orch.NewSink(f.store)
	k.Summary(s)
	k.Close()
}

// addSession is a live session other than the caller, in the store and the host.
func (f *fixture) addSession(t *testing.T, s proto.AgentSummary, parent, relationship string) {
	t.Helper()
	f.host.mu.Lock()
	f.host.others[s.ID] = s
	f.host.mu.Unlock()
	f.seed(t, s)
	if parent == "" && relationship == "" {
		return
	}
	if err := f.store.Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`UPDATE sessions SET parent_id = ?, relationship = ? WHERE id = ?`, parent, relationship, s.ID)
		return err
	}); err != nil {
		t.Fatal(err)
	}
}

func (f *fixture) say(t *testing.T, session string, items ...proto.Item) {
	t.Helper()
	k := orch.NewSink(f.store)
	for _, it := range items {
		k.Item(session, 1, it)
	}
	k.Close()
}

func (f *fixture) effectKinds() []string {
	f.eff.mu.Lock()
	defer f.eff.mu.Unlock()
	var out []string
	for _, e := range f.eff.ran {
		out = append(out, e.Kind)
	}
	return out
}

func threadIDs(sc map[string]any) []string {
	var ids []string
	for _, x := range sc["threads"].([]any) {
		ids = append(ids, x.(map[string]any)["threadId"].(string))
	}
	slices.Sort(ids)
	return ids
}

func TestCapabilitiesNameTheAgentsWhoCanDelegateTheLimitsAndTheCaller(t *testing.T) {
	f := newFixture(t)
	sc := f.ok(t, "orchestrator_capabilities", `{}`)
	if self := sc["self"].(map[string]any); self["link"] != parentLink || self["access"] != "full" {
		t.Errorf("self = %v", self)
	}
	can := map[string]bool{}
	for _, a := range sc["agents"].([]any) {
		a := a.(map[string]any)
		can[a["id"].(string)], _ = a["canDelegate"].(bool)
	}
	if !can["claude"] || !can["codex"] || can["pi"] {
		t.Errorf("canDelegate = %v", can)
	}
	if sc["limits"].(map[string]any)["depth"] != float64(3) {
		t.Errorf("limits = %v", sc["limits"])
	}
}

func TestOnlyASessionInAnActiveTurnCanDelegate(t *testing.T) {
	f := newFixtureAs(t, "idle", "full")
	sc, isErr := f.call(t, "delegate_task", `{"task":"Write the tests"}`)
	if !isErr || errorCode(sc) != orch.CodeInvalid {
		t.Fatalf("an idle session delegated: %v", sc)
	}
	if n := len(f.eff.launches()); n != 0 {
		t.Fatalf("launches = %d", n)
	}
}

func TestTaskStatusAndCancelNameTheChildSessionAndStopsItsTurn(t *testing.T) {
	f := newFixture(t)
	task := f.ok(t, "delegate_task", `{"task":"Write the tests","title":"Tests"}`)["taskId"].(string)
	args := fmt.Sprintf(`{"taskId":%q}`, task)
	var view map[string]any
	eventually(t, "the child to start", func() bool {
		view = f.ok(t, "task_status", args)["tasks"].([]any)[0].(map[string]any)
		return view["status"] == "running"
	})
	if view["link"] != "anywhere://session/child" || view["childThreadId"] != "child" {
		t.Errorf("task_status = %v", view)
	}
	cancelled := f.ok(t, "task_cancel", args)
	if cancelled["status"] != "cancel_requested" || cancelled["link"] != "anywhere://session/child" {
		t.Errorf("task_cancel = %v", cancelled)
	}
	view = f.ok(t, "task_status", args)["tasks"].([]any)[0].(map[string]any)
	if view["status"] != "cancelled" {
		t.Errorf("after cancel: %v", view)
	}
	eventually(t, "the child's turn to be interrupted", func() bool {
		in := f.eff.effects(orch.EffInterrupt)
		return len(in) == 1 && in[0].Lane == "child"
	})
}

func TestCreateThreadsStartsEachSessionInTheCallersCheckoutAndLinksIt(t *testing.T) {
	f := newFixture(t)
	sc := f.ok(t, "create_threads", `{"threads":[{"prompt":"one","title":"One"},{"prompt":"two","title":"Two","target":{"agent":"codex"}}]}`)
	threads := sc["threads"].([]any)
	if len(threads) != 2 {
		t.Fatalf("threads = %v", threads)
	}
	for _, x := range threads {
		x := x.(map[string]any)
		if x["status"] != "started" || x["link"] != "anywhere://session/child" {
			t.Errorf("thread = %v", x)
		}
	}
	var agents []string
	for _, l := range f.eff.launches() {
		var p orch.LaunchPayload
		json.Unmarshal(l.Payload, &p)
		if p.Checkout != "shared" || p.Parent != "" {
			t.Errorf("launch = %+v", p)
		}
		agents = append(agents, p.Agent)
	}
	slices.Sort(agents)
	if !slices.Equal(agents, []string{"claude", "codex"}) {
		t.Errorf("agents = %v", agents)
	}
	if _, isErr := f.call(t, "create_threads", `{"threads":[]}`); !isErr {
		t.Error("no threads was accepted")
	}
}

func TestThreadLaunchStartsASessionInTheProjectRoot(t *testing.T) {
	f := newFixture(t)
	sc := f.ok(t, "anywhere_thread_launch", `{"prompt":"look around","checkout":{"strategy":"root"}}`)
	if sc["status"] != "started" || sc["link"] != "anywhere://session/child" {
		t.Fatalf("launch = %v", sc)
	}
	var p orch.LaunchPayload
	json.Unmarshal(f.eff.launches()[0].Payload, &p)
	if p.Checkout != "root" || p.Project != "/repo" || p.Prompt != "look around" {
		t.Errorf("payload = %+v", p)
	}
	if _, isErr := f.call(t, "anywhere_thread_launch", `{"checkout":{"strategy":"existing_worktree"}}`); !isErr {
		t.Error("an existing_worktree launch named no worktree")
	}
}

func TestThreadListShowsTheProjectsSessionsWithLinksAndHidesChildrenUnlessAsked(t *testing.T) {
	f := newFixture(t)
	f.seed(t, f.host.parent)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo", Title: "Alpha"}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "s2", Provider: "claude", Access: "edits", Status: "idle", Project: "/other", Title: "Elsewhere"}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "kid", Provider: "codex", Access: "edits", Status: "working", Project: "/repo", Title: "Child"}, "parent", "subagent")

	sc := f.ok(t, "anywhere_thread_list", `{}`)
	if got := threadIDs(sc); !slices.Equal(got, []string{"parent", "s1"}) {
		t.Errorf("default list = %v", got)
	}
	for _, x := range sc["threads"].([]any) {
		x := x.(map[string]any)
		if x["link"] != "anywhere://session/"+x["threadId"].(string) {
			t.Errorf("entry without its link: %v", x)
		}
	}
	if got := threadIDs(f.ok(t, "anywhere_thread_list", `{"includeSubagents":true}`)); !slices.Equal(got, []string{"kid", "parent", "s1"}) {
		t.Errorf("with children = %v", got)
	}
	if got := threadIDs(f.ok(t, "anywhere_thread_list", `{"allProjects":true}`)); !slices.Equal(got, []string{"parent", "s1", "s2"}) {
		t.Errorf("all projects = %v", got)
	}
}

func TestThreadListRefusesANegativeCursor(t *testing.T) {
	f := newFixture(t)
	f.seed(t, f.host.parent)
	sc, isErr := f.call(t, "anywhere_thread_list", `{"cursor":-1}`)
	if !isErr || errorCode(sc) != orch.CodeInvalid {
		t.Fatalf("a negative cursor gave %v", sc)
	}
}

func TestThreadSearchFindsASessionByItsConversationTitleOrBranch(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo", Title: "Refactor", Branch: "feat/orchestrator-edge"}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "kid", Provider: "codex", Access: "edits", Status: "idle", Project: "/repo", Title: "Refactor child", Branch: "feat/orchestrator-edge-kid"}, "parent", "subagent")
	f.say(t, "s1", proto.Item{ID: "i1", Seq: 4, Ts: 1, Kind: "user", Text: "find the needle in the haystack"})
	for query, want := range map[string]string{"needle": "s1", "Refactor": "s1", "orchestrator-edge": "s1"} {
		sc := f.ok(t, "anywhere_thread_search", fmt.Sprintf(`{"query":%q}`, query))
		results, _ := sc["results"].([]any)
		if len(results) != 1 {
			t.Errorf("%q: results = %v, want only %s (children are left out)", query, results, want)
			continue
		}
		thread := results[0].(map[string]any)["thread"].(map[string]any)
		if thread["threadId"] != want || thread["link"] != "anywhere://session/s1" {
			t.Errorf("%q: thread = %v", query, thread)
		}
	}
	hits, _ := f.ok(t, "anywhere_thread_search", `{"query":"needle"}`)["results"].([]any)
	if len(hits) != 1 || hits[0].(map[string]any)["position"] != float64(4) {
		t.Errorf("a conversation hit has no position: %v", hits)
	}
	if sc := f.ok(t, "anywhere_thread_search", `{"query":"nothing like this"}`); sc["results"] != nil {
		t.Errorf("results = %v", sc["results"])
	}
}

func TestThreadSearchFindsASessionByItsBranchsPullRequest(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo", Worktree: "/repo/wt", Title: "Edge", Branch: "feat/edge"}, "", "")
	f.host.prs = map[string]*proto.PR{"/repo/wt": {Number: 268, Title: "Orchestrator in pocketd", URL: "https://github.com/o/r/pull/268"}}
	for _, query := range []string{"#268", "268", "orchestrator in", "https://github.com/o/r/pull/268"} {
		results, _ := f.ok(t, "anywhere_thread_search", fmt.Sprintf(`{"query":%q}`, query))["results"].([]any)
		if len(results) != 1 || results[0].(map[string]any)["thread"].(map[string]any)["threadId"] != "s1" ||
			results[0].(map[string]any)["snippet"] != "PR #268 Orchestrator in pocketd" {
			t.Errorf("%q: results = %v", query, results)
		}
	}
}

func TestThreadSearchLeavesOtherProjectsAndChildrenOutBeforeItsLimit(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo", Title: "Mine", UpdatedAt: 1}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "s2", Provider: "claude", Access: "edits", Status: "idle", Project: "/other", Title: "needle elsewhere", UpdatedAt: 3}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "kid", Provider: "codex", Access: "edits", Status: "idle", Project: "/repo", Title: "needle child", UpdatedAt: 2}, "parent", "subagent")
	f.say(t, "s1", proto.Item{ID: "i1", Seq: 1, Ts: 1, Kind: "user", Text: "find the needle in the haystack"})
	for _, other := range []string{"s2", "kid"} {
		for n := range 5 {
			f.say(t, other, proto.Item{ID: fmt.Sprint(other, n), Seq: int64(n + 1), Ts: 1, Kind: "user", Text: "needle"})
		}
	}
	for _, limit := range []int{1, 2} {
		results, _ := f.ok(t, "anywhere_thread_search", fmt.Sprintf(`{"query":"needle","limit":%d}`, limit))["results"].([]any)
		if len(results) != 1 || results[0].(map[string]any)["thread"].(map[string]any)["threadId"] != "s1" {
			t.Errorf("limit %d: results = %v, want s1", limit, results)
		}
	}
}

func TestThreadReadPagesMessagesAndGivesAWorkflowItsTaskIDInTheActivityView(t *testing.T) {
	f := newFixture(t)
	f.host.items["parent"] = []proto.Item{
		{ID: "i1", Seq: 1, Kind: "user", Text: "run the workflow"},
		{ID: "i2", Seq: 2, Kind: "assistant", Text: "started it"},
		{ID: "i3", Seq: 3, Kind: "tool", Call: &proto.ToolCall{ToolUseID: "tu1", Name: "Workflow", Status: "running", Detail: proto.ToolDetail{Kind: "other"}}},
		{ID: "i4", Seq: 4, Kind: "tool", Call: &proto.ToolCall{ToolUseID: "tu2", Name: "Read", Status: "ok", Detail: proto.ToolDetail{Kind: "read", Path: "/f"}}},
	}
	f.host.tasks["parent/tu1"] = "task-9"

	msgs := f.ok(t, "anywhere_thread_read", `{}`)
	if len(msgs["items"].([]any)) != 2 || msgs["thread"].(map[string]any)["link"] != parentLink {
		t.Errorf("messages = %v", msgs)
	}
	if after := f.ok(t, "anywhere_thread_read", `{"afterPosition":1}`)["items"].([]any); len(after) != 1 || after[0].(map[string]any)["text"] != "started it" {
		t.Errorf("after position 1 = %v", after)
	}
	activity := f.ok(t, "anywhere_thread_read", `{"view":"activity"}`)["items"].([]any)
	if len(activity) != 2 {
		t.Fatalf("activity = %v", activity)
	}
	if activity[0].(map[string]any)["taskId"] != "task-9" {
		t.Errorf("the workflow's activity row has no taskId: %v", activity[0])
	}
	if _, has := activity[1].(map[string]any)["taskId"]; has {
		t.Errorf("a plain tool call has a taskId: %v", activity[1])
	}
}

func TestWorkflowStopSendsTheTaskIDTheActivityViewGave(t *testing.T) {
	f := newFixture(t)
	f.host.items["parent"] = []proto.Item{{ID: "i3", Seq: 3, Kind: "tool", Call: &proto.ToolCall{ToolUseID: "tu1", Name: "Workflow", Status: "running", Detail: proto.ToolDetail{Kind: "other"}}}}
	f.host.tasks["parent/tu1"] = "task-9"
	rows, _ := f.ok(t, "anywhere_thread_read", `{"view":"activity"}`)["items"].([]any)
	taskID, _ := rows[0].(map[string]any)["taskId"].(string)
	sc := f.ok(t, "anywhere_workflow_stop", fmt.Sprintf(`{"taskId":%q}`, taskID))
	if sc["status"] != "stop_requested" || sc["link"] != parentLink {
		t.Errorf("workflow_stop = %v", sc)
	}
	stops := f.eff.effects(orch.EffStopTask)
	if len(stops) != 1 || !strings.Contains(string(stops[0].Payload), `"task-9"`) {
		t.Fatalf("stop_task effects = %+v", stops)
	}
}

func TestThreadSendStartsOrQueuesAndRefusesASessionBroaderThanTheCaller(t *testing.T) {
	f := newFixtureAs(t, "working", "full")
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo"}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "busy", Provider: "claude", Access: "edits", Status: "working", Project: "/repo"}, "", "")
	sc := f.ok(t, "anywhere_thread_send", `{"threadId":"s1","message":"go on"}`)
	if sc["queued"] != false || sc["link"] != "anywhere://session/s1" {
		t.Errorf("send to an idle session = %v", sc)
	}
	eventually(t, "the turn to start", func() bool {
		s := f.eff.effects(orch.EffTurnStart)
		return len(s) == 1 && s[0].Lane == "s1" && strings.Contains(string(s[0].Payload), "go on")
	})
	if sc := f.ok(t, "anywhere_thread_send", `{"threadId":"busy","message":"later"}`); sc["queued"] != true {
		t.Errorf("send to a working session = %v", sc)
	}

	narrow := newFixtureAs(t, "working", "edits")
	narrow.addSession(t, proto.AgentSummary{ID: "wide", Provider: "claude", Access: "full", Status: "idle", Project: "/repo"}, "", "")
	for tool, args := range map[string]string{
		"anywhere_thread_send":      `{"threadId":"wide","message":"hi"}`,
		"anywhere_thread_interrupt": `{"threadId":"wide"}`,
		"anywhere_thread_configure": `{"threadId":"wide","model":"m"}`,
		"anywhere_thread_update":    `{"threadId":"wide","title":"x"}`,
		"anywhere_queue":            `{"threadId":"wide","action":"cancel","entryId":"q"}`,
	} {
		if sc, isErr := narrow.call(t, tool, args); !isErr || errorCode(sc) != orch.CodeForbidden {
			t.Errorf("%s on a broader session: %v", tool, sc)
		}
	}
	if kinds := narrow.effectKinds(); len(kinds) != 0 {
		t.Errorf("effects ran: %v", kinds)
	}
}

func TestThreadWaitReturnsOnceTheTurnEndsOrTimesOut(t *testing.T) {
	old := waitPoll
	waitPoll = 5 * time.Millisecond
	t.Cleanup(func() { waitPoll = old })
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "idle", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo"}, "", "")
	f.addSession(t, proto.AgentSummary{ID: "busy", Provider: "claude", Access: "edits", Status: "working", Project: "/repo"}, "", "")
	if sc := f.ok(t, "anywhere_thread_wait", `{"threadId":"idle"}`); sc["waitTimedOut"] != false || sc["link"] != "anywhere://session/idle" {
		t.Errorf("idle = %v", sc)
	}
	if sc := f.ok(t, "anywhere_thread_wait", `{"threadId":"busy","timeoutMs":50}`); sc["waitTimedOut"] != true || sc["status"] != "working" {
		t.Errorf("busy = %v", sc)
	}
	if _, isErr := f.call(t, "anywhere_thread_wait", `{"threadId":"parent"}`); !isErr {
		t.Error("a session waited on itself")
	}
}

func TestThreadInterruptStopsAnotherSessionsTurn(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "busy", Provider: "claude", Access: "edits", Status: "working", Project: "/repo"}, "", "")
	sc := f.ok(t, "anywhere_thread_interrupt", `{"threadId":"busy"}`)
	if sc["status"] != "interrupt_requested" || sc["link"] != "anywhere://session/busy" {
		t.Errorf("interrupt = %v", sc)
	}
	if in := f.eff.effects(orch.EffInterrupt); len(in) != 1 || in[0].Lane != "busy" {
		t.Errorf("interrupt effects = %+v", in)
	}
}

func TestThreadConfigureReadsAndSetsModelAndRefusesAnotherAgent(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo", Model: "m1"}, "", "")
	read := f.ok(t, "anywhere_thread_configure", `{"threadId":"s1"}`)
	if read["model"] != "m1" || read["link"] != "anywhere://session/s1" || len(f.effectKinds()) != 0 {
		t.Errorf("read = %v, effects %v", read, f.effectKinds())
	}
	set := f.ok(t, "anywhere_thread_configure", `{"threadId":"s1","model":"m2","effort":"high"}`)
	if set["link"] != "anywhere://session/s1" {
		t.Errorf("set = %v", set)
	}
	cfg := f.eff.effects(orch.EffConfigure)
	if len(cfg) != 1 || !strings.Contains(string(cfg[0].Payload), `"model":"m2"`) || !strings.Contains(string(cfg[0].Payload), `"effort":"high"`) {
		t.Errorf("configure effects = %+v", cfg)
	}
	if sc, isErr := f.call(t, "anywhere_thread_configure", `{"threadId":"s1","agent":"codex"}`); !isErr || errorCode(sc) != orch.CodeUnsupported {
		t.Errorf("changing the agent: %v", sc)
	}
}

func TestPendingRequestListsAChildsQuestionAndAnswersIt(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "kid", Provider: "codex", Access: "edits", Status: "working", Project: "/repo"}, "parent", "subagent")
	f.addSession(t, proto.AgentSummary{ID: "stranger", Provider: "codex", Access: "edits", Status: "working", Project: "/repo"}, "", "")
	f.engine.AskOpened("kid", "r1", "question", json.RawMessage(`{"questions":[{"question":"Which?"}]}`))
	eventually(t, "the question to be recorded", func() bool {
		open, _ := f.store.Requests("kid", "open")
		return len(open) == 1
	})
	list := f.ok(t, "anywhere_pending_request", `{"threadId":"kid"}`)
	qs := list["questions"].([]any)
	if len(qs) != 1 || qs[0].(map[string]any)["requestId"] != "r1" || list["link"] != "anywhere://session/kid" {
		t.Fatalf("list = %v", list)
	}
	if sc, isErr := f.call(t, "anywhere_pending_request", `{"threadId":"stranger"}`); !isErr || errorCode(sc) != orch.CodeForbidden {
		t.Errorf("a session outside the caller's: %v", sc)
	}
	done := f.ok(t, "anywhere_pending_request", `{"threadId":"kid","requestId":"r1","answers":{"Which?":"A"}}`)
	if done["status"] != "answered" || done["link"] != "anywhere://session/kid" {
		t.Errorf("answer = %v", done)
	}
	if a := f.eff.effects(orch.EffAnswer); len(a) != 1 || a[0].Lane != "kid" || !strings.Contains(string(a[0].Payload), `"Which?":"A"`) {
		t.Errorf("answer effects = %+v", a)
	}
}

func TestQueueToolListsAndCancelsTheMessagesQueuedForASession(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "busy", Provider: "claude", Access: "edits", Status: "working", Project: "/repo"}, "", "")
	entry := f.ok(t, "anywhere_thread_send", `{"threadId":"busy","message":"later","mode":"queue"}`)["entryId"].(string)
	list := f.ok(t, "anywhere_queue", `{"threadId":"busy"}`)
	if entries := list["entries"].([]any); len(entries) != 1 || entries[0].(map[string]any)["text"] != "later" || list["link"] != "anywhere://session/busy" {
		t.Fatalf("list = %v", list)
	}
	cancelled := f.ok(t, "anywhere_queue", fmt.Sprintf(`{"threadId":"busy","action":"cancel","entryId":%q}`, entry))
	if len(cancelled["entries"].([]any)) != 0 || cancelled["link"] != "anywhere://session/busy" {
		t.Errorf("after cancel = %v", cancelled)
	}
}

func TestAForkFromClaudeToCodexCarriesTheConversationAsAContextBlock(t *testing.T) {
	f := newFixture(t)
	f.host.items["parent"] = []proto.Item{
		{ID: "i1", Seq: 1, Kind: "user", Text: "fix the parser"},
		{ID: "i2", Seq: 2, Kind: "assistant", Text: "the bug is in lex"},
	}
	sc := f.ok(t, "anywhere_thread_transfer", `{"action":"fork","target":{"agent":"codex"},"prompt":"carry on"}`)
	if sc["status"] != "started" || sc["link"] != "anywhere://session/fork" || sc["native"] != false {
		t.Fatalf("fork = %v", sc)
	}
	forks := f.eff.effects(orch.EffForkLaunch)
	if len(forks) != 1 {
		t.Fatalf("fork launches = %d", len(forks))
	}
	var p orch.LaunchPayload
	json.Unmarshal(forks[0].Payload, &p)
	if p.Agent != "codex" || p.Rel != "fork" || p.Parent != "parent" || p.Fork != "" || p.Prompt != "carry on" {
		t.Errorf("launch = %+v", p)
	}
	if want := ContextBlock("", f.host.items["parent"], 0); p.Context != want || !strings.Contains(p.Context, "User: fix the parser") || !strings.Contains(p.Context, "Assistant: the bug is in lex") {
		t.Errorf("context = %q", p.Context)
	}
}

func TestRenameAndRegenerateTitleRunNothingButTheTitleEffect(t *testing.T) {
	f := newFixture(t)
	sc := f.ok(t, "anywhere_thread_update", `{"title":"A better name"}`)
	if sc["link"] != parentLink {
		t.Errorf("update = %v", sc)
	}
	if kinds := f.effectKinds(); !slices.Equal(kinds, []string{orch.EffRename}) {
		t.Fatalf("rename ran %v", kinds)
	}
	if p := string(f.eff.effects(orch.EffRename)[0].Payload); p != `{"title":"A better name"}` {
		t.Errorf("rename payload = %s", p)
	}
	f.ok(t, "anywhere_thread_update", `{"regenerateTitle":true}`)
	if kinds := f.effectKinds(); !slices.Equal(kinds, []string{orch.EffRename, orch.EffNaming}) {
		t.Errorf("regenerate ran %v", kinds)
	}
	if _, isErr := f.call(t, "anywhere_thread_update", `{}`); !isErr {
		t.Error("an update with no title or regenerateTitle was accepted")
	}
}

func TestOrganizeRunsOneEffectPerAction(t *testing.T) {
	f := newFixture(t)
	f.addSession(t, proto.AgentSummary{ID: "s1", Provider: "claude", Access: "edits", Status: "idle", Project: "/repo"}, "", "")
	for _, c := range []struct{ action, want string }{{"pin", orch.EffPin}, {"unpin", orch.EffPin}, {"mark_unread", orch.EffMarkUnread}, {"archive", orch.EffClose}} {
		action, want := c.action, c.want
		before := len(f.effectKinds())
		sc := f.ok(t, "anywhere_thread_organize", fmt.Sprintf(`{"threadId":"s1","action":%q}`, action))
		if sc["link"] != "anywhere://session/s1" {
			t.Errorf("%s: %v", action, sc)
		}
		kinds := f.effectKinds()
		if len(kinds) != before+1 || kinds[before] != want {
			t.Errorf("%s ran %v, want %s", action, kinds[before:], want)
		}
	}
	if pins := f.eff.effects(orch.EffPin); !strings.Contains(string(pins[0].Payload), "true") || !strings.Contains(string(pins[1].Payload), "false") {
		t.Errorf("pin payloads = %s, %s", pins[0].Payload, pins[1].Payload)
	}
	for _, action := range []string{"snooze", "settle"} {
		if sc, isErr := f.call(t, "anywhere_thread_organize", fmt.Sprintf(`{"threadId":"s1","action":%q}`, action)); !isErr || errorCode(sc) != orch.CodeUnsupported {
			t.Errorf("%s: %v", action, sc)
		}
	}
}

func TestBackgroundWaitHoldsTheTasksAnOpenCodeSessionDeclaredAndRefusesOtherAgents(t *testing.T) {
	f := newFixture(t)
	f.host.parent.Provider = "opencode"
	f.host.background["bg1"] = true
	sc := f.ok(t, "anywhere_background_wait", `{"taskIds":["bg1","nope"]}`)
	if got := fmt.Sprint(sc["matched"], sc["unmatched"]); got != "[bg1] [nope]" {
		t.Errorf("matched/unmatched = %s", got)
	}
	holds := f.eff.effects(orch.EffHold)
	if len(holds) != 1 || !strings.Contains(string(holds[0].Payload), `"bg1"`) || strings.Contains(string(holds[0].Payload), `"nope"`) {
		t.Errorf("hold effects = %+v", holds)
	}

	codex := newFixture(t)
	codex.host.parent.Provider = "codex"
	codex.host.background["bg1"] = true
	if sc, isErr := codex.call(t, "anywhere_background_wait", `{"taskIds":["bg1"]}`); !isErr || errorCode(sc) != orch.CodeUnsupported {
		t.Errorf("codex: %v", sc)
	}
}

func TestEveryScheduleKindTheToolAdvertisesIsOneAutomationsTake(t *testing.T) {
	valid := map[string]proto.Schedule{
		"days":     {Kind: "days", Days: []int{1, 3}, Time: "09:30"},
		"interval": {Kind: "interval", EveryMin: 60},
	}
	var schema struct {
		Properties struct {
			Schedule struct {
				Properties struct {
					Kind struct {
						Enum []string `json:"enum"`
					} `json:"kind"`
				} `json:"properties"`
			} `json:"schedule"`
		} `json:"properties"`
	}
	raw := NewToolkit(&host{}).tools["schedule_task"].Schema
	if err := json.Unmarshal([]byte(raw), &schema); err != nil {
		t.Fatal(err)
	}
	kinds := schema.Properties.Schedule.Properties.Kind.Enum
	if len(kinds) != len(valid) {
		t.Fatalf("advertised kinds = %v", kinds)
	}
	for _, k := range kinds {
		if s, ok := valid[k]; !ok || !s.Valid() {
			t.Errorf("the schema advertises %q, which Schedule.Valid rejects", k)
		}
	}
	for _, old := range []string{"hourly", "daily", "weekdays", "weekly"} {
		if strings.Contains(raw, old) {
			t.Errorf("the schema still names %q", old)
		}
	}
}
