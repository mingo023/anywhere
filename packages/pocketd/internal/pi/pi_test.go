package pi

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/timeline"
)

// sink writes down what a Session makes of its agent, a line each.
type sink struct {
	mu   sync.Mutex
	said []string
}

func (s *sink) add(format string, a ...any) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.said = append(s.said, fmt.Sprintf(format, a...))
}

func line(e timeline.Event) string {
	line := e.Kind
	switch e.Kind {
	case "user", "assistant_text", "thinking", "assistant_draft", "thinking_draft", "notice":
		line += ":" + e.Text
	case "tool_start":
		line += ":" + e.ToolUseID + ":" + e.Name
		if d := e.Detail; d != nil {
			line += ":" + d.Kind + ":" + d.Command + d.Path + d.Query + d.Description
			if d.Diff != nil {
				line += fmt.Sprintf(":+%d-%d", d.Diff.Additions, d.Diff.Deletions)
			}
			if d.Input != nil {
				line += ":" + string(d.Input)
			}
		}
		if e.Input != nil {
			line += ":" + string(e.Input)
		}
	case "tool_end":
		line += fmt.Sprintf(":%s:%v", e.ToolUseID, e.OK)
		if e.Output != nil {
			line += ":" + *e.Output
		}
	case "result":
		line += fmt.Sprintf(":%v:%s", e.OK, e.Error)
		if e.Usage != nil {
			line += fmt.Sprintf(":%d/%d/%d", e.Usage.InputTokens, e.Usage.OutputTokens, e.Usage.CacheReadTokens)
		}
	case "compacted":
		line += ":" + e.Trigger
	case "tool_background":
		line += ":" + e.ToolUseID
	case "tool_model":
		line += ":" + e.ToolUseID + ":" + e.Name
	case "tool_tokens":
		line += fmt.Sprintf(":%s:%d", e.ToolUseID, e.Tokens)
	}
	if e.ParentToolUseID != "" {
		line = e.ParentToolUseID + ">" + line
	}
	return line
}

func (s *sink) Apply(e timeline.Event)       { s.add("%s", line(e)) }
func (s *sink) SetTitle(title string)        { s.add("title:%s", title) }
func (s *sink) SetTokens(used, window int64) { s.add("tokens:%d/%d", used, window) }
func (s *sink) SetModel(model string)        { s.add("model:%s", model) }
func (s *sink) SetEffort(effort string)      { s.add("effort:%s", effort) }
func (s *sink) Bound(session string)         { s.add("bound:%s", session) }
func (s *sink) Working()                     { s.add("working") }
func (s *sink) NeedsYou()                    { s.add("needsYou") }
func (s *sink) TurnEnded(failed bool)        { s.add("ended:%v", failed) }
func (s *sink) Stopped()                     { s.add("stopped") }
func (s *sink) Compacting()                  { s.add("compacting") }
func (s *sink) Compacted()                   { s.add("compactingEnded") }

func (s *sink) SetOptions(o map[string]string) { s.add("options:%v", o) }

func (s *sink) lines() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.said)
}

// wait returns what the sink wrote down once it holds line.
func (s *sink) wait(t *testing.T, line string) []string {
	t.Helper()
	return s.waitN(t, line, 1)
}

// waitN returns what the sink wrote down once it holds line n times.
func (s *sink) waitN(t *testing.T, line string, n int) []string {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		got := s.lines()
		if count := len(got) - len(slices.DeleteFunc(slices.Clone(got), func(l string) bool { return l == line })); count >= n {
			return got
		}
	}
	t.Fatalf("no %d %q in:\n%s", n, line, strings.Join(s.lines(), "\n"))
	return nil
}

// agent is the other end of a Session's wire, scripted by a test: answer
// gives each command its data, or "!" and the error it is refused with,
// after saying the events the agent writes first.
type agent struct {
	t      *testing.T
	in     chan []byte // to the client
	out    chan []byte // from the client
	closed chan struct{}
	once   sync.Once
	broken atomic.Bool // its pipe takes no more

	mu     sync.Mutex
	wrote  []string // each record the client wrote, as "type fields", once it is answered
	ids    []string // the id of each
	answer func(a *agent, kind string, cmd map[string]any) string
}

func (a *agent) Read(ctx context.Context) ([]byte, error) {
	select {
	case b := <-a.in:
		return b, nil
	case <-a.closed:
		return nil, errors.New("closed")
	}
}

func (a *agent) Write(_ context.Context, msg []byte) error {
	if a.broken.Load() {
		return errors.New("broken pipe")
	}
	select {
	case a.out <- msg:
	case <-a.closed:
	}
	return nil
}

func (a *agent) Close() { a.once.Do(func() { close(a.closed) }) }

// say writes records to the client, as the agent would.
func (a *agent) say(records ...string) {
	for _, r := range records {
		select {
		case a.in <- []byte(r):
		case <-a.closed:
		}
	}
}

// serve answers the commands in the order they came, as pi does.
func (a *agent) serve() {
	for {
		var raw []byte
		select {
		case raw = <-a.out:
		case <-a.closed:
			return
		}
		var cmd map[string]any
		json.Unmarshal(raw, &cmd)
		kind, _ := cmd["type"].(string)
		delete(cmd, "type")
		id, _ := cmd["id"].(string)
		if kind != "extension_ui_response" {
			if delete(cmd, "id"); id == "" {
				a.t.Errorf("no id in %s", raw)
			}
			result := a.answer(a, kind, cmd)
			if refusal, refused := strings.CutPrefix(result, "!"); refused {
				a.say(fmt.Sprintf(`{"type":"response","id":%q,"command":%q,"success":false,"error":%q}`, id, kind, refusal))
			} else {
				a.say(fmt.Sprintf(`{"type":"response","id":%q,"command":%q,"success":true,"data":%s}`, id, kind, cmp.Or(result, "null")))
			}
		}
		fields, _ := json.Marshal(cmd)
		a.mu.Lock()
		a.wrote = append(a.wrote, kind+" "+string(fields))
		a.ids = append(a.ids, id)
		a.mu.Unlock()
	}
}

// kinds is the type of each record the client wrote, but the stats it reads after a turn.
func (a *agent) kinds() []string {
	a.mu.Lock()
	defer a.mu.Unlock()
	var kinds []string
	for _, w := range a.wrote {
		if kind, _, _ := strings.Cut(w, " "); kind != "get_session_stats" {
			kinds = append(kinds, kind)
		}
	}
	return kinds
}

// sent returns the fields of the records of kind the client wrote, once there are n.
func (a *agent) sent(kind string, n int) []string {
	a.t.Helper()
	var got []string
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		a.mu.Lock()
		got = nil
		for _, w := range a.wrote {
			if k, fields, _ := strings.Cut(w, " "); k == kind {
				got = append(got, fields)
			}
		}
		a.mu.Unlock()
		if len(got) >= n {
			return got
		}
	}
	a.t.Fatalf("%d of %d %s", len(got), n, kind)
	return nil
}

// fresh is what pi 1.1.0 answered get_state with on a new session.
const fresh = `{"model":{"id":"deepseek-v4-pro","name":"DeepSeek V4 Pro","api":"openai-completions","baseUrl":"https://api.deepseek.com","provider":"deepseek","reasoning":true,"input":["text"],"cost":{"input":1.32,"output":3.96,"cacheRead":0.044,"cacheWrite":0},"contextWindow":1000000,"maxTokens":384000,"thinkingLevelMap":{"minimal":null,"low":null,"medium":null,"high":"high","max":"max"},"type":"chat"},"thinkingLevel":"off","isStreaming":false,"isCompacting":false,"steeringMode":"all","followUpMode":"one-at-a-time","sessionFile":"/tmp/agent-probe/pi-sessions/2026-11-11T04-30-48-550Z_01a1c8df-6ee5-73f4-9369-d38f92ac7a6a.jsonl","sessionId":"01a1c8df-6ee5-73f4-9369-d38f92ac7a6a","autoCompactionEnabled":true,"messageCount":0,"pendingMessageCount":0}`

// The reply pi 1.1.0 ended with when its provider refused the request, and the turn around it.
const (
	noBalance = `{"role":"assistant","content":[],"api":"openai-completions","provider":"deepseek","model":"deepseek-v4-pro","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"error","timestamp":1794371448642,"errorMessage":"402: {\"message\":\"Insufficient Balance (request_id: 00a9f247-b478-4352-8991-a6bd9eb3db6f)\",\"type\":\"insufficient_balance_error\",\"param\":null,\"code\":\"invalid_request_error\"}","durationMs":705,"thinkingLevel":"off"}`
	system    = `{"role":"system","content":"","sections":{"preamble":"You are an expert coding assistant operating inside pi, a coding agent harness."},"timestamp":1794371448628,"toolsAdded":[{"name":"read","description":"Read the contents of a file."}]}`
	asked     = `{"role":"user","content":[{"type":"text","text":"reply with the single word ok"}],"timestamp":1794371448628}`
	refused   = `402: {"message":"Insufficient Balance (request_id: 00a9f247-b478-4352-8991-a6bd9eb3db6f)","type":"insufficient_balance_error","param":null,"code":"invalid_request_error"}`
)

const (
	agentStart = `{"type":"agent_start"}`
	agentEnd   = `{"type":"agent_end","messages":[],"willRetry":false}`
	settled    = `{"type":"agent_settled","aborted":false}`
	reply      = `{"type":"message_start","message":{"role":"assistant","content":[]}}`
)

// echoed is pi reading a prompt.
func echoed(text string) string {
	return fmt.Sprintf(`{"type":"message_start","message":{"role":"user","content":[{"type":"text","text":%q}]}}`, text)
}

// update is a part of the reply pi is writing.
func update(kind string, index int, rest string) string {
	return fmt.Sprintf(`{"type":"message_update","message":{"role":"assistant","content":[]},"assistantMessageEvent":{"type":%q,"contentIndex":%d,%s}}`, kind, index, rest)
}

// ended is a reply pi ended, of blocks.
func ended(blocks, rest string) string {
	return fmt.Sprintf(`{"type":"message_end","message":{"role":"assistant","content":[%s],%s}}`, blocks, cmp.Or(rest, `"stopReason":"stop"`))
}

func said(text string) string { return ended(fmt.Sprintf(`{"type":"text","text":%q}`, text), "") }

type opts struct {
	provider string
	state    string
	launch   state.Launch
	midTurn  bool
	// answer gives a command its data; "" leaves it to the session's state and a started prompt.
	answer func(a *agent, kind string, cmd map[string]any) string
}

// start opens a Session against a scripted agent.
func start(t *testing.T, o opts) (*Session, *agent, *sink, *broker.Broker) {
	t.Helper()
	s, a, sk, b := scripted(t, o)
	if _, err := s.Open(a, o.launch, o.midTurn); err != nil {
		t.Fatal(err)
	}
	return s, a, sk, b
}

// scripted is a Session and the scripted agent it has yet to open on.
func scripted(t *testing.T, o opts) (*Session, *agent, *sink, *broker.Broker) {
	t.Helper()
	a := &agent{t: t, in: make(chan []byte, 64), out: make(chan []byte, 64), closed: make(chan struct{})}
	a.answer = func(a *agent, kind string, cmd map[string]any) string {
		if o.answer != nil {
			if r := o.answer(a, kind, cmd); r != "" {
				return r
			}
		}
		switch kind {
		case "get_state":
			return cmp.Or(o.state, fresh)
		case "prompt":
			return `{"disposition":"started"}`
		}
		return ""
	}
	go a.serve()
	sk := &sink{}
	b := broker.New(hub.New())
	s := NewSession("a1", cmp.Or(o.provider, "pi"), sk, b)
	t.Cleanup(s.Close)
	t.Cleanup(a.Close)
	return s, a, sk, b
}

// after is lines from the first that is from on.
func after(lines []string, from string) []string {
	if i := slices.Index(lines, from); i >= 0 {
		return lines[i:]
	}
	return nil
}

func equal(t *testing.T, got, want []string) {
	t.Helper()
	if !slices.Equal(got, want) {
		t.Fatalf("got\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}

// shown is the one request the broker holds open, once it does.
func shown(t *testing.T, b *broker.Broker) proto.PermissionRequest {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		if open := b.Open(); len(open) == 1 {
			return open[0]
		}
	}
	t.Fatal("nothing asked")
	return proto.PermissionRequest{}
}

func TestAProviderErrorFailsTheTurnWithPisMessage(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	if err := s.Prompt("reply with the single word ok"); err != nil {
		t.Fatal(err)
	}
	equal(t, a.sent("prompt", 1), []string{`{"message":"reply with the single word ok"}`})
	a.say(agentStart, `{"type":"turn_start"}`,
		`{"type":"message_start","message":`+system+`}`, `{"type":"message_end","message":`+system+`}`,
		`{"type":"message_start","message":`+asked+`}`, `{"type":"message_end","message":`+asked+`}`,
		`{"type":"message_start","message":`+noBalance+`}`, `{"type":"message_end","message":`+noBalance+`}`,
		`{"type":"turn_end","message":`+noBalance+`,"toolResults":[]}`,
		`{"type":"agent_end","messages":[`+system+`,`+asked+`,`+noBalance+`],"willRetry":false}`,
		settled)
	equal(t, sk.wait(t, "ended:true"), []string{
		"bound:01a1c8df-6ee5-73f4-9369-d38f92ac7a6a",
		"model:deepseek/deepseek-v4-pro",
		"working",
		"user:reply with the single word ok",
		"result:false:" + refused,
		"ended:true",
	})
}

func TestAReplyDraftsAsItStreamsThenStandsWholeWithWhatItCost(t *testing.T) {
	s, a, sk, _ := start(t, opts{answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "get_session_stats" {
			return `{"sessionId":"s1","contextUsage":{"tokens":1460,"contextWindow":1000000,"percent":0.146}}`
		}
		return ""
	}})
	s.Prompt("hi")
	a.sent("prompt", 1)
	a.say(agentStart, echoed("hi"), reply,
		update("thinking_start", 0, `"partial":{}`),
		update("thinking_delta", 0, `"delta":"Let me"`), update("thinking_delta", 0, `"delta":" see"`),
		update("thinking_end", 0, `"content":"Let me see"`),
		update("text_delta", 1, `"delta":"he"`), update("text_delta", 1, `"delta":"llo"`),
		update("text_end", 1, `"content":"hello"`),
		ended(`{"type":"thinking","thinking":"Let me see"},{"type":"text","text":"hello"}`,
			`"stopReason":"stop","usage":{"input":10,"output":5,"cacheRead":2,"cacheWrite":0,"totalTokens":17}`),
		agentEnd, settled)
	equal(t, after(sk.wait(t, "tokens:1460/1000000"), "working"), []string{
		"working",
		"user:hi",
		"thinking_draft:Let me", "thinking_draft: see", "thinking:Let me see",
		"assistant_draft:he", "assistant_draft:llo", "assistant_text:hello",
		"tokens:17/1000000",
		"result:true::10/5/2",
		"ended:false",
		"tokens:1460/1000000",
	})
}

func TestAToolCallShowsOnceWithItsCardAndWhatItPrinted(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("look")
	a.sent("prompt", 1)
	a.say(agentStart, echoed("look"), reply,
		update("toolcall_end", 0, `"toolCall":{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"ls"}}`),
		ended(`{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"ls"}}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"bash","args":{"command":"ls"}}`,
		`{"type":"tool_execution_update","toolCallId":"t1","toolName":"bash","args":{"command":"ls"},"partialResult":{"content":[{"type":"text","text":"a"}]}}`,
		`{"type":"tool_execution_end","toolCallId":"t1","toolName":"bash","result":{"content":[{"type":"text","text":"a.txt\n"}],"details":{}},"isError":false}`,
		// A call told only as it runs, with its arguments as JSON in a string, which fails.
		reply,
		`{"type":"tool_execution_start","toolCallId":"t2","toolName":"read","args":"{\"path\":\"b.txt\"}"}`,
		`{"type":"tool_execution_end","toolCallId":"t2","toolName":"read","result":{"content":[{"type":"text","text":"no such file"}]},"isError":true}`,
		`{"type":"tool_execution_start","toolCallId":"t3","toolName":"todo_write","args":{"todos":[{"content":"a","status":"in-progress"},{"text":"b","status":"done"},{"content":"c"}]}}`,
		`{"type":"tool_execution_end","toolCallId":"t3","toolName":"todo_write","result":{"content":[]},"isError":false}`,
		said("done"), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:look"), []string{
		"user:look",
		"tool_start:t1:shell:shell:ls",
		"tool_end:t1:true:a.txt\n",
		"tool_start:t2:read:read:b.txt",
		"tool_end:t2:false:no such file",
		`tool_start:t3:TodoWrite:{"todos":[{"content":"a","status":"in_progress"},{"content":"b","status":"completed"},{"content":"c","status":"pending"}]}`,
		"tool_end:t3:true",
		"assistant_text:done",
		"result:true:",
		"ended:false",
	})
}

func TestACallInALaterTurnShowsThoughItReusesAnEarlierCallsID(t *testing.T) {
	s, a, sk, _ := start(t, opts{state: strings.Replace(fresh, `"messageCount":0`, `"messageCount":3`, 1), answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "get_messages" {
			return `{"messages":[{"role":"user","content":"look"},
				{"role":"assistant","content":[{"type":"toolCall","id":"call_1","name":"bash","arguments":{"command":"ls"}}],"stopReason":"toolUse"},
				{"role":"toolResult","toolCallId":"call_1","toolName":"bash","content":[],"isError":false},
				{"role":"assistant","content":[{"type":"text","text":"done"}],"stopReason":"stop"}]}`
		}
		return ""
	}})
	call := func(text string) {
		a.say(agentStart, echoed(text), reply,
			`{"type":"tool_execution_start","toolCallId":"call_1","toolName":"bash","args":{"command":"ls"}}`,
			`{"type":"tool_execution_end","toolCallId":"call_1","toolName":"bash","result":{"content":[]},"isError":false}`,
			agentEnd, settled)
	}
	s.Prompt("one")
	a.sent("prompt", 1)
	call("one")
	equal(t, after(sk.wait(t, "ended:false"), "user:one"), []string{
		"user:one", "tool_start:call_1:shell:shell:ls", "tool_end:call_1:true", "result:true:", "ended:false"})
	s.Prompt("two")
	a.sent("prompt", 2)
	call("two")
	equal(t, after(sk.waitN(t, "ended:false", 2), "user:two"), []string{
		"user:two", "tool_start:call_1:shell:shell:ls", "tool_end:call_1:true", "result:true:", "ended:false"})
}

func TestAToolIsDrawnByItsName(t *testing.T) {
	for _, c := range []struct{ name, args, want string }{
		{"bash", `{"command":"ls","description":"list"}`, "shell:shell:lslist"},
		{"read", `{"path":"a.go"}`, "read:read:a.go"},
		{"write", `{"path":"a.go","content":"a\nb\n"}`, "write:write:a.go:+2-0"},
		{"edit", `{"path":"a.go","edits":[{"oldText":"a","newText":"b\nc"}]}`, "edit:edit:a.go:+2-1"},
		{"edit", `{"path":"a.go","oldText":"a","newText":"b"}`, "edit:edit:a.go:+1-1"},
		{"edit", `{"path":"a.go"}`, "edit:edit:a.go"},
		{"grep", `{"pattern":"TODO","path":"src"}`, "search:search:srcTODO"},
		{"find", `{"pattern":"*.go"}`, "search:search:*.go"},
		{"ls", `{"path":"src"}`, "search:search:src"},
		{"web_search", `{"query":"pi rpc"}`, "search:search:pi rpc"},
		{"deploy", `{"env":"prod"}`, `deploy:other::{"env":"prod"}`},
		{"list_issues", `{}`, "list_issues:other:"},
	} {
		name, detail := describe(c.name, json.RawMessage(c.args))
		if got := strings.TrimPrefix(line(timeline.Event{Kind: "tool_start", Name: name, Detail: &detail}), "tool_start::"); got != c.want {
			t.Errorf("%s %s: got %s, want %s", c.name, c.args, got, c.want)
		}
	}
}

func TestAnErrorPiTriesAgainAfterIsANoticeAndNotTheTurns(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("hi")
	a.sent("prompt", 1)
	overloaded := ended("", `"stopReason":"error","errorMessage":"529 overloaded"`)
	a.say(agentStart, echoed("hi"), reply, overloaded,
		`{"type":"agent_end","messages":[],"willRetry":true}`,
		`{"type":"auto_retry_start","attempt":1,"maxAttempts":3,"delayMs":2000,"errorMessage":"529 overloaded"}`,
		agentStart, reply, said("ok"),
		`{"type":"auto_retry_end","success":true,"attempt":1}`,
		agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"user:hi",
		"notice:Pi is trying again (1/3): 529 overloaded",
		"assistant_text:ok",
		"result:true:",
		"ended:false",
	})

	// The tries run out: the run that ends them is not the turn's end, which carries the last error.
	s.Prompt("again")
	a.sent("prompt", 2)
	a.say(agentStart, echoed("again"), reply, overloaded,
		`{"type":"agent_end","messages":[],"willRetry":true}`,
		`{"type":"auto_retry_start","attempt":1,"maxAttempts":1,"delayMs":2000,"errorMessage":"529 overloaded"}`,
		agentStart, reply, overloaded, agentEnd,
		`{"type":"auto_retry_end","success":false,"attempt":1,"finalError":"529 overloaded"}`,
		settled)
	equal(t, after(sk.wait(t, "ended:true"), "user:again"), []string{
		"user:again",
		"notice:Pi is trying again (1/1): 529 overloaded",
		"result:false:529 overloaded",
		"ended:true",
	})
}

func TestAPromptSentMidTurnSteersAndShowsWhenPiReadsIt(t *testing.T) {
	s, a, sk, _ := start(t, opts{answer: func(_ *agent, kind string, cmd map[string]any) string {
		if kind == "prompt" && cmd["streamingBehavior"] != nil {
			return `{"disposition":"queued"}`
		}
		return ""
	}})
	s.Prompt("one")
	a.sent("prompt", 1)
	a.say(agentStart, echoed("one"), reply,
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"bash","args":{"command":"sleep 1"}}`)
	sk.wait(t, "tool_start:t1:shell:shell:sleep 1")
	s.Prompt("/skill:two")
	equal(t, a.sent("prompt", 2), []string{`{"message":"one"}`, `{"message":"/skill:two","streamingBehavior":"steer"}`})
	a.say(`{"type":"tool_execution_end","toolCallId":"t1","toolName":"bash","result":{"content":[]},"isError":false}`,
		echoed("<skill name=\"two\">what the skill says</skill>"), reply, said("both"), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"user:one",
		"tool_start:t1:shell:shell:sleep 1",
		"tool_end:t1:true",
		"user:/skill:two",
		"assistant_text:both",
		"result:true:",
		"ended:false",
	})
}

func TestAPromptPiRefusesFailsItsTurnAndOneItRefusesMidTurnIsANotice(t *testing.T) {
	s, a, sk, _ := start(t, opts{answer: func(_ *agent, kind string, cmd map[string]any) string {
		switch {
		case kind == "prompt" && cmd["message"] == "hi":
			return "!No API key found for deepseek"
		case kind == "prompt" && cmd["streamingBehavior"] != nil:
			return "!Cannot submit a prompt while compaction is in progress."
		}
		return ""
	}})
	s.Prompt("hi")
	equal(t, after(sk.wait(t, "ended:true"), "working"), []string{
		"working",
		"user:hi",
		"result:false:No API key found for deepseek",
		"ended:true",
	})

	s.Prompt("one")
	a.sent("prompt", 2)
	a.say(agentStart, echoed("one"))
	sk.wait(t, "user:one")
	s.Prompt("two")
	sk.wait(t, "notice:Pi didn't take “two”: Cannot submit a prompt while compaction is in progress.")
	a.say(reply, said("ok"), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:one"), []string{
		"user:one",
		"user:two",
		"notice:Pi didn't take “two”: Cannot submit a prompt while compaction is in progress.",
		"assistant_text:ok",
		"result:true:",
		"ended:false",
	})
}

func TestAPromptThatCannotBeWrittenFailsItsTurn(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	a.broken.Store(true)
	s.Prompt("hi")
	equal(t, after(sk.wait(t, "ended:true"), "working"), []string{"working", "user:hi", "result:false:broken pipe", "ended:true"})
}

func TestAPiThatNeverTellsItsStateGivesBackThePromptsThatWaited(t *testing.T) {
	s, a, sk, _ := scripted(t, opts{answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "get_state" {
			return "!No API key found for deepseek"
		}
		return ""
	}})
	s.Prompt("do the important thing")
	unsent, err := s.Open(a, state.Launch{}, false)
	if err == nil || err.Error() != "No API key found for deepseek" || !slices.Equal(unsent, []string{"do the important thing"}) {
		t.Fatalf("unsent %q, err %v", unsent, err)
	}
	if err := s.Prompt("again"); err == nil || err.Error() != "Pi didn't start: No API key found for deepseek" || len(sk.lines()) != 0 {
		t.Fatalf("err %v, said %q", err, sk.lines())
	}
}

func TestAPromptAnExtensionTookAsItsCommandEndsItsTurnWithNoRun(t *testing.T) {
	s, _, sk, _ := start(t, opts{answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "prompt" {
			return `{"disposition":"handled"}`
		}
		return ""
	}})
	s.Prompt("/reload")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{"working", "user:/reload", "result:true:", "ended:false"})
}

func TestAStopDropsWhatPiHeldAndEndsTheTurnInterrupted(t *testing.T) {
	for name, settles := range map[string]bool{"a pi that tells when it settles": true, "one that doesn't": false} {
		t.Run(name, func(t *testing.T) {
			release := make(chan struct{})
			s, a, sk, _ := start(t, opts{answer: func(a *agent, kind string, cmd map[string]any) string {
				switch {
				case kind == "prompt" && cmd["streamingBehavior"] != nil:
					return `{"disposition":"queued"}`
				// pi answers an abort once it is idle.
				case kind == "abort":
					<-release
					a.say(agentEnd)
					if settles {
						a.say(`{"type":"agent_settled","aborted":true}`)
					}
				}
				return ""
			}})
			s.Prompt("slow")
			a.sent("prompt", 1)
			a.say(agentStart, echoed("slow"))
			sk.wait(t, "user:slow")
			s.Prompt("held")
			a.sent("prompt", 2)
			s.Cancel()
			a.sent("clear_queue", 1)
			// A prompt sent while the turn stops waits for its end: pi would run it in the turn.
			s.Prompt("next")
			close(release)
			equal(t, a.sent("prompt", 3)[2:], []string{`{"message":"next"}`})
			equal(t, a.kinds(), []string{"get_state", "prompt", "prompt", "clear_queue", "abort", "prompt"})
			a.say(agentStart, echoed("next"), reply, said("ok"), agentEnd)
			if settles {
				a.say(settled)
			}
			equal(t, after(sk.wait(t, "ended:false"), "user:slow"), []string{
				"user:slow",
				"result:false:interrupted",
				"stopped",
				"working",
				"user:next",
				"assistant_text:ok",
				"result:true:",
				"ended:false",
			})
		})
	}
}

func TestARestoredSessionShowsItsHistoryWithoutWorking(t *testing.T) {
	restored := strings.Replace(fresh, `"messageCount":0`, `"messageCount":3`, 1)
	history := func(messages string) func(*agent, string, map[string]any) string {
		return func(_ *agent, kind string, _ map[string]any) string {
			if kind == "get_messages" {
				return `{"messages":[` + messages + `]}`
			}
			return ""
		}
	}
	bound := []string{"bound:01a1c8df-6ee5-73f4-9369-d38f92ac7a6a", "model:deepseek/deepseek-v4-pro"}

	// What pi 1.1.0 held after the turn its provider refused.
	_, a, sk, _ := start(t, opts{state: restored, answer: history(system + "," + asked + "," + noBalance)})
	equal(t, sk.lines(), append(slices.Clone(bound), "user:reply with the single word ok", "result:false:"+refused))
	equal(t, a.kinds(), []string{"get_state", "get_messages"})

	// The turn ended while no pocketd listened.
	_, _, sk, _ = start(t, opts{state: restored, midTurn: true, answer: history(system + "," + asked + "," + noBalance)})
	equal(t, sk.lines(), append(slices.Clone(bound), "user:reply with the single word ok", "result:false:"+refused, "ended:true"))

	_, _, sk, _ = start(t, opts{state: restored, answer: history(`
		{"role":"compactionSummary","summary":"what came before"},
		{"role":"user","content":"look"},
		{"role":"assistant","content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":"looking"},{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"ls"}}],"stopReason":"toolUse"},
		{"role":"toolResult","toolCallId":"t1","toolName":"bash","content":[{"type":"text","text":"a.txt"}],"isError":false},
		{"role":"user","content":[{"type":"text","text":"steered in"}]},
		{"role":"assistant","content":[{"type":"text","text":"found it"}],"stopReason":"stop"},
		{"role":"user","content":"more"},
		{"role":"assistant","content":[{"type":"text","text":"so"}],"stopReason":"aborted"}`)})
	equal(t, after(sk.lines(), "compacted:auto"), []string{
		"compacted:auto",
		"user:look",
		"thinking:hm", "assistant_text:looking", "tool_start:t1:shell:shell:ls", "tool_end:t1:true:a.txt",
		"user:steered in",
		"assistant_text:found it",
		"result:true:",
		"user:more",
		"assistant_text:so",
		"result:false:interrupted",
	})

	// The pocketd before this one stopped with pi in a turn: it and the call it left open end interrupted.
	_, _, sk, _ = start(t, opts{state: restored, answer: history(`
		{"role":"user","content":"look"},
		{"role":"assistant","content":[{"type":"text","text":"found it"}],"stopReason":"stop"},
		{"role":"user","content":"run them"},
		{"role":"assistant","content":[{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"ls"}},{"type":"toolCall","id":"t2","name":"bash","arguments":{"command":"make"}}],"stopReason":"toolUse"},
		{"role":"toolResult","toolCallId":"t1","toolName":"bash","content":[{"type":"text","text":"a.txt"}],"isError":false}`)})
	equal(t, after(sk.lines(), "user:look"), []string{
		"user:look",
		"assistant_text:found it",
		"result:true:",
		"user:run them",
		"tool_start:t1:shell:shell:ls", "tool_start:t2:shell:shell:make", "tool_end:t1:true:a.txt",
		"tool_end:t2:false:Interrupted",
		"result:false:interrupted",
	})
	_, _, sk, _ = start(t, opts{state: restored, answer: history(`{"role":"user","content":"look"}`)})
	equal(t, after(sk.lines(), "user:look"), []string{"user:look", "result:false:interrupted"})

	// pi is in a turn another pocketd started: it goes on, with no result yet.
	streaming := strings.Replace(restored, `"isStreaming":false`, `"isStreaming":true`, 1)
	_, a, sk, _ = start(t, opts{state: streaming, midTurn: true, answer: history(`{"role":"user","content":"look"}`)})
	equal(t, after(sk.lines(), "user:look"), []string{"user:look", "working"})
	a.say(reply, said("found it"), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{"working", "assistant_text:found it", "result:true:", "ended:false"})
}

func TestASessionTheAgentAlreadyShowsReadsNoHistory(t *testing.T) {
	restored := strings.Replace(fresh, `"messageCount":0`, `"messageCount":3`, 1)
	s, a, sk, _ := scripted(t, opts{state: restored})
	s.Shown = true
	if _, err := s.Open(a, state.Launch{}, false); err != nil {
		t.Fatal(err)
	}
	equal(t, a.kinds(), []string{"get_state"})
	equal(t, sk.lines(), []string{"bound:01a1c8df-6ee5-73f4-9369-d38f92ac7a6a", "model:deepseek/deepseek-v4-pro"})
	s.Prompt("next")
	equal(t, a.sent("prompt", 1), []string{`{"message":"next"}`})
}

func TestASessionIsSavedOnceTheFilePiNamesForItExists(t *testing.T) {
	file := filepath.Join(t.TempDir(), "session.jsonl")
	s, _, _, _ := start(t, opts{state: fmt.Sprintf(`{"sessionId":"s1","sessionFile":%q}`, file)})
	if saved, err := s.Saved(); saved || err != nil {
		t.Fatalf("saved before pi wrote its file: %v", err)
	}
	if err := os.WriteFile(file, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	if saved, err := s.Saved(); !saved || err != nil {
		t.Fatalf("not saved once pi wrote its file: %v", err)
	}
}

func TestAPiThatAnswersTooLateSaysWhatItDidntAnswer(t *testing.T) {
	was := CallWait
	CallWait = 50 * time.Millisecond
	t.Cleanup(func() { CallWait = was })
	var opened atomic.Bool
	slow := make(chan struct{})
	s, _, _, _ := start(t, opts{answer: func(_ *agent, kind string, _ map[string]any) string {
		if opened.Load() {
			<-slow
		}
		return ""
	}})
	t.Cleanup(func() { close(slow) })
	opened.Store(true)
	if err := s.Settings("", "high"); err == nil || err.Error() != "it didn't answer set_thinking_level in 50ms" {
		t.Fatalf("a switch: %v", err)
	}
	if saved, err := s.Saved(); saved || err == nil || err.Error() != "it didn't answer get_state in 50ms" {
		t.Fatalf("saved: %v, %v", saved, err)
	}
}

func TestATurnThatEndsWhileItsHistoryIsReadIsNotLeftRunning(t *testing.T) {
	streaming := strings.NewReplacer(`"messageCount":0`, `"messageCount":2`, `"isStreaming":false`, `"isStreaming":true`).Replace(fresh)
	s, a, sk, _ := start(t, opts{state: streaming, midTurn: true, answer: func(a *agent, kind string, _ map[string]any) string {
		if kind == "get_messages" {
			a.say(agentEnd, settled)
			return `{"messages":[{"role":"user","content":"look"},{"role":"assistant","content":[{"type":"text","text":"found it"}],"stopReason":"stop"}]}`
		}
		return ""
	}})
	equal(t, after(sk.lines(), "user:look"), []string{"user:look", "assistant_text:found it", "result:true:", "ended:false"})
	s.Prompt("next")
	equal(t, a.sent("prompt", 1), []string{`{"message":"next"}`})
}

func TestACompactionShowsWhatItCameToAndHoldsThePromptsSentMeanwhile(t *testing.T) {
	release := make(chan struct{})
	s, a, sk, _ := start(t, opts{answer: func(a *agent, kind string, _ map[string]any) string {
		if kind == "compact" {
			a.say(`{"type":"compaction_start","reason":"manual"}`)
			<-release
			result := `{"summary":"so far","firstKeptEntryId":"e1","tokensBefore":90000,"estimatedTokensAfter":1200,"details":{}}`
			a.say(`{"type":"compaction_end","reason":"manual","result":` + result + `,"aborted":false,"willRetry":false}`)
			return result
		}
		return ""
	}})
	if err := s.Compact(); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "compacting")
	s.Prompt("next")
	close(release)
	equal(t, a.sent("prompt", 1), []string{`{"message":"next"}`})
	equal(t, after(sk.wait(t, "working"), "compacting"), []string{
		"compacting",
		"compacted:manual",
		"tokens:1200/1000000",
		"compactingEnded",
		"working",
	})
}

func TestACompactionAskedForMidTurnRunsOnceTheTurnEndsAndAStopDropsIt(t *testing.T) {
	s, a, sk, _ := start(t, opts{answer: func(a *agent, kind string, _ map[string]any) string {
		if kind == "compact" {
			return `{"summary":"so far","estimatedTokensAfter":1200}`
		}
		return ""
	}})
	s.Prompt("one")
	a.sent("prompt", 1)
	if err := s.Compact(); err != nil {
		t.Fatal(err)
	}
	if slices.Contains(sk.lines(), "compacting") {
		t.Fatalf("compacted mid-turn: %v", sk.lines())
	}
	a.say(agentStart, echoed("one"), reply, said("ok"), agentEnd, settled)
	equal(t, after(sk.wait(t, "compactingEnded"), "result:true:"), []string{
		"result:true:", "ended:false", "compacting", "compacted:manual", "tokens:1200/1000000", "compactingEnded"})

	s.Prompt("two")
	a.sent("prompt", 2)
	s.Compact()
	s.Cancel()
	a.say(`{"type":"agent_settled","aborted":true}`)
	sk.wait(t, "stopped")
	s.Prompt("three")
	a.sent("prompt", 3)
	if got := a.sent("compact", 1); len(got) != 1 {
		t.Fatalf("compacted after the stop: %v", got)
	}
}

func TestACompactionThatFailsIsANoticeWithPisReason(t *testing.T) {
	s, _, sk, _ := start(t, opts{answer: func(a *agent, kind string, _ map[string]any) string {
		if kind == "compact" {
			// As pi 1.1.0 told it: the event says so before its reason, the response doesn't.
			a.say(`{"type":"compaction_start","reason":"manual"}`,
				`{"type":"compaction_end","reason":"manual","aborted":false,"willRetry":false,"errorMessage":"Compaction failed: Nothing to compact (session too small)"}`)
			return "!Nothing to compact (session too small)"
		}
		return ""
	}})
	s.Compact()
	equal(t, after(sk.wait(t, "compactingEnded"), "compacting"), []string{
		"compacting",
		"notice:Pi couldn't compact: Nothing to compact (session too small)",
		"compactingEnded",
	})
}

func TestAnOverflowPiCompactsAwayIsNoFailedTurn(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	// pi tells that it settles, so the end of a run is no longer the end of its turn.
	s.Prompt("hi")
	a.sent("prompt", 1)
	a.say(agentStart, echoed("hi"), reply, said("ok"), agentEnd, settled)
	sk.wait(t, "ended:false")

	s.Prompt("more")
	a.sent("prompt", 2)
	a.say(agentStart, echoed("more"), reply,
		ended("", `"stopReason":"error","errorMessage":"prompt is too long: 210000 tokens > 200000 maximum"`),
		agentEnd,
		`{"type":"compaction_start","reason":"overflow"}`)
	sk.wait(t, "compacting")
	// A prompt sent meanwhile waits: pi refuses one while it compacts.
	s.Prompt("and this")
	a.say(`{"type":"compaction_end","reason":"overflow","result":{"summary":"so far","tokensBefore":210000,"estimatedTokensAfter":900},"aborted":false,"willRetry":true}`,
		agentStart, reply, said("fits now"))
	equal(t, a.sent("prompt", 3)[2:], []string{`{"message":"and this","streamingBehavior":"steer"}`})
	a.say(echoed("and this"), reply, said("both"), agentEnd, settled)
	equal(t, after(sk.waitN(t, "ended:false", 2), "user:more"), []string{
		"user:more",
		"compacting",
		"compacted:auto",
		"tokens:900/1000000",
		"compactingEnded",
		"assistant_text:fits now",
		"user:and this",
		"assistant_text:both",
		"result:true:",
		"ended:false",
	})
}

func TestAnExtensionsDialogWaitsForTheUserAndItsNotesDont(t *testing.T) {
	s, a, sk, b := start(t, opts{})
	s.Prompt("hi")
	a.sent("prompt", 1)
	a.say(agentStart, echoed("hi"),
		`{"type":"extension_ui_request","id":"u0","method":"setStatus","statusKey":"git","statusText":"main"}`,
		`{"type":"extension_ui_request","id":"u1","method":"notify","message":"\u001b[32mlinted\u001b[0m","notifyType":"info"}`,
		`{"type":"extension_ui_request","id":"u2","method":"confirm","title":"Delete the branch?","message":"It is merged."}`)
	req := shown(t, b)
	if req.ToolName != "Delete the branch?" || string(req.Detail.Input) != `{"message":"It is merged."}` {
		t.Fatalf("asked %+v", req)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "allow"})
	equal(t, a.sent("extension_ui_response", 1), []string{`{"confirmed":true,"id":"u2"}`})

	a.say(`{"type":"extension_ui_request","id":"u3","method":"select","title":"Which one?","options":["\u001b[1mred\u001b[0m","blue"]}`)
	req = shown(t, b)
	if want := `{"questions":[{"multiSelect":false,"options":[{"description":"","label":"red"},{"description":"","label":"blue"}],"question":"Which one?"}]}`; req.ToolName != "AskUserQuestion" || string(req.Detail.Input) != want {
		t.Fatalf("asked %s %s", req.ToolName, req.Detail.Input)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "allow", Answers: map[string]string{"Which one?": "red"}})
	equal(t, a.sent("extension_ui_response", 2)[1:], []string{`{"id":"u3","value":"\u001b[1mred\u001b[0m"}`})

	a.say(`{"type":"extension_ui_request","id":"u4","method":"input","title":"Its name?"}`)
	req = shown(t, b)
	b.Resolve(req.RequestID, broker.Answer{Decision: "deny"})
	equal(t, a.sent("extension_ui_response", 3)[2:], []string{`{"cancelled":true,"id":"u4"}`})

	// A stop answers what is asked as cancelled.
	a.say(`{"type":"extension_ui_request","id":"u5","method":"confirm","title":"Push?"}`)
	shown(t, b)
	s.Cancel()
	equal(t, a.sent("extension_ui_response", 4)[3:], []string{`{"cancelled":true,"id":"u5"}`})
	equal(t, after(sk.wait(t, "stopped"), "user:hi"), []string{
		"user:hi",
		"notice:linted",
		"needsYou", "working",
		"needsYou", "working",
		"needsYou", "working",
		"needsYou",
		"result:false:interrupted",
		"stopped",
	})
}

func TestAStopBetweenTurnsCancelsAnExtensionsDialogAndSaysSoOnce(t *testing.T) {
	s, a, sk, b := start(t, opts{})
	a.say(`{"type":"extension_ui_request","id":"u0","method":"confirm","title":"Trust this folder?"}`)
	shown(t, b)
	s.Cancel()
	equal(t, a.sent("extension_ui_response", 1), []string{`{"cancelled":true,"id":"u0"}`})
	time.Sleep(50 * time.Millisecond)
	equal(t, after(sk.lines(), "needsYou"), []string{"needsYou", "stopped"})
}

func TestSwitchesNameTheModelAsPiDoesAndSayWhatPiRefused(t *testing.T) {
	s, a, _, _ := start(t, opts{launch: state.Launch{Access: "full", Effort: "high"}, answer: func(_ *agent, kind string, cmd map[string]any) string {
		if kind == "set_model" && cmd["modelId"] == "nope" {
			return "!Model not found: deepseek/nope"
		}
		return ""
	}})
	// The effort the agent was launched on is set where pi started on another.
	equal(t, a.sent("set_thinking_level", 1), []string{`{"level":"high"}`})
	if err := s.Settings("openrouter/anthropic/claude-sonnet-5", "max"); err != nil {
		t.Fatal(err)
	}
	equal(t, a.sent("set_model", 1), []string{`{"modelId":"anthropic/claude-sonnet-5","provider":"openrouter"}`})
	equal(t, a.sent("set_thinking_level", 2)[1:], []string{`{"level":"max"}`})
	if err := s.Settings("deepseek/nope", ""); err == nil || err.Error() != "Model not found: deepseek/nope" {
		t.Fatalf("a model pi doesn't have: %v", err)
	}
	if err := s.Settings("sonnet", ""); err == nil || err.Error() != "Pi names a model as provider/id" {
		t.Fatalf("a model with no provider: %v", err)
	}
}

func TestOmpStartsFastWhereTheLaunchAsksAndSaysWhenTheModelHasNoFast(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "omp", launch: state.Launch{Access: "full", Options: map[string]string{"fast": "true"}}})
	equal(t, a.sent("set_fast_mode", 1), []string{`{"enabled":true}`})
	sk.wait(t, "options:map[fast:true]")

	_, _, sk, _ = start(t, opts{provider: "omp", launch: state.Launch{Access: "full", Options: map[string]string{"fast": "true"}}, answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "set_fast_mode" {
			return "!Fast mode is unavailable for the current model."
		}
		return ""
	}})
	sk.wait(t, "notice:omp couldn't set fast to true: Fast mode is unavailable for the current model.")
	sk.wait(t, "options:map[]")

	// pi has no Fast to set.
	_, a, _, _ = start(t, opts{launch: state.Launch{Access: "full", Options: map[string]string{"fast": "true"}}})
	equal(t, a.kinds(), []string{"get_state"})
}

func TestOmpSwitchesFastMidChatAndPiHasNoSettingToSwitch(t *testing.T) {
	s, a, _, _ := start(t, opts{provider: "omp", launch: state.Launch{Access: "full"}})
	if err := s.SetOptions(map[string]string{"fast": "true"}); err != nil {
		t.Fatal(err)
	}
	if err := s.SetOptions(map[string]string{"fast": "false"}); err != nil {
		t.Fatal(err)
	}
	equal(t, a.sent("set_fast_mode", 2), []string{`{"enabled":true}`, `{"enabled":false}`})
	if err := s.SetOptions(map[string]string{"context": "1m"}); err == nil || err.Error() != "omp has no setting context" {
		t.Fatalf("a setting omp lacks: %v", err)
	}

	s, a, _, _ = start(t, opts{launch: state.Launch{Access: "full"}})
	if err := s.SetOptions(map[string]string{"fast": "true"}); err == nil || err.Error() != "Pi has no setting fast" {
		t.Fatalf("pi's fast: %v", err)
	}
	equal(t, a.kinds(), []string{"get_state"})
}

func TestOmpsFastIsOnOrOffAndASwitchWithASettingItLacksSetsNone(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp", launch: state.Launch{Access: "full"}})
	if err := s.SetOptions(map[string]string{"fast": "yes"}); err == nil || err.Error() != "fast is on or off" {
		t.Fatalf("fast set to neither: %v", err)
	}
	if err := s.SetOptions(map[string]string{"fast": "true", "zzz": "1"}); err == nil || err.Error() != "omp has no setting zzz" {
		t.Fatalf("a setting omp lacks: %v", err)
	}
	equal(t, a.kinds(), []string{"get_state"})
	equal(t, sk.lines(), []string{"bound:01a1c8df-6ee5-73f4-9369-d38f92ac7a6a", "model:deepseek/deepseek-v4-pro"})
}

func TestOmpTellsTheFastItRunsOnOnceSetAndAgainOnceAModelSwitchMovesIt(t *testing.T) {
	switched := false
	s, _, sk, _ := start(t, opts{provider: "omp", launch: state.Launch{Access: "full"}, answer: func(_ *agent, kind string, _ map[string]any) string {
		switch {
		case kind == "set_model":
			switched = true
		case kind == "get_state" && switched:
			return `{"sessionId":"s1","fastModeEnabled":false}`
		}
		return ""
	}})
	if err := s.SetOptions(map[string]string{"fast": "true"}); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "options:map[fast:true]")
	if err := s.Settings("openai/gpt-5.5", ""); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "options:map[fast:false]")
}

func TestPiListsItsSkillsAsCommands(t *testing.T) {
	s, _, _, _ := start(t, opts{answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind == "get_commands" {
			return `{"commands":[
				{"name":"llama","description":"Manage llama.cpp router models","source":"extension","sourceInfo":{"path":"builtin:llama.cpp"}},
				{"name":"fix","description":"A template","source":"prompt"},
				{"name":"skill:research","description":"Look into a question","source":"skill"},
				{"name":"skill:research","description":"Again","source":"skill"}]}`
		}
		return ""
	}})
	got, err := s.Commands()
	if want := []proto.Command{{Name: "skill:research", Description: "Look into a question"}}; err != nil || !slices.Equal(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestPiTellsItsTitleAndThinkingLevelAsTheyChange(t *testing.T) {
	_, a, sk, _ := start(t, opts{})
	a.say(`{"type":"session_info_changed","name":"Fix the build"}`, `{"type":"thinking_level_changed","level":"high"}`,
		`{"type":"extension_error","extensionPath":"/x.ts","event":"tool_call","error":"boom"}`)
	equal(t, after(sk.wait(t, "notice:boom"), "title:Fix the build"), []string{"title:Fix the build", "effort:high", "notice:boom"})
}

func TestOmpTellsOfItselfInEventsOfItsOwn(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp", answer: func(a *agent, kind string, cmd map[string]any) string {
		switch {
		case kind == "prompt" && cmd["message"] == "/model":
			a.say(`{"type":"command_output","text":"\u001b[2mgpt-6\u001b[0m"}`)
		case kind == "get_available_commands":
			return `{"commands":[{"name":"plan","description":"Plan first","source":"builtin","input":{"hint":"what"}}]}`
		}
		return ""
	}})
	got, err := s.Commands()
	if want := []proto.Command{{Name: "plan", Description: "Plan first", ArgumentHint: "what"}}; err != nil || !slices.Equal(got, want) {
		t.Fatalf("asked for: %+v, %v", got, err)
	}
	a.say(`{"type":"available_commands_update","commands":[{"name":"model","description":"Pick a model"}]}`,
		`{"type":"config_update","model":{"provider":"openai","id":"gpt-6","contextWindow":400000},"thinkingLevel":"high"}`,
		`{"type":"session_info_update","sessionId":"forked"}`, `{"type":"session_info_update","title":"Pick a model"}`)
	sk.wait(t, "title:Pick a model")
	got, _ = s.Commands()
	if want := []proto.Command{{Name: "model", Description: "Pick a model"}}; !slices.Equal(got, want) {
		t.Fatalf("as told: %+v", got)
	}

	// A command of omp's own runs no agent: omp says so, which ends its turn.
	s.Prompt("/model")
	id := sentID(a, 1)
	a.say(fmt.Sprintf(`{"type":"prompt_result","id":%q,"agentInvoked":false}`, id))
	sk.wait(t, "ended:false")

	// omp ends a run it goes on from, and shows what an extension put in the conversation.
	s.Prompt("hi")
	a.sent("prompt", 2)
	a.say(agentStart, echoed("hi"), reply, said("one"),
		`{"type":"agent_end","messages":[],"isTerminal":false}`,
		`{"type":"message_start","message":{"role":"custom","customType":"note","content":"a note","display":true}}`,
		`{"type":"message_end","message":{"role":"custom","customType":"note","content":"a note","display":true}}`,
		agentStart, reply, said("two"),
		`{"type":"agent_end","messages":[],"isTerminal":true}`)
	equal(t, after(sk.waitN(t, "ended:false", 2), "model:openai/gpt-6"), []string{
		"model:openai/gpt-6", "effort:high", "bound:forked", "title:Pick a model",
		"working", "notice:gpt-6", "user:/model", "result:true:", "ended:false",
		"working", "user:hi", "assistant_text:one", "notice:a note", "assistant_text:two", "result:true:", "ended:false",
	})
}

func TestOmpEndsTheTurnOfAPromptItAnswersAsNoAgentsOrFailsAfterItTookIt(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp", answer: func(a *agent, kind string, cmd map[string]any) string {
		if kind == "prompt" && cmd["message"] == "/reviewer careful" {
			a.say(`{"type":"command_output","text":"Selected reviewer: careful"}`)
			return `{"agentInvoked":false}`
		}
		return ""
	}})
	s.Prompt("/reviewer careful")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working", "notice:Selected reviewer: careful", "user:/reviewer careful", "result:true:", "ended:false",
	})

	s.Prompt("/workflow")
	a.say(fmt.Sprintf(`{"type":"response","id":%q,"command":"prompt","success":false,"error":"Workflow failed"}`, sentID(a, 2)))
	equal(t, after(sk.wait(t, "ended:true"), "user:/workflow"), []string{"user:/workflow", "result:false:Workflow failed", "ended:true"})
}

// ompTask is a snapshot omp's task tool tells of its sub-agent, as omp 14 does against a stub model.
func ompTask(event, field, status, async, progress string) string {
	return fmt.Sprintf(`{"type":%q,"toolCallId":"t1","toolName":"task","args":{"tasks":[{"task":"run echo sub"}],"context":"List the files"},%q:{"content":[{"type":"text","text":"Running agent Kid..."}],
		"details":{"results":[],"progress":[{"index":0,"id":"Kid","agent":"task","status":%q,"task":"Complete assignment thoroughly:\n\nrun echo sub","assignment":"run echo sub",%s}],"async":{"state":%q,"jobId":"Kid","type":"task"}}}}`,
		event, field, status, progress, async)
}

func TestAnOmpSubAgentsWorkNestsUnderItsTaskWhichEndsWithItsReport(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp"})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	call := `{"type":"toolCall","id":"t1","name":"task","arguments":{"tasks":[{"task":"run echo sub"}],"context":"List the files","i":"List the files"}}`
	a.say(agentStart, echoed("Delegate"), reply, ended(`{"type":"text","text":"Delegating."},`+call, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"task","args":{"tasks":[{"task":"run echo sub"}],"context":"List the files"},"intent":"List the files"}`,
		ompTask("tool_execution_update", "partialResult", "pending", "running", `"recentTools":[],"recentOutput":[],"toolCount":0`),
		ompTask("tool_execution_end", "result", "pending", "running", `"recentTools":[],"recentOutput":[],"toolCount":0`),
		said("Spawned it."), agentEnd, settled)
	sk.wait(t, "ended:false")
	a.say(ompTask("tool_execution_update", "partialResult", "running", "running", `"recentTools":[],"recentOutput":["Running."],"toolCount":1,"resolvedModel":"stub/stub-1","currentTool":"bash","currentToolArgs":"echo sub","currentToolArgsKey":"command"`),
		ompTask("tool_execution_update", "partialResult", "running", "running", `"recentTools":[{"tool":"bash","args":"echo sub","argsKey":"command","isError":false}],"recentOutput":["Running."],"toolCount":1,"resolvedModel":"stub/stub-1","currentTool":"read","currentToolArgs":"a.go","currentToolArgsKey":"path"`),
		ompTask("tool_execution_update", "partialResult", "completed", "completed", `"recentTools":[{"tool":"read","args":"a.go","argsKey":"path","isError":true},{"tool":"bash","args":"echo sub","argsKey":"command","isError":false}],"recentOutput":["Done: sub","All read."],"toolCount":2,"resolvedModel":"stub/stub-1"`),
		`{"type":"session_info_changed","name":"marker"}`)
	equal(t, after(sk.wait(t, "title:marker"), "user:Delegate"), []string{
		"user:Delegate",
		"assistant_text:Delegating.",
		"tool_start:t1:task:task:List the files",
		"t1>user:run echo sub",
		"tool_background:t1",
		"assistant_text:Spawned it.",
		"result:true:",
		"ended:false",
		"tool_model:t1:stub/stub-1",
		"t1>tool_start:t1/1:shell:shell:echo sub",
		"tool_end:t1/1:true",
		"t1>tool_start:t1/2:read:read:a.go",
		"tool_end:t1/2:false",
		"tool_end:t1:true:All read.\nDone: sub",
		"title:marker",
	})
}

// piAgent is what the subagent extension pi 1.1.0 ships tells of a sub-agent: its messages so far.
func piAgent(agent, task, rest string, messages ...string) string {
	return fmt.Sprintf(`{"agent":%q,"agentSource":"user","task":%q,"exitCode":0,"messages":[%s],"stderr":"","usage":{"contextTokens":120,"turns":1},"model":"stub/stub-1"%s}`,
		agent, task, strings.Join(append([]string{system}, messages...), ","), rest)
}

func piAgents(event, args, field, said, mode string, results ...string) string {
	return fmt.Sprintf(`{"type":%q,"toolCallId":"t1","toolName":"subagent","args":%s,%q:{"content":[{"type":"text","text":%q}],"details":{"mode":%q,"agentScope":"user","projectAgentsDir":null,"results":[%s]}}}`,
		event, args, field, said, mode, strings.Join(results, ","))
}

func TestAPiSubAgentsWorkNestsUnderTheCallThatRunsIt(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	args := `{"agent":"worker","task":"run echo sub"}`
	briefed := `{"role":"user","content":[{"type":"text","text":"Task: run echo sub"}]}`
	runs := `{"role":"assistant","content":[{"type":"text","text":"Running."},{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"echo sub"}}],"stopReason":"toolUse"}`
	ran := `{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{"type":"text","text":"sub\n"}],"isError":false}`
	done := `{"role":"assistant","content":[{"type":"text","text":"Done: sub"}],"stopReason":"stop"}`
	a.say(agentStart, echoed("Delegate"), reply, ended(`{"type":"toolCall","id":"t1","name":"subagent","arguments":`+args+`}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"subagent","args":`+args+`}`,
		piAgents("tool_execution_update", args, "partialResult", "(running...)", "single", piAgent("worker", "run echo sub", "", briefed)),
		piAgents("tool_execution_update", args, "partialResult", "Running.", "single", piAgent("worker", "run echo sub", "", briefed, runs)),
		piAgents("tool_execution_update", args, "partialResult", "Running.", "single", piAgent("worker", "run echo sub", "", briefed, runs, ran)),
		piAgents("tool_execution_end", "null", "result", "Done: sub", "single", piAgent("worker", "run echo sub", `,"stopReason":"stop"`, briefed, runs, ran, done)),
		said("It said sub."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:run echo sub",
		"t1>user:run echo sub",
		"tool_model:t1:stub/stub-1",
		"tool_tokens:t1:120",
		"t1>assistant_text:Running.",
		"t1>tool_start:t1/c1:shell:shell:echo sub",
		"tool_end:t1/c1:true:sub\n",
		"t1>assistant_text:Done: sub",
		"tool_end:t1:true:Done: sub",
		"assistant_text:It said sub.",
		"result:true:",
		"ended:false",
	})
}

func TestEachOfSeveralPiSubAgentsShowsAsACallOfItsOwnUnderTheOneThatRunsThem(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	args := `{"tasks":[{"agent":"worker","task":"one"},{"agent":"scout","task":"two"}]}`
	one, two := `{"role":"assistant","content":[{"type":"text","text":"First."}],"stopReason":"stop"}`, `{"role":"assistant","content":[{"type":"text","text":"Second."}],"stopReason":"error"}`
	a.say(agentStart, echoed("Delegate"), reply, ended(`{"type":"toolCall","id":"t1","name":"subagent","arguments":`+args+`}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"subagent","args":`+args+`}`,
		piAgents("tool_execution_update", args, "partialResult", "Parallel: 0/2 done, 2 running...", "parallel", piAgent("worker", "one", ""), piAgent("scout", "two", "")),
		piAgents("tool_execution_update", args, "partialResult", "Parallel: 1/2 done, 1 running...", "parallel", piAgent("worker", "one", "", one), piAgent("scout", "two", "")),
		piAgents("tool_execution_end", "null", "result", "Parallel: 1/2 succeeded", "parallel", piAgent("worker", "one", `,"stopReason":"stop"`, one), piAgent("scout", "two", `,"stopReason":"error"`, two)),
		said("One failed."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:",
		"t1>tool_start:t1/0:task:task:one",
		"t1/0>user:one",
		"tool_model:t1/0:stub/stub-1",
		"tool_tokens:t1/0:120",
		"t1>tool_start:t1/1:task:task:two",
		"t1/1>user:two",
		"tool_model:t1/1:stub/stub-1",
		"tool_tokens:t1/1:120",
		"t1/0>assistant_text:First.",
		"tool_end:t1/0:true:First.",
		"t1/1>assistant_text:Second.",
		"tool_end:t1/1:false:Second.",
		"tool_end:t1:true:Parallel: 1/2 succeeded",
		"assistant_text:One failed.",
		"result:true:",
		"ended:false",
	})
}

// As pi 1.1.0 holds a turn whose subagent call ran.
func TestARestoredPiSessionShowsItsSubAgentsWorkUnderTheCallThatRanIt(t *testing.T) {
	restored := strings.Replace(fresh, `"messageCount":0`, `"messageCount":4`, 1)
	runs := `{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"echo sub"}}],"stopReason":"toolUse"}`
	ran := `{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{"type":"text","text":"sub\n"}],"isError":false}`
	done := `{"role":"assistant","content":[{"type":"text","text":"Done: sub"}],"stopReason":"stop"}`
	_, _, sk, _ := start(t, opts{state: restored, answer: func(_ *agent, kind string, _ map[string]any) string {
		if kind != "get_messages" {
			return ""
		}
		return `{"messages":[{"role":"user","content":"Delegate"},
			{"role":"assistant","content":[{"type":"toolCall","id":"t1","name":"subagent","arguments":{"agent":"worker","task":"run echo sub"}}],"stopReason":"toolUse"},
			{"role":"toolResult","toolCallId":"t1","toolName":"subagent","content":[{"type":"text","text":"Done: sub"}],"isError":false,
				"details":{"mode":"single","agentScope":"user","projectAgentsDir":null,"results":[` + piAgent("worker", "run echo sub", `,"stopReason":"stop"`, runs, ran, done) + `]}},
			{"role":"assistant","content":[{"type":"text","text":"It said sub."}],"stopReason":"stop"}]}`
	}})
	equal(t, after(sk.lines(), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:run echo sub",
		"t1>user:run echo sub",
		"tool_model:t1:stub/stub-1",
		"t1>tool_start:t1/c1:shell:shell:echo sub",
		"tool_end:t1/c1:true:sub\n",
		"t1>assistant_text:Done: sub",
		"tool_tokens:t1:120",
		"tool_end:t1:true:Done: sub",
		"assistant_text:It said sub.",
		"result:true:",
	})
}

// The extension throws as it is stopped, so the call's end tells nothing of its sub-agents.
func TestStoppingAPiSubagentCallEndsItsSubAgentsAndTheToolsTheyLeftRunning(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	args := `{"tasks":[{"agent":"worker","task":"one"},{"agent":"scout","task":"two"}]}`
	runs := `{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"sleep 9"}}],"stopReason":"toolUse"}`
	a.say(agentStart, echoed("Delegate"), reply, ended(`{"type":"toolCall","id":"t1","name":"subagent","arguments":`+args+`}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"subagent","args":`+args+`}`,
		piAgents("tool_execution_update", args, "partialResult", "Parallel: 0/2 done, 2 running...", "parallel", piAgent("worker", "one", "", runs), piAgent("scout", "two", "")),
		`{"type":"tool_execution_end","toolCallId":"t1","toolName":"subagent","result":{"content":[{"type":"text","text":"Subagent was aborted"}],"details":{}},"isError":true}`,
		said("Stopped."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "tool_tokens:t1/1:120"), []string{
		"tool_tokens:t1/1:120",
		"tool_end:t1/0/c1:false",
		"tool_end:t1/0:false",
		"tool_end:t1/1:false",
		"tool_end:t1:false:Subagent was aborted",
		"assistant_text:Stopped.",
		"result:true:",
		"ended:false",
	})
}

func TestAToolWhoseDetailsHoldResultsRunsNoSubAgents(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("Search")
	a.sent("prompt", 1)
	args := `{"query":"go"}`
	a.say(agentStart, echoed("Search"), reply, ended(`{"type":"toolCall","id":"w1","name":"web_search","arguments":`+args+`}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"w1","toolName":"web_search","args":`+args+`}`,
		`{"type":"tool_execution_end","toolCallId":"w1","toolName":"web_search","result":{"content":[{"type":"text","text":"2 found"}],"details":{"results":[{"title":"Go","url":"https://go.dev"},{"title":"Pkg","url":"https://pkg.go.dev"}]}},"isError":false}`,
		said("Found."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Search"), []string{
		"user:Search",
		"tool_start:w1:search:search:go",
		"tool_end:w1:true:2 found",
		"assistant_text:Found.",
		"result:true:",
		"ended:false",
	})
}

// A chain runs its sub-agents one after another, each told the one before's report.
func TestEachStepOfAPiChainEndsAsTheNextOneStarts(t *testing.T) {
	s, a, sk, _ := start(t, opts{})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	args := `{"chain":[{"agent":"scout","task":"look"},{"agent":"worker","task":"fix {previous}"}]}`
	looked, fixed := `{"role":"assistant","content":[{"type":"text","text":"a.go"}],"stopReason":"stop"}`, `{"role":"assistant","content":[{"type":"text","text":"Fixed."}],"stopReason":"stop"}`
	a.say(agentStart, echoed("Delegate"), reply, ended(`{"type":"toolCall","id":"t1","name":"subagent","arguments":`+args+`}`, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"subagent","args":`+args+`}`,
		piAgents("tool_execution_update", args, "partialResult", "(running...)", "chain", piAgent("scout", "look", "")),
		piAgents("tool_execution_update", args, "partialResult", "(running...)", "chain", piAgent("scout", "look", `,"stopReason":"stop"`, looked), piAgent("worker", "fix a.go", "")),
		piAgents("tool_execution_end", "null", "result", "Fixed.", "chain", piAgent("scout", "look", `,"stopReason":"stop"`, looked), piAgent("worker", "fix a.go", `,"stopReason":"stop"`, fixed)),
		said("Done."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:",
		"t1>tool_start:t1/0:task:task:look",
		"t1/0>user:look",
		"tool_model:t1/0:stub/stub-1",
		"tool_tokens:t1/0:120",
		"t1/0>assistant_text:a.go",
		"tool_end:t1/0:true:a.go",
		"t1>tool_start:t1/1:task:task:fix a.go",
		"t1/1>user:fix a.go",
		"tool_model:t1/1:stub/stub-1",
		"tool_tokens:t1/1:120",
		"t1/1>assistant_text:Fixed.",
		"tool_end:t1/1:true:Fixed.",
		"tool_end:t1:true:Fixed.",
		"assistant_text:Done.",
		"result:true:",
		"ended:false",
	})
}

// ompTasks is what omp's task tool tells of the sub-agents of a call it waits for.
func ompTasks(event, field, args, results string, progress ...string) string {
	return fmt.Sprintf(`{"type":%q,"toolCallId":"t1","toolName":"task","args":%s,%q:{"content":[{"type":"text","text":"2 agents"}],"details":{"results":[%s],"progress":[%s]}}}`,
		event, args, field, results, strings.Join(progress, ","))
}

func ompProgress(id, status, assignment, rest string) string {
	return fmt.Sprintf(`{"id":%q,"status":%q,"assignment":%q%s}`, id, status, assignment, rest)
}

func TestEachOfSeveralOmpSubAgentsTheCallWaitsForShowsAsACallOfItsOwn(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp"})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	call := `{"type":"toolCall","id":"t1","name":"task","arguments":{"tasks":[{"task":"one"},{"task":"two"}]}}`
	a.say(agentStart, echoed("Delegate"), reply, ended(call, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"task","args":{"tasks":[{"task":"one"},{"task":"two"}]}}`,
		ompTasks("tool_execution_update", "partialResult", `{"tasks":[{"task":"one"},{"task":"two"}]}`, "", ompProgress("A", "running", "one", `,"currentTool":"bash","currentToolArgs":"ls","currentToolArgsKey":"command","toolCount":1`), ompProgress("B", "running", "two", "")),
		ompTasks("tool_execution_update", "partialResult", `{"tasks":[{"task":"one"},{"task":"two"}]}`, "", ompProgress("A", "completed", "one", `,"recentTools":[{"tool":"bash","args":"ls","argsKey":"command"}],"toolCount":1`), ompProgress("B", "aborted", "two", "")),
		ompTasks("tool_execution_end", "result", `{"tasks":[{"task":"one"},{"task":"two"}]}`, `{"id":"A","output":"Listed."}`, ompProgress("A", "completed", "one", ""), ompProgress("B", "aborted", "two", "")),
		said("One stopped."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:",
		"t1>tool_start:t1/0:task:task:A",
		"t1/0>user:one",
		"t1/0>tool_start:t1/0/1:shell:shell:ls",
		"t1>tool_start:t1/1:task:task:B",
		"t1/1>user:two",
		"tool_end:t1/0/1:true",
		"tool_end:t1/0:true",
		"tool_end:t1/1:false",
		"tool_end:t1:true:2 agents",
		"assistant_text:One stopped.",
		"result:true:",
		"ended:false",
	})
}

func TestStoppingOmpWhileItsTaskRunsEndsTheSubAgentAndTheToolItRan(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "omp"})
	s.Prompt("Delegate")
	a.sent("prompt", 1)
	call := `{"type":"toolCall","id":"t1","name":"task","arguments":{"tasks":[{"task":"one"}]}}`
	a.say(agentStart, echoed("Delegate"), reply, ended(call, `"stopReason":"toolUse"`),
		`{"type":"tool_execution_start","toolCallId":"t1","toolName":"task","args":{"tasks":[{"task":"one"}]}}`,
		ompTasks("tool_execution_update", "partialResult", `{"tasks":[{"task":"one"}]}`, "", ompProgress("A", "running", "one", `,"currentTool":"bash","currentToolArgs":"sleep 9","currentToolArgsKey":"command","toolCount":1`)),
		`{"type":"tool_execution_end","toolCallId":"t1","toolName":"task","result":{"content":[{"type":"text","text":"Aborted"}],"details":{}},"isError":true}`,
		said("Stopped."), agentEnd, settled)
	equal(t, after(sk.wait(t, "ended:false"), "user:Delegate"), []string{
		"user:Delegate",
		"tool_start:t1:task:task:",
		"t1>user:one",
		"t1>tool_start:t1/1:shell:shell:sleep 9",
		"tool_end:t1/1:false",
		"tool_end:t1:false:Aborted",
		"assistant_text:Stopped.",
		"result:true:",
		"ended:false",
	})
}

// sentID is the id of the nth prompt the client wrote, once the agent has answered it.
func sentID(a *agent, n int) string {
	a.t.Helper()
	a.sent("prompt", n)
	a.mu.Lock()
	defer a.mu.Unlock()
	for i, w := range a.wrote {
		if strings.HasPrefix(w, "prompt ") {
			if n--; n == 0 {
				return a.ids[i]
			}
		}
	}
	return ""
}

// script is a wire that plays records to a client and keeps none.
type script struct {
	records chan []byte
	closed  chan struct{}
}

func (s *script) Read(context.Context) ([]byte, error) {
	select {
	case r := <-s.records:
		return r, nil
	case <-s.closed:
		return nil, errors.New("closed")
	}
}
func (s *script) Write(context.Context, []byte) error { return nil }
func (s *script) Close()                              { close(s.closed) }

type events chan string

func (e events) Event(kind string, _ json.RawMessage) { e <- kind }

func TestAResponseToNoCommandOfThisClientIsDropped(t *testing.T) {
	w := &script{records: make(chan []byte, 4), closed: make(chan struct{})}
	got := make(events, 4)
	c := Connect(w, got)
	defer c.Close()
	// What pi answers a pocketd before this one, on the pipe it left.
	w.records <- []byte(`{"type":"response","id":"aw1-1","command":"prompt","success":true,"data":{"disposition":"started"}}`)
	w.records <- []byte(`not json`)
	w.records <- []byte(`{"type":"extension_ui_request","id":7,"method":"notify"}`)
	select {
	case kind := <-got:
		if kind != "extension_ui_request" {
			t.Fatalf("got %s", kind)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no event")
	}
}
