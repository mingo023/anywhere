package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

type launcher struct {
	mu  sync.Mutex
	ran []orch.Effect
}

func (l *launcher) Run(ctx context.Context, e orch.Effect) (map[string]any, error) {
	l.mu.Lock()
	l.ran = append(l.ran, e)
	l.mu.Unlock()
	if e.Kind == orch.EffLaunch {
		return map[string]any{"childId": "child", "targetId": "t-child"}, nil
	}
	return nil, nil
}

func (l *launcher) launches() []orch.Effect {
	l.mu.Lock()
	defer l.mu.Unlock()
	var out []orch.Effect
	for _, e := range l.ran {
		if e.Kind == orch.EffLaunch {
			out = append(out, e)
		}
	}
	return out
}

type host struct {
	engine *orch.Engine
	parent proto.AgentSummary
	sched  Schedules
}

func (h *host) Engine() *orch.Engine { return h.engine }
func (h *host) SessionIn(terminal string) (proto.AgentSummary, bool) {
	return h.parent, terminal == h.parent.TerminalID
}
func (h *host) Summary(id string) (proto.AgentSummary, bool) { return h.parent, id == h.parent.ID }
func (h *host) Items(string) []proto.Item                    { return nil }
func (h *host) Agents() []AgentKind {
	return []AgentKind{{ID: "claude", Installed: true, Parent: true}, {ID: "pi", Installed: true, Access: []string{"full"}, NeverAsks: true}}
}
func (h *host) InBackground(string, string) bool { return false }
func (h *host) Dirty(string) bool                { return false }
func (h *host) Schedules() Schedules             { return h.sched }

type fixture struct {
	srv    *httptest.Server
	tokens *Tokens
	store  *orch.Store
	eff    *launcher
	token  string
}

func newFixture(t *testing.T) *fixture {
	t.Helper()
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	eff := &launcher{}
	e := orch.NewEngine(store, eff)
	parent := proto.AgentSummary{ID: "parent", TerminalID: "t-parent", Provider: "claude", Access: "full", Status: "working", Project: "/repo", Cwd: "/repo"}
	e.Agent = func(string) orch.AgentInfo { return orch.AgentInfo{Known: true, Access: []string{"ask", "full"}} }
	e.Live = func(id string) (orch.Session, bool) {
		if id != parent.ID {
			return orch.Session{}, false
		}
		return orch.Session{ID: id, Agent: "claude", Access: "full", Status: "working", Project: "/repo", Cwd: "/repo"}, true
	}
	t.Cleanup(e.Close)
	tokens := NewTokens(store)
	token, err := tokens.Issue(parent.TerminalID, "claude")
	if err != nil {
		t.Fatal(err)
	}
	s := &Server{Tokens: tokens, Tools: NewToolkit(&host{engine: e, parent: parent}), Version: "test"}
	srv := httptest.NewServer(s.Handler())
	t.Cleanup(srv.Close)
	return &fixture{srv: srv, tokens: tokens, store: store, eff: eff, token: token}
}

func (f *fixture) rpc(t *testing.T, path, token, body string) (*http.Response, map[string]any) {
	t.Helper()
	req, _ := http.NewRequest(http.MethodPost, f.srv.URL+path, strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var out map[string]any
	json.NewDecoder(resp.Body).Decode(&out)
	return resp, out
}

func toolNames(t *testing.T, res map[string]any) []string {
	t.Helper()
	tools, _ := res["result"].(map[string]any)["tools"].([]any)
	var names []string
	for _, x := range tools {
		names = append(names, x.(map[string]any)["name"].(string))
	}
	return names
}

func TestACallWithoutAValidTokenIsUnauthorized(t *testing.T) {
	f := newFixture(t)
	for _, token := range []string{"", "anyw_wrong"} {
		resp, _ := f.rpc(t, "/mcp", token, `{"jsonrpc":"2.0","id":1,"method":"ping"}`)
		if resp.StatusCode != http.StatusUnauthorized || resp.Header.Get("WWW-Authenticate") == "" {
			t.Fatalf("token %q: status %d, WWW-Authenticate %q", token, resp.StatusCode, resp.Header.Get("WWW-Authenticate"))
		}
	}
}

func TestInitializeAgreesOnAVersionTheClientSpeaks(t *testing.T) {
	f := newFixture(t)
	for asked, want := range map[string]string{"2025-03-26": "2025-03-26", "1999-01-01": Versions[0]} {
		_, out := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"`+asked+`"}}`)
		res := out["result"].(map[string]any)
		if res["protocolVersion"] != want {
			t.Fatalf("asked %s, got %v", asked, res["protocolVersion"])
		}
		if res["capabilities"].(map[string]any)["tools"].(map[string]any)["listChanged"] != true {
			t.Fatal("tools.listChanged not advertised")
		}
	}
}

func TestANotificationGetsAcceptedWithNoBody(t *testing.T) {
	f := newFixture(t)
	resp, _ := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","method":"notifications/initialized"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("status %d", resp.StatusCode)
	}
}

func TestTheDirectServerListsOnlyTheBlockingTools(t *testing.T) {
	f := newFixture(t)
	_, out := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":1,"method":"tools/list"}`)
	if got := strings.Join(toolNames(t, out), ","); got != "delegate_task,orchestrator_capabilities,task_status,tool_search" {
		t.Fatalf("tools = %s", got)
	}
	_, out = f.rpc(t, "/mcp?direct=1", f.token, `{"jsonrpc":"2.0","id":1,"method":"tools/list"}`)
	if got := strings.Join(toolNames(t, out), ","); got != "anywhere_thread_wait,delegate_task" {
		t.Fatalf("direct tools = %s", got)
	}
}

func TestToolSearchListsTheToolsItFoundAndTellsTheStream(t *testing.T) {
	f := newFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, f.srv.URL+"/mcp", nil)
	req.Header.Set("Authorization", "Bearer "+f.token)
	stream, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Body.Close()
	events := make(chan string, 4)
	go func() {
		sc := bufio.NewScanner(stream.Body)
		for sc.Scan() {
			if data, ok := strings.CutPrefix(sc.Text(), "data: "); ok {
				events <- data
			}
		}
	}()
	time.Sleep(50 * time.Millisecond) // the stream registers after its headers flush

	f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"tool_search","arguments":{"query":"send a message to a thread"}}}`)
	select {
	case e := <-events:
		if !strings.Contains(e, "notifications/tools/list_changed") {
			t.Fatalf("event = %s", e)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("no list_changed")
	}
	_, out := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`)
	if names := toolNames(t, out); !strings.Contains(strings.Join(names, ","), "anywhere_thread_send") {
		t.Fatalf("tools after search = %v", names)
	}
}

func TestDelegateTaskOverHTTPLaunchesAChildWithTheBrief(t *testing.T) {
	f := newFixture(t)
	_, out := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delegate_task","arguments":{"task":"Write the tests","title":"Tests","clientRequestId":"r1"}}}`)
	res := out["result"].(map[string]any)
	if res["isError"] == true {
		t.Fatalf("delegate failed: %v", res)
	}
	task := res["structuredContent"].(map[string]any)
	if task["taskId"] == "" || task["isolation"] != "worktree" {
		t.Fatalf("task = %v", task)
	}
	deadline := time.Now().Add(2 * time.Second)
	for len(f.eff.launches()) == 0 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	launches := f.eff.launches()
	if len(launches) != 1 {
		t.Fatalf("launches = %d", len(launches))
	}
	var p orch.LaunchPayload
	json.Unmarshal(launches[0].Payload, &p)
	if p.Prompt != "Write the tests" || p.Parent != "parent" || p.Checkout != "worktree" {
		t.Fatalf("launch = %+v", p)
	}

	_, again := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"delegate_task","arguments":{"task":"Write the tests","title":"Tests","clientRequestId":"r1"}}}`)
	if again["result"].(map[string]any)["structuredContent"].(map[string]any)["taskId"] != task["taskId"] {
		t.Fatal("a retried call with the same clientRequestId made a second task")
	}
}

func TestTheStoreKeepsOnlyTheTokensHash(t *testing.T) {
	f := newFixture(t)
	var n int
	f.store.DB().QueryRow(`SELECT count(*) FROM tokens WHERE hash = ?`, f.token).Scan(&n)
	if n != 0 {
		t.Fatal("raw token stored")
	}
	f.store.DB().QueryRow(`SELECT count(*) FROM tokens WHERE hash = ?`, hash(f.token)).Scan(&n)
	if n != 1 {
		t.Fatalf("hash rows = %d", n)
	}
}

func TestATokenIdleADayExpiresAndARevokedOneStopsWorking(t *testing.T) {
	f := newFixture(t)
	now := time.Now()
	f.tokens.now = func() time.Time { return now }
	if _, err := f.tokens.Resolve(f.token); err != nil {
		t.Fatal(err)
	}
	now = now.Add(IdleExpiry - time.Minute)
	if _, err := f.tokens.Resolve(f.token); err != nil {
		t.Fatalf("just under a day idle: %v", err)
	}
	now = now.Add(IdleExpiry + time.Minute)
	if _, err := f.tokens.Resolve(f.token); err != ErrBadToken {
		t.Fatalf("a day idle: %v", err)
	}

	other, _ := f.tokens.Issue("t-other", "codex")
	if err := f.tokens.Revoke("t-other"); err != nil {
		t.Fatal(err)
	}
	if _, err := f.tokens.Resolve(other); err != ErrBadToken {
		t.Fatalf("revoked: %v", err)
	}
}

type fakeSchedules struct{ autos map[string]proto.Automation }

func (s *fakeSchedules) List() []proto.Automation {
	var out []proto.Automation
	for _, a := range s.autos {
		out = append(out, a)
	}
	return out
}
func (s *fakeSchedules) Save(a proto.Automation) (proto.Automation, error) {
	if a.ID == "" {
		a.ID = "new"
	}
	s.autos[a.ID] = a
	return a, nil
}
func (s *fakeSchedules) Delete(id string) error              { delete(s.autos, id); return nil }
func (s *fakeSchedules) RunNow(id string) (proto.Run, error) { return proto.Run{ID: "r"}, nil }
func (s *fakeSchedules) Get(id string) (proto.Automation, bool) {
	a, ok := s.autos[id]
	return a, ok
}

func TestAnAgentChangesOnlySchedulesWithinItsAccessAndNeverReadsASecret(t *testing.T) {
	sched := &fakeSchedules{autos: map[string]proto.Automation{
		"users": {ID: "users", Prompt: "deploy", Access: "full", Webhook: true, WebhookSecret: "s3cret"},
	}}
	parent := proto.AgentSummary{ID: "parent", TerminalID: "t", Provider: "claude", Access: "edits", Status: "working"}
	k := NewToolkit(&host{parent: parent, sched: sched})
	g := Grant{Terminal: "t"}
	call := func(name, args string) map[string]any {
		out, _ := k.Call(context.Background(), g, name, json.RawMessage(args))
		return out
	}
	for _, c := range []struct{ tool, args string }{
		{"schedule_task", `{"id":"users","prompt":"rm -rf"}`},
		{"delete_scheduled_task", `{"id":"users"}`},
		{"run_scheduled_task_now", `{"id":"users"}`},
	} {
		if out := call(c.tool, c.args); out["isError"] != true || !strings.Contains(fmt.Sprint(out["structuredContent"]), "forbidden") {
			t.Errorf("%s: %v", c.tool, out)
		}
	}
	if a := sched.autos["users"]; a.Prompt != "deploy" || a.CreatedBy != "" {
		t.Fatalf("the user's schedule changed: %+v", a)
	}
	created := call("schedule_task", `{"prompt":"nightly tests","webhook":true,"webhookSecret":"mine"}`)
	if a := sched.autos["new"]; a.Access != "edits" || a.CreatedBy != "parent" {
		t.Fatalf("created: %+v", a)
	}
	delete(sched.autos, "new")
	if out := call("schedule_task", `{"prompt":"nightly tests","agent":"pi"}`); out["isError"] != true || len(sched.autos) != 1 {
		t.Fatalf("an Edits session scheduled an always-Full agent: %v, %+v", out, sched.autos)
	}
	sched.autos["new"] = proto.Automation{ID: "new", Access: "edits", CreatedBy: "parent", Webhook: true, WebhookSecret: "mine"}
	listed := call("list_scheduled_tasks", `{}`)
	if strings.Contains(fmt.Sprint(created, listed), "s3cret") || strings.Contains(fmt.Sprint(created, listed), "mine") {
		t.Fatalf("a secret reached the agent: %v %v", created, listed)
	}
}
