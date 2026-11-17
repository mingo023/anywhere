package codex

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/codex/codextest"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type sink struct {
	mu            sync.Mutex
	events        []string
	title         string
	model, effort string
}

func (s *sink) Apply(e timeline.Event) {
	s.mu.Lock()
	defer s.mu.Unlock()
	line := e.Kind
	switch e.Kind {
	case "user", "assistant_text", "thinking":
		line += ":" + e.Text
	case "tool_start":
		line += ":" + e.Name
		if e.Detail != nil {
			line += fmt.Sprintf(":%s:%s%s%s", e.Detail.Kind, e.Detail.Command, e.Detail.Path, e.Detail.Description)
			if e.Detail.Model != "" {
				line += "@" + e.Detail.Model
			}
		}
	case "tool_tokens":
		line += fmt.Sprintf(":%s:%d", e.ToolUseID, e.Tokens)
	case "tool_background":
		line += ":" + e.ToolUseID
	case "tool_end":
		line += fmt.Sprintf(":%s:%v", e.ToolUseID, e.OK)
		if e.Output != nil {
			line += ":" + *e.Output
		}
	case "result":
		line += fmt.Sprintf(":%v:%d:%s", e.OK, e.DurationMs, e.Error)
		if e.Limit {
			line += ":limit"
		}
	}
	if e.ParentToolUseID != "" {
		line = e.ParentToolUseID + ">" + line
	}
	s.events = append(s.events, line)
}

func (s *sink) SetTitle(title string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.title = title
}

func (s *sink) SetTokens(used, window int64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.events = append(s.events, fmt.Sprintf("tokens:%d:%d", used, window))
}

func (s *sink) SetModel(model string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.model = model
}

func (s *sink) SetEffort(effort string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.effort = effort
}

func (s *sink) wait(t *testing.T, want ...string) {
	t.Helper()
	for range 200 {
		s.mu.Lock()
		got := strings.Join(s.events, "\n")
		s.mu.Unlock()
		if got == strings.Join(want, "\n") {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("events:\n%s\nwant:\n%s", strings.Join(s.events, "\n"), strings.Join(want, "\n"))
}

const history = `{"thread":{"name":"Fix build","turns":[{"id":"t1","status":"completed","durationMs":5,"items":[
 {"type":"userMessage","id":"u1","content":[{"type":"text","text":"run ls","text_elements":[]}]},
 {"type":"reasoning","id":"r1","summary":["Listing files"],"content":[]},
 {"type":"commandExecution","id":"c1","command":"ls","commandActions":[],"cwd":"/w","status":"completed","exitCode":0,"aggregatedOutput":"a.go\n"},
 {"type":"fileChange","id":"f1","status":"failed","changes":[{"path":"/w/a.go","kind":{"type":"update"},"diff":"@@ -1 +1 @@\n-a\n+b\n"}]},
 {"type":"agentMessage","id":"a1","text":"done"}]}]}}`

func open(t *testing.T, answer codextest.Answer) (*codextest.Server, *Session, *sink, *broker.Broker) {
	t.Helper()
	srv := codextest.Start(t, sock(t), answer)
	snk := &sink{}
	b := broker.New(hub.New())
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	s, err := Open(ctx, srv.Sock, "th1", "agent1", snk, b, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	return srv, s, snk, b
}

func TestAThreadItsAgentAlreadyShowsReplaysNoneOfItsHistory(t *testing.T) {
	srv := codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(history), ""
		}
		return map[string]any{}, ""
	})
	snk := &sink{}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	s, err := Open(ctx, srv.Sock, "th1", "agent1", snk, broker.New(hub.New()), true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	snk.mu.Lock()
	defer snk.mu.Unlock()
	if len(snk.events) != 0 || snk.title != "Fix build" {
		t.Fatalf("events %q, title %q", snk.events, snk.title)
	}
}

func TestReplaysHistoryAfterResumeRetries(t *testing.T) {
	ResumeRetry = 10 * time.Millisecond
	fails := 2
	_, _, snk, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			if fails > 0 {
				fails--
				return nil, "no rollout found for thread id th1"
			}
			return json.RawMessage(history), ""
		}
		return map[string]any{}, ""
	})
	snk.wait(t,
		"user:run ls",
		"thinking:Listing files",
		"tool_start:shell:shell:ls",
		"tool_end:c1:true:a.go\n",
		"tool_start:apply_patch:edit:/w/a.go",
		"tool_end:f1:false",
		"assistant_text:done",
		"result:true:5:")
	if snk.title != "Fix build" {
		t.Fatal(snk.title)
	}
}

func emptyThread(method string, _ json.RawMessage) (any, string) {
	if method == "thread/resume" {
		return json.RawMessage(`{"thread":{"turns":[]}}`), ""
	}
	return map[string]any{}, ""
}

func TestLiveTurnStreamsOnce(t *testing.T) {
	srv, _, snk, _ := open(t, emptyThread)
	srv.Push("turn/started", nil, `{"threadId":"th1","turn":{"id":"t2","items":[],"status":"inProgress"}}`)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t2","item":{"type":"userMessage","id":"u2","content":[{"type":"text","text":"hi"}]}}`)
	srv.Push("item/agentMessage/delta", nil, `{"threadId":"th1","turnId":"t2","itemId":"a2","delta":"hel"}`)
	srv.Push("item/agentMessage/delta", nil, `{"threadId":"th1","turnId":"t2","itemId":"a2","delta":"lo"}`)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t2","item":{"type":"agentMessage","id":"a2","text":"hello"}}`)
	srv.Push("thread/name/updated", nil, `{"threadId":"th1","threadName":"Greeting"}`)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":{"id":"t2","items":[],"status":"interrupted"}}`)
	snk.wait(t, "user:hi", "assistant_text:hel", "assistant_text:lo", "result:false:0:interrupted")
	if snk.title != "Greeting" {
		t.Fatal(snk.title)
	}
}

func TestOnlyALiveTurnEndsAtTheAccountsLimit(t *testing.T) {
	const limited = `{"id":"t1","items":[],"status":"failed","error":{"message":"You've hit your usage limit.","codexErrorInfo":"usageLimitExceeded"}}`
	srv, _, live, _ := open(t, emptyThread)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":`+limited+`}`)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":{"id":"t2","items":[],"status":"failed","error":{"message":"boom","codexErrorInfo":{"httpConnectionFailed":{"httpStatusCode":502}}}}}`)
	live.wait(t, "result:false:0:You've hit your usage limit.:limit", "result:false:0:boom")

	_, _, replayed, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(`{"thread":{"turns":[` + limited + `]}}`), ""
		}
		return map[string]any{}, ""
	})
	replayed.wait(t, "result:false:0:You've hit your usage limit.")
}

func TestAMessageUnderWayAtResumeShowsWholeOnceItCompletes(t *testing.T) {
	srv, _, snk, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(`{"thread":{"turns":[{"id":"t1","status":"inProgress","items":[{"type":"userMessage","id":"u1","content":[{"type":"text","text":"count"}]}]}]}}`), ""
		}
		return map[string]any{}, ""
	})
	srv.Push("item/agentMessage/delta", nil, `{"threadId":"th1","turnId":"t1","itemId":"a1","delta":"3"}`)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t1","item":{"type":"agentMessage","id":"a1","text":"123"}}`)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":{"type":"agentMessage","id":"a2","text":""}}`)
	srv.Push("item/agentMessage/delta", nil, `{"threadId":"th1","turnId":"t1","itemId":"a2","delta":"done"}`)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t1","item":{"type":"agentMessage","id":"a2","text":"done"}}`)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":{"id":"t1","items":[],"status":"completed"}}`)
	snk.wait(t, "user:count", "assistant_text:123", "assistant_text:done", "result:true:0:")
}

func TestPhoneAnswersApproval(t *testing.T) {
	for _, c := range []struct{ decision, reply string }{{"allow", `{"decision":"accept"}`}, {"deny", `{"decision":"decline"}`}} {
		t.Run(c.decision, func(t *testing.T) {
			srv, _, _, b := open(t, emptyThread)
			srv.Push("item/commandExecution/requestApproval", 7, `{"threadId":"th1","turnId":"t2","itemId":"c2","startedAtMs":1,"command":"rm -rf build"}`)
			open := waitOpen(t, b)
			if open.AgentID != "agent1" || open.ToolName != "shell" || open.Detail.Command != "rm -rf build" {
				t.Fatalf("%+v", open)
			}
			b.Resolve(open.RequestID, broker.Answer{Decision: c.decision})
			if got := srv.Reply("7"); got != c.reply {
				t.Fatal(got)
			}
		})
	}
}

func TestFileApprovalShowsStartedItem(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t2","item":{"type":"fileChange","id":"f2","status":"inProgress","changes":[{"path":"/w/b.go","kind":{"type":"update"},"diff":"@@ -1 +1 @@\n-a\n+b\n"}]}}`)
	srv.Push("item/fileChange/requestApproval", "r9", `{"threadId":"th1","turnId":"t2","itemId":"f2","startedAtMs":1}`)
	open := waitOpen(t, b)
	if open.ToolName != "apply_patch" || open.Detail.Path != "/w/b.go" || open.Detail.Diff == nil {
		t.Fatalf("%+v", open)
	}
}

func TestDesktopAnswerClosesCard(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("item/commandExecution/requestApproval", "r1", `{"threadId":"th1","turnId":"t2","itemId":"c2","startedAtMs":1,"command":"ls"}`)
	waitOpen(t, b)
	srv.Push("serverRequest/resolved", nil, `{"threadId":"th1","requestId":"r1"}`)
	for range 100 {
		if len(b.Open()) == 0 {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("card still open")
}

func waitOpen(t *testing.T, b *broker.Broker) proto.PermissionRequest {
	t.Helper()
	for range 200 {
		if open := b.Open(); len(open) == 1 {
			return open[0]
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("no open request")
	return proto.PermissionRequest{}
}

const inProgress = `{"thread":{"turns":[{"id":"t4","status":"inProgress","items":[
 {"type":"commandExecution","id":"c4","command":"make","status":"inProgress"},
 {"type":"fileChange","id":"f4","status":"inProgress","changes":[{"path":"/w/c.go","kind":{"type":"update"},"diff":"@@ -1 +1 @@\n-a\n+b\n"}]}]}]}}`

func resumeInProgress(method string, _ json.RawMessage) (any, string) {
	if method == "thread/resume" {
		return json.RawMessage(inProgress), ""
	}
	return map[string]any{}, ""
}

func TestReplayedInProgressToolEndsOnce(t *testing.T) {
	srv, _, snk, _ := open(t, resumeInProgress)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t4","item":{"type":"commandExecution","id":"c4","command":"make","status":"completed","exitCode":0}}`)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t4","item":{"type":"commandExecution","id":"c4","command":"make","status":"completed","exitCode":0}}`)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":{"id":"t4","items":[],"status":"completed"}}`)
	snk.wait(t, "tool_start:shell:shell:make", "tool_start:apply_patch:edit:/w/c.go", "tool_end:c4:true", "result:true:0:")
}

func TestFileApprovalForReplayedItemShowsDiff(t *testing.T) {
	srv, _, _, b := open(t, resumeInProgress)
	srv.Push("item/fileChange/requestApproval", "r4", `{"threadId":"th1","turnId":"t4","itemId":"f4","startedAtMs":1}`)
	open := waitOpen(t, b)
	if open.Detail.Path != "/w/c.go" || open.Detail.Diff == nil {
		t.Fatalf("%+v", open)
	}
}

func TestFileApprovalBeforeReplayShowsDiff(t *testing.T) {
	var srv *codextest.Server
	srv = codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			srv.Push("item/fileChange/requestApproval", "r4", `{"threadId":"th1","turnId":"t4","itemId":"f4","startedAtMs":1}`)
			return json.RawMessage(inProgress), ""
		}
		return map[string]any{}, ""
	})
	b := broker.New(hub.New())
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	s, err := Open(ctx, srv.Sock, "th1", "agent1", &sink{}, b, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	open := waitOpen(t, b)
	if open.Detail.Path != "/w/c.go" || open.Detail.Diff == nil {
		t.Fatalf("%+v", open)
	}
}

func userTurn(id string) string {
	return `{"id":"` + id + `","status":"completed","items":[{"type":"userMessage","id":"u` + id + `","content":[{"type":"text","text":"` + id + `"}]}]}`
}

func TestAPaginatedThreadHydratesFromTurnsList(t *testing.T) {
	var mu sync.Mutex
	var asked []string
	_, _, snk, _ := open(t, func(method string, params json.RawMessage) (any, string) {
		switch method {
		case "thread/resume":
			return json.RawMessage(`{"thread":{"turns":[` + userTurn("t3") + `]},"turnsBackwardsCursor":"k3"}`), ""
		case "thread/turns/list":
			mu.Lock()
			asked = append(asked, string(params))
			mu.Unlock()
			if strings.Contains(string(params), `"k3"`) {
				return json.RawMessage(`{"data":[` + userTurn("t3") + `,` + userTurn("t2") + `],"nextCursor":"k1"}`), ""
			}
			return json.RawMessage(`{"data":[` + userTurn("t1") + `],"nextCursor":null}`), ""
		}
		return map[string]any{}, ""
	})
	snk.wait(t, "user:t1", "result:true:0:", "user:t2", "result:true:0:", "user:t3", "result:true:0:")
	want := `{"cursor":"k3","itemsView":"full","sortDirection":"desc","threadId":"th1"}`
	if len(asked) != 2 || asked[0] != want {
		t.Fatalf("turns/list params = %q", asked)
	}
}

func TestHydrationStopsAtMaxTurnPages(t *testing.T) {
	old := MaxTurnPages
	MaxTurnPages = 2
	t.Cleanup(func() { MaxTurnPages = old })
	pages := 0
	_, _, snk, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		switch method {
		case "thread/resume":
			return json.RawMessage(`{"thread":{"turns":[]},"turnsBackwardsCursor":"k"}`), ""
		case "thread/turns/list":
			pages++
			return json.RawMessage(`{"data":[` + userTurn(fmt.Sprint("p", pages)) + `],"nextCursor":"k"}`), ""
		}
		return map[string]any{}, ""
	})
	snk.wait(t, "user:p2", "result:true:0:", "user:p1", "result:true:0:")
}

func TestResumeReplayEqualsTheLiveTimeline(t *testing.T) {
	const (
		user  = `{"type":"userMessage","id":"u1","content":[{"type":"text","text":"run ls"}]}`
		cmd   = `{"type":"commandExecution","id":"c1","command":"ls","status":"completed","exitCode":0,"aggregatedOutput":"a.go\n"}`
		reply = `{"type":"agentMessage","id":"a1","text":"done"}`
		done  = `{"id":"t1","status":"completed","durationMs":5,"items":[` + user + `,` + cmd + `,` + reply + `]}`
	)
	srv, _, live, _ := open(t, emptyThread)
	srv.Push("turn/started", nil, `{"threadId":"th1","turn":{"id":"t1","items":[],"status":"inProgress"}}`)
	for _, it := range []string{user, cmd, reply} {
		srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+it+`}`)
		srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t1","item":`+it+`}`)
	}
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":`+done+`}`)
	want := []string{"user:run ls", "tool_start:shell:shell:ls", "tool_end:c1:true:a.go\n", "assistant_text:done", "result:true:5:"}
	live.wait(t, want...)

	_, _, replayed, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(`{"thread":{"turns":[` + done + `]}}`), ""
		}
		return map[string]any{}, ""
	})
	replayed.wait(t, want...)
}

func TestTokenUsageReportsTheLastTurnAndTheWindow(t *testing.T) {
	srv, _, snk, _ := open(t, emptyThread)
	usage := `{"threadId":"th1","turnId":"t2","tokenUsage":{"total":{"totalTokens":9000,"inputTokens":8000,"cachedInputTokens":0,"cacheWriteInputTokens":0,"outputTokens":1000,"reasoningOutputTokens":0},"last":{"totalTokens":%d,"inputTokens":0,"cachedInputTokens":0,"cacheWriteInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},"modelContextWindow":%s}}`
	srv.Push("thread/tokenUsage/updated", nil, fmt.Sprintf(usage, 1200, "258400"))
	srv.Push("thread/tokenUsage/updated", nil, fmt.Sprintf(usage, 1500, "null"))
	snk.wait(t, "tokens:1200:258400", "tokens:1500:0")
}

func TestResumeNamesTheThreadsModelAndEffort(t *testing.T) {
	_, _, snk, _ := open(t, func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(`{"thread":{"turns":[]},"model":"gpt-6-sol","reasoningEffort":"xhigh"}`), ""
		}
		return map[string]any{}, ""
	})
	snk.mu.Lock()
	defer snk.mu.Unlock()
	if snk.model != "gpt-6-sol" || snk.effort != "xhigh" {
		t.Fatalf("model %q, effort %q", snk.model, snk.effort)
	}
}

const (
	spawned   = `{"type":"subAgentActivity","id":"sa1","kind":"started","agentThreadId":"th2","agentPath":"/root/count_a","model":"gpt-6.1-sol"}`
	finished  = `{"type":"subAgentActivity","id":"sa1-done","kind":"completed","agentThreadId":"th2","agentPath":"/root/count_a"}`
	agentCmd  = `{"type":"commandExecution","id":"c2","command":"wc -l a.txt","status":"completed","exitCode":0,"aggregatedOutput":"3 a.txt\n"}`
	agentSaid = `{"type":"agentMessage","id":"a2","text":"3"}`
)

func TestASubAgentsWorkNestsUnderTheCallThatSpawnedIt(t *testing.T) {
	srv, _, snk, _ := open(t, emptyThread)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+spawned+`}`)
	srv.Push("item/completed", nil, `{"threadId":"th1","turnId":"t1","item":`+spawned+`}`)
	srv.Push("turn/started", nil, `{"threadId":"th2","turn":{"id":"t2","items":[],"status":"inProgress"}}`)
	srv.Push("item/started", nil, `{"threadId":"th2","turnId":"t2","item":`+agentCmd+`}`)
	srv.Push("item/completed", nil, `{"threadId":"th2","turnId":"t2","item":`+agentCmd+`}`)
	srv.Push("item/agentMessage/delta", nil, `{"threadId":"th2","turnId":"t2","itemId":"a2","delta":"3"}`)
	srv.Push("item/completed", nil, `{"threadId":"th2","turnId":"t2","item":`+agentSaid+`}`)
	srv.Push("thread/tokenUsage/updated", nil, `{"threadId":"th2","turnId":"t2","tokenUsage":{"last":{"totalTokens":20452},"modelContextWindow":258400}}`)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+finished+`}`)
	srv.Push("turn/completed", nil, `{"threadId":"th2","turn":{"id":"t2","items":[],"status":"completed"}}`)
	srv.Push("item/completed", nil, `{"threadId":"th9","turnId":"t9","item":{"type":"agentMessage","id":"a9","text":"not ours"}}`)
	srv.Push("turn/completed", nil, `{"threadId":"th1","turn":{"id":"t1","items":[],"status":"completed","durationMs":9}}`)
	snk.wait(t,
		"tool_start:spawn_agent:task:count_a@gpt-6.1-sol",
		"tool_background:sa1",
		"sa1>tool_start:shell:shell:wc -l a.txt",
		"sa1>tool_end:c2:true:3 a.txt\n",
		"sa1>assistant_text:3",
		"tool_tokens:sa1:20452",
		"tool_end:sa1:true:3",
		"result:true:9:")
}

func TestASubAgentWhoseTurnFailedEndsItsCallFailed(t *testing.T) {
	srv, _, snk, _ := open(t, emptyThread)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+spawned+`}`)
	srv.Push("turn/completed", nil, `{"threadId":"th2","turn":{"id":"t2","items":[],"status":"failed","error":{"message":"boom"}}}`)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+finished+`}`)
	snk.wait(t, "tool_start:spawn_agent:task:count_a@gpt-6.1-sol", "tool_background:sa1", "tool_end:sa1:false:boom")
}

func TestAResumedThreadReplaysEachSubAgentsWorkFromItsOwnThread(t *testing.T) {
	_, _, snk, _ := open(t, func(method string, params json.RawMessage) (any, string) {
		switch method {
		case "thread/resume":
			return json.RawMessage(`{"thread":{"turns":[{"id":"t1","status":"completed","durationMs":9,"items":[` + spawned + `,` + finished + `]}]}}`), ""
		case "thread/read":
			if !strings.Contains(string(params), `"th2"`) {
				return nil, "no such thread"
			}
			return json.RawMessage(`{"thread":{"turns":[{"id":"t2","status":"completed","items":[` + agentCmd + `,` + agentSaid + `]}]}}`), ""
		}
		return map[string]any{}, ""
	})
	snk.wait(t,
		"tool_start:spawn_agent:task:count_a@gpt-6.1-sol",
		"tool_background:sa1",
		"sa1>tool_start:shell:shell:wc -l a.txt",
		"sa1>tool_end:c2:true:3 a.txt\n",
		"sa1>assistant_text:3",
		"tool_end:sa1:true:3",
		"result:true:9:")
}

func TestASubAgentsApprovalClosesItsCardWhenItsThreadResolvesIt(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t1","item":`+spawned+`}`)
	srv.Push("item/commandExecution/requestApproval", "r1", `{"threadId":"th2","turnId":"t2","itemId":"c2","command":"rm -rf build"}`)
	waitOpen(t, b)
	srv.Push("serverRequest/resolved", nil, `{"threadId":"th2","requestId":"r1"}`)
	for range 200 {
		if len(b.Open()) == 0 {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("the card stayed open")
}

func TestCodexUserInputAsksAsAQuestionAndAnswersByID(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("item/tool/requestUserInput", 0, `{"threadId":"th1","turnId":"t2","itemId":"call_1","questions":[{"id":"schema_preference","header":"Fixture","question":"Strict schemas or UI flexibility?","isOther":true,"options":[{"label":"Strict schemas","description":"Validated"},{"label":"UI flexibility"}]}],"isBlocking":true}`)
	open := waitOpen(t, b)
	if open.ToolName != "AskUserQuestion" || !strings.Contains(string(open.Detail.Input), "Strict schemas or UI flexibility?") {
		t.Fatalf("%+v", open)
	}
	b.Resolve(open.RequestID, broker.Answer{Decision: "allow", Answers: map[string]string{"Strict schemas or UI flexibility?": "Strict schemas"}})
	if got := srv.Reply("0"); got != `{"answers":{"schema_preference":{"answers":["Strict schemas"]}}}` {
		t.Fatal(got)
	}
}

// toolCallApproval is the mcpServer/elicitation/request codex-cli 0.163 sends before an MCP tool call.
func toolCallApproval(server string) string {
	return `{"threadId":"th1","turnId":"t2","serverName":"` + server + `","mode":"form","message":"Allow the ` + server + ` MCP server to run tool \"create_issue\"?",` +
		`"requestedSchema":{"type":"object","properties":{}},"_meta":{"codex_approval_kind":"mcp_tool_call","persist":["session","always"],"tool_name":"create_issue","tool_params":{"title":"x"}}}`
}

func TestAToolCallOfPocketdsOwnServersRunsUnaskedInAChat(t *testing.T) {
	for _, server := range []string{"anywhere", "anywhere-direct"} {
		t.Run(server, func(t *testing.T) {
			srv, b := chat(t)
			srv.Push("mcpServer/elicitation/request", 3, toolCallApproval(server))
			if got := srv.Reply("3"); got != `{"action":"accept","content":{}}` {
				t.Fatal(got)
			}
			if open := b.Open(); len(open) != 0 {
				t.Fatalf("asked: %+v", open)
			}
		})
	}
}

func TestAnotherServersToolCallIsAskedAndAnsweredAsAnElicitationAction(t *testing.T) {
	for _, c := range []struct{ decision, reply string }{{"allow", `{"action":"accept","content":{}}`}, {"deny", `{"action":"decline"}`}} {
		t.Run(c.decision, func(t *testing.T) {
			srv, _, _, b := open(t, emptyThread)
			srv.Push("mcpServer/elicitation/request", 4, toolCallApproval("github"))
			open := waitOpen(t, b)
			if open.ToolName != "github.create_issue" || open.Detail.Kind != "other" || string(open.Detail.Input) != `{"title":"x"}` {
				t.Fatalf("%+v", open)
			}
			b.Resolve(open.RequestID, broker.Answer{Decision: c.decision})
			if got := srv.Reply("4"); got != c.reply {
				t.Fatal(got)
			}
		})
	}
}

func TestAYesOrNoElicitationWithoutAToolIsAskedByItsServerAndMessage(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("mcpServer/elicitation/request", 7, `{"threadId":"th1","serverName":"github","mode":"form","message":"Go on?","requestedSchema":{"type":"object","properties":{}}}`)
	open := waitOpen(t, b)
	if open.ToolName != "github" || string(open.Detail.Input) != `{"message":"Go on?"}` {
		t.Fatalf("%+v", open)
	}
}

func TestAChatDeclinesAnElicitationThatWantsAFormFilled(t *testing.T) {
	srv, b := chat(t)
	srv.Push("mcpServer/elicitation/request", 5, `{"threadId":"th1","turnId":null,"serverName":"github","mode":"form","message":"Which repo?",`+
		`"requestedSchema":{"type":"object","properties":{"repo":{"type":"string"}},"required":["repo"]}}`)
	if got := srv.Reply("5"); got != `{"action":"decline"}` {
		t.Fatal(got)
	}
	if open := b.Open(); len(open) != 0 {
		t.Fatalf("asked: %+v", open)
	}
}

func TestAPermissionsRequestGrantsWhatCodexAskedForOrNothing(t *testing.T) {
	asked := `{"network":{"enabled":true},"fileSystem":{"write":["/w/out"]}}`
	for _, c := range []struct{ decision, reply string }{{"allow", `{"permissions":` + asked + `}`}, {"deny", `{"permissions":{}}`}} {
		t.Run(c.decision, func(t *testing.T) {
			srv, _, _, b := open(t, emptyThread)
			srv.Push("item/permissions/requestApproval", 6, `{"threadId":"th1","turnId":"t2","itemId":"c2","cwd":"/w","startedAtMs":1,"reason":"fetch deps","permissions":`+asked+`}`)
			open := waitOpen(t, b)
			if open.ToolName != "permissions" || string(open.Detail.Input) != asked {
				t.Fatalf("%+v", open)
			}
			b.Resolve(open.RequestID, broker.Answer{Decision: c.decision})
			if got := srv.Reply("6"); got != c.reply {
				t.Fatal(got)
			}
		})
	}
}

// chat is a Session that drives its thread, as a chat does, on a fresh app-server.
func chat(t *testing.T) (*codextest.Server, *broker.Broker) {
	t.Helper()
	srv := codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/start" {
			return json.RawMessage(`{"thread":{"id":"th1","turns":[]}}`), ""
		}
		return map[string]any{}, ""
	})
	b := broker.New(hub.New())
	s := NewSession("agent1", &sink{}, b)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	c, err := Dial(ctx, srv.Sock, s)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	res, err := c.Call(ctx, "thread/start", map[string]any{})
	if err != nil {
		t.Fatal(err)
	}
	s.Replay(ctx, c, res)
	return srv, b
}

func TestAChatRefusesAServerRequestItDoesNotKnow(t *testing.T) {
	srv, _ := chat(t)
	srv.Push("item/tool/call", 8, `{"threadId":"th1","turnId":"t2","callId":"call_1","tool":"lookup","arguments":{}}`)
	if f := srv.NextReply(); string(f.ID) != "8" || string(f.Error) != `{"code":-32601,"message":"item/tool/call is not supported"}` {
		t.Fatalf("reply %s: result %s, error %s", f.ID, f.Result, f.Error)
	}
}

func TestBesideTheTUIAServerRequestPocketdDoesNotKnowIsLeftToTheTUI(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	srv.Push("item/tool/call", 8, `{"threadId":"th1","turnId":"t2","callId":"call_1","tool":"lookup","arguments":{}}`)
	srv.Push("item/commandExecution/requestApproval", 9, `{"threadId":"th1","turnId":"t2","itemId":"c2","startedAtMs":1,"command":"ls"}`)
	b.Resolve(waitOpen(t, b).RequestID, broker.Answer{Decision: "allow"})
	if f := srv.NextReply(); string(f.ID) != "9" {
		t.Fatalf("first reply was to %s: %s%s", f.ID, f.Result, f.Error)
	}
}

func TestOpeningAThreadWithNoFirstTurnFailsWhenItsContextEnds(t *testing.T) {
	srv := codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return nil, "no rollout found"
		}
		return map[string]any{}, ""
	})
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	if _, err := Open(ctx, srv.Sock, "th1", "agent1", &sink{}, broker.New(hub.New()), false); err == nil {
		t.Fatal("opened")
	}
}
