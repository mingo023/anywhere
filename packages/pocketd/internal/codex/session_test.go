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
	mu     sync.Mutex
	events []string
	title  string
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
			line += fmt.Sprintf(":%s:%s%s", e.Detail.Kind, e.Detail.Command, e.Detail.Path)
		}
	case "tool_end":
		line += fmt.Sprintf(":%s:%v", e.ToolUseID, e.OK)
		if e.Output != nil {
			line += ":" + *e.Output
		}
	case "result":
		line += fmt.Sprintf(":%v:%d:%s", e.OK, e.DurationMs, e.Error)
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
	s, err := Open(ctx, srv.Sock, "th1", "agent1", snk, b)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	return srv, s, snk, b
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
	s, err := Open(ctx, srv.Sock, "th1", "agent1", &sink{}, b)
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
