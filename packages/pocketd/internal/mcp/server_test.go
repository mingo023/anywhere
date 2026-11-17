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
	switch e.Kind {
	case orch.EffLaunch:
		return map[string]any{"childId": "child", "targetId": "t-child"}, nil
	case orch.EffForkLaunch:
		return map[string]any{"childId": "fork", "targetId": "t-fork"}, nil
	}
	return nil, nil
}

func (l *launcher) effects(kind string) []orch.Effect {
	l.mu.Lock()
	defer l.mu.Unlock()
	var out []orch.Effect
	for _, e := range l.ran {
		if e.Kind == kind {
			out = append(out, e)
		}
	}
	return out
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
	envs   Environments
	prs    map[string]*proto.PR // by tree
	// outsideGit is the parent's Project not being a git repository.
	outsideGit bool

	mu         sync.Mutex
	others     map[string]proto.AgentSummary
	items      map[string][]proto.Item
	tasks      map[string]string // "session/toolUseId" to task id
	background map[string]bool
}

func (h *host) Engine() *orch.Engine { return h.engine }
func (h *host) SessionIn(terminal string) (proto.AgentSummary, bool) {
	return h.parent, terminal == h.parent.TerminalID
}
func (h *host) Summary(id string) (proto.AgentSummary, bool) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if id == h.parent.ID {
		return h.parent, true
	}
	s, ok := h.others[id]
	return s, ok
}
func (h *host) Items(id string) []proto.Item {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.items[id]
}
func (h *host) Agents() []AgentKind {
	return []AgentKind{{ID: "claude", Installed: true, Parent: true, Access: proto.Accesses}, {ID: "codex", Installed: true, Parent: true, Access: []string{"ask", "edits", "full"}},
		{ID: "pi", Installed: true, Access: []string{"full"}, NeverAsks: true}}
}
func (h *host) Models(string) []string                      { return nil }
func (h *host) InBackground(_, id string) bool              { return h.background[id] }
func (h *host) TaskOf(session, toolUseID string) string     { return h.tasks[session+"/"+toolUseID] }
func (h *host) Dirty(string) bool                           { return false }
func (h *host) OutsideGit(string) bool                      { return h.outsideGit }
func (h *host) Schedules() Schedules                        { return h.sched }
func (h *host) Environments() Environments                  { return h.envs }
func (h *host) PR(_ context.Context, tree string) *proto.PR { return h.prs[tree] }

type fixture struct {
	srv    *httptest.Server
	tokens *Tokens
	store  *orch.Store
	eff    *launcher
	token  string
	engine *orch.Engine
	host   *host
}

func newFixture(t *testing.T) *fixture { return newFixtureAs(t, "working", "full") }

// newFixtureAs is a fixture whose calling session is in status with access.
func newFixtureAs(t *testing.T, status, access string) *fixture {
	t.Helper()
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	eff := &launcher{}
	e := orch.NewEngine(store, eff)
	parent := proto.AgentSummary{ID: "parent", TerminalID: "t-parent", Provider: "claude", Access: access, Status: status, Project: "/repo", Cwd: "/repo"}
	h := &host{engine: e, parent: parent, others: map[string]proto.AgentSummary{}, items: map[string][]proto.Item{}, tasks: map[string]string{}, background: map[string]bool{}}
	e.Agent = func(string) orch.AgentInfo {
		return orch.AgentInfo{Known: true, Access: []string{"ask", "edits", "auto", "full"}}
	}
	e.Live = func(id string) (orch.Session, bool) {
		s, ok := h.Summary(id)
		if !ok {
			return orch.Session{}, false
		}
		return orch.Session{ID: id, Agent: s.Provider, Access: s.Access, Status: s.Status, Project: s.Project, Cwd: s.Cwd}, true
	}
	t.Cleanup(e.Close)
	tokens := NewTokens(store)
	token, err := tokens.Issue(parent.TerminalID, "claude")
	if err != nil {
		t.Fatal(err)
	}
	s := &Server{Tokens: tokens, Tools: NewToolkit(h), Version: "test"}
	srv := httptest.NewServer(s.Handler())
	t.Cleanup(srv.Close)
	return &fixture{srv: srv, tokens: tokens, store: store, eff: eff, token: token, engine: e, host: h}
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

func TestADelegationFromAProjectOutsideGitSharesItsFolder(t *testing.T) {
	f := newFixture(t)
	f.host.outsideGit = true
	if task := f.ok(t, "delegate_task", `{"task":"Write the tests"}`); task["isolation"] != "shared" {
		t.Fatalf("task = %v", task)
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
	sched.autos["own"] = proto.Automation{ID: "own", Prompt: "lint", Provider: "claude"}
	if out := call("schedule_task", `{"id":"own","prompt":"lint and test"}`); out["isError"] == true || sched.autos["own"].Access != "" || sched.autos["own"].Prompt != "lint and test" {
		t.Fatalf("an edit of a schedule on the agent's own settings: %v, %+v", out, sched.autos["own"])
	}
	delete(sched.autos, "own")
	sched.autos["new"] = proto.Automation{ID: "new", Access: "edits", CreatedBy: "parent", Webhook: true, WebhookSecret: "mine"}
	listed := call("list_scheduled_tasks", `{}`)
	if strings.Contains(fmt.Sprint(created, listed), "s3cret") || strings.Contains(fmt.Sprint(created, listed), "mine") {
		t.Fatalf("a secret reached the agent: %v %v", created, listed)
	}
}

type fakeEnvironments struct {
	list    []Environment
	cards   []proto.HostLink
	removed []string
}

func (e *fakeEnvironments) List() []Environment { return e.list }
func (e *fakeEnvironments) Projects(context.Context, string) ([]proto.Project, error) {
	return nil, nil
}
func (e *fakeEnvironments) Remove(id string) error {
	e.removed = append(e.removed, id)
	return nil
}
func (e *fakeEnvironments) Card(_ string, l proto.HostLink) error {
	e.cards = append(e.cards, l)
	return nil
}

// shrinkingEnvironments loses its hosts after the first List, as a removal between two reads would.
type shrinkingEnvironments struct{ fakeEnvironments }

func (e *shrinkingEnvironments) List() []Environment {
	list := e.list
	e.list = nil
	return list
}

func TestAHostRemovedAsATaskIsDelegatedThereRefusesTheTask(t *testing.T) {
	f := newFixture(t)
	f.host.envs = &shrinkingEnvironments{fakeEnvironments{list: []Environment{{ID: "devbox", Label: "Dev Box", State: "online"}}}}

	sc, isErr := f.call(t, "delegate_task", `{"task":"x","target":{"environmentId":"devbox"}}`)

	if !isErr || errorCode(sc) != orch.CodeInvalid {
		t.Fatalf("got %v", sc)
	}
}

func TestOnlyASessionWithFullAccessUnlinksAHost(t *testing.T) {
	for _, access := range []string{"ask", "edits", "auto", "full"} {
		f := newFixtureAs(t, "working", access)
		envs := &fakeEnvironments{list: []Environment{{ID: "devbox", Label: "Dev Box", State: "online"}}}
		f.host.envs = envs

		sc, isErr := f.call(t, "anywhere_environment_unlink", `{"environmentId":"devbox"}`)

		if full := access == "full"; isErr == full || full != (len(envs.removed) == 1) || !full && errorCode(sc) != orch.CodeForbidden {
			t.Errorf("%s: %v, removed %q", access, sc, envs.removed)
		}
	}
}

func TestLinkingAHostAlreadyOnlineSaysItIsLinked(t *testing.T) {
	envs := &fakeEnvironments{list: []Environment{{ID: "devbox", Label: "Dev Box", State: "online"}, {ID: "vps", Label: "vps", State: "failed"}}}
	parent := proto.AgentSummary{ID: "parent", TerminalID: "t", Provider: "claude", Access: "full", Status: "working"}
	k := NewToolkit(&host{parent: parent, envs: envs})
	link := func(args string) map[string]any {
		out, _ := k.Call(context.Background(), Grant{Terminal: "t"}, "anywhere_environment_link", json.RawMessage(args))
		got, _ := out["structuredContent"].(map[string]any)
		return got
	}
	if got := link(`{"hint":"Dev Box"}`); got["status"] != "linked" || got["environmentId"] != "devbox" || got["label"] != "Dev Box" || len(envs.cards) != 0 {
		t.Fatalf("linking devbox: %v, cards %v", got, envs.cards)
	}
	if got := link(`{"environmentId":"vps","reason":"The build needs the GPU box."}`); got["status"] != "pending" || len(envs.cards) != 1 || envs.cards[0].Reason != "The build needs the GPU box." {
		t.Fatalf("linking vps, which isn't online: %v, cards %v", got, envs.cards)
	}
}

func TestAForkMergesBackOnlyWhatItSaidSinceTheSplit(t *testing.T) {
	items := []proto.Item{{ID: "inherited", Ts: 50}, {ID: "own", Ts: 150}}
	forks := []orch.Transfer{{Source: "p", Target: "f", Kind: "fork", CreatedAt: 100}}
	ids := func(items []proto.Item) string {
		var out []string
		for _, it := range items {
			out = append(out, it.ID)
		}
		return strings.Join(out, ",")
	}
	if got := ids(sinceSplit(items, forks, "f", "p", &orch.Session{ID: "p"})); got != "own" {
		t.Errorf("fork to its source: %s", got)
	}
	if got := ids(sinceSplit(items, forks, "p", "f", &orch.Session{ID: "f", ParentID: "p", CreatedAt: 120})); got != "own" {
		t.Errorf("source to its fork: %s", got)
	}
	if got := ids(sinceSplit(items, nil, "p", "c", &orch.Session{ID: "c", ParentID: "p", CreatedAt: 100})); got != "own" {
		t.Errorf("parent to its child: %s", got)
	}
}

func TestAWebhookOnlyTaskGetsTheWebhookScheduleAndIsListedWithItsURLButNotItsSecret(t *testing.T) {
	HookBase = "http://127.0.0.1:7123"
	t.Cleanup(func() { HookBase = "" })
	sched := &fakeSchedules{autos: map[string]proto.Automation{}}
	parent := proto.AgentSummary{ID: "parent", TerminalID: "t", Provider: "claude", Access: "edits", Status: "working"}
	k := NewToolkit(&host{parent: parent, sched: sched})
	g := Grant{Terminal: "t"}
	if out, _ := k.Call(context.Background(), g, "schedule_task", json.RawMessage(`{"prompt":"nightly tests","webhook":true,"webhookSecret":"mine"}`)); out["isError"] == true {
		t.Fatalf("%v", out)
	}
	if a := sched.autos["new"]; a.Schedule.Kind != "webhook" || !a.Webhook || !a.Schedule.ValidFor(a.Webhook) {
		t.Fatalf("saved: %+v", a)
	}
	out, _ := k.Call(context.Background(), g, "list_scheduled_tasks", json.RawMessage(`{}`))
	raw, _ := json.Marshal(out)
	if !strings.Contains(string(raw), `"webhookUrl":"http://127.0.0.1:7123/hooks/new"`) || strings.Contains(string(raw), "mine") {
		t.Fatalf("listed: %s", raw)
	}
}

// projectEnvironments has the caller's project on every host.
type projectEnvironments struct{ fakeEnvironments }

func (*projectEnvironments) Projects(context.Context, string) ([]proto.Project, error) {
	return []proto.Project{{Name: "repo", Path: "/repo"}}, nil
}

func TestTaskStatusSaysARunningTasksHostIsUnreachable(t *testing.T) {
	f := newFixture(t)
	envs := &projectEnvironments{fakeEnvironments{list: []Environment{{ID: "devbox", Label: "Dev Box", State: "online"}}}}
	f.host.envs = envs
	task := f.ok(t, "delegate_task", `{"task":"x","target":{"environmentId":"devbox"}}`)["taskId"].(string)
	warnings := func() []any {
		w, _ := f.ok(t, "task_status", fmt.Sprintf(`{"taskId":%q}`, task))["tasks"].([]any)[0].(map[string]any)["warnings"].([]any)
		return w
	}
	if w := warnings(); len(w) != 0 {
		t.Fatalf("online: %v", w)
	}
	envs.list[0].State = "retrying"
	if w := warnings(); len(w) != 1 || w[0] != "Host Dev Box is unreachable; the task keeps waiting." {
		t.Fatalf("retrying: %v", w)
	}
}
