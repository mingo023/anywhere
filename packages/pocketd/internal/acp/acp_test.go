package acp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"slices"
	"strings"
	"sync"
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

func (s *sink) Apply(e timeline.Event) {
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
	}
	s.add("%s", line)
}

func (s *sink) SetTitle(title string)        { s.add("title:%s", title) }
func (s *sink) SetTokens(used, window int64) { s.add("tokens:%d/%d", used, window) }
func (s *sink) SetModel(model string)        { s.add("model:%s", model) }
func (s *sink) Bound(session string)         { s.add("bound:%s", session) }
func (s *sink) Loaded()                      { s.add("loaded") }
func (s *sink) Working()                     { s.add("working") }
func (s *sink) NeedsYou()                    { s.add("needsYou") }
func (s *sink) TurnEnded(failed bool)        { s.add("ended:%v", failed) }
func (s *sink) Stopped()                     { s.add("stopped") }

func (s *sink) lines() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.said)
}

// wait returns what the sink wrote down once it holds line.
func (s *sink) wait(t *testing.T, line string) []string {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		if got := s.lines(); slices.Contains(got, line) {
			return got
		}
	}
	t.Fatalf("no %q in:\n%s", line, strings.Join(s.lines(), "\n"))
	return nil
}

// agent is the other end of a Session's wire, scripted by a test: answer
// gives each call its result, or "!" and the error it is refused with, after
// saying the lines the agent writes first.
type agent struct {
	t      *testing.T
	in     chan []byte // to the client
	out    chan []byte // from the client
	closed chan struct{}
	once   sync.Once

	mu      sync.Mutex
	calls   []string          // each call and notification, as "method params"
	replies map[string]string // the client's answers to the agent's requests, by id
	answer  func(a *agent, method string, params json.RawMessage) string
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
	select {
	case a.out <- msg:
	case <-a.closed:
	}
	return nil
}

func (a *agent) Close() { a.once.Do(func() { close(a.closed) }) }

// say writes a line to the client, as the agent would.
func (a *agent) say(line string) {
	select {
	case a.in <- []byte(line):
	case <-a.closed:
	}
}

func (a *agent) update(update string) {
	a.say(`{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":` + update + `}}`)
}

func (a *agent) serve() {
	for {
		var raw []byte
		select {
		case raw = <-a.out:
		case <-a.closed:
			return
		}
		var f frame
		json.Unmarshal(raw, &f)
		if f.Version != "2.0" {
			a.t.Errorf("no jsonrpc version in %s", raw)
		}
		a.mu.Lock()
		if f.Method == "" {
			a.replies[string(f.ID)] = string(f.Result)
			if f.Error != nil {
				a.replies[string(f.ID)] = fmt.Sprintf("!%d %s", f.Error.Code, f.Error.Message)
			}
			a.mu.Unlock()
			continue
		}
		a.calls = append(a.calls, f.Method+" "+string(f.Params))
		a.mu.Unlock()
		if f.ID == nil {
			a.answer(a, f.Method, f.Params)
			continue
		}
		// Off the loop, as a turn's answer waits while the client goes on calling.
		go func() {
			result := a.answer(a, f.Method, f.Params)
			if refusal, refused := strings.CutPrefix(result, "!"); refused {
				a.say(fmt.Sprintf(`{"jsonrpc":"2.0","id":%s,"error":%s}`, f.ID, refusal))
			} else {
				a.say(fmt.Sprintf(`{"jsonrpc":"2.0","id":%s,"result":%s}`, f.ID, result))
			}
		}()
	}
}

// called returns the calls the client made of method, once there are n.
func (a *agent) called(method string, n int) []string {
	a.t.Helper()
	var got []string
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		a.mu.Lock()
		got = nil
		for _, c := range a.calls {
			if m, params, _ := strings.Cut(c, " "); m == method {
				got = append(got, params)
			}
		}
		a.mu.Unlock()
		if len(got) >= n {
			return got
		}
	}
	a.t.Fatalf("%d of %d %s", len(got), n, method)
	return nil
}

// replied returns the client's answer to the agent's request id.
func (a *agent) replied(id string) string {
	a.t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		a.mu.Lock()
		r, ok := a.replies[id]
		a.mu.Unlock()
		if ok {
			return r
		}
	}
	a.t.Fatalf("no answer to %s", id)
	return ""
}

// ask asks the client for method, as request id.
func (a *agent) ask(id, method, params string) {
	a.say(fmt.Sprintf(`{"jsonrpc":"2.0","id":%s,"method":%q,"params":%s}`, id, method, params))
}

func promptOf(params json.RawMessage) string {
	var p struct {
		Prompt []struct{ Text string }
	}
	json.Unmarshal(params, &p)
	if len(p.Prompt) == 0 {
		return ""
	}
	return p.Prompt[0].Text
}

const hermesNew = `{"models":{"availableModels":[{"modelId":"moa:default","name":"Mixture of Agents · default"}],"currentModelId":"openai-codex:gpt-6-sol"},
	"modes":{"availableModes":[{"id":"default","name":"Default"},{"id":"accept_edits","name":"Accept Edits"},{"id":"dont_ask","name":"Don't Ask"}],"currentModeId":"default"},"sessionId":"s1"}`

const opencodeNew = `{"sessionId":"s1","configOptions":[
	{"id":"model","name":"Model","category":"model","type":"select","currentValue":"opencode/big-pickle","options":[{"value":"opencode/big-pickle","name":"OpenCode/Big Pickle"}]},
	{"id":"mode","name":"Mode","category":"mode","type":"select","currentValue":"build","options":[{"value":"build","name":"build"},{"value":"plan","name":"plan"}]}]}`

// script answers session/new with created and the rest with {}, but for
// what turn answers of a prompt's text; "" there is {} too.
func script(created string, turn func(a *agent, text string) string) func(*agent, string, json.RawMessage) string {
	return func(a *agent, method string, params json.RawMessage) string {
		switch method {
		case "session/new":
			return created
		case "session/prompt":
			if r := turn(a, promptOf(params)); r != "" {
				return r
			}
		}
		return `{}`
	}
}

const endTurn = `{"stopReason":"end_turn"}`

type opts struct {
	provider string
	saved    string
	launch   state.Launch
	left     Left
	answer   func(*agent, string, json.RawMessage) string
}

// start opens a Session against a scripted agent.
func start(t *testing.T, o opts) (*Session, *agent, *sink, *broker.Broker) {
	t.Helper()
	a := &agent{t: t, in: make(chan []byte, 64), out: make(chan []byte, 64), closed: make(chan struct{}),
		replies: map[string]string{}, answer: o.answer}
	go a.serve()
	sk := &sink{}
	b := broker.New(hub.New())
	if o.launch.Access == "" {
		o.launch.Access = "ask"
	}
	s := NewSession("a1", o.provider, sk, b)
	t.Cleanup(s.Close)
	t.Cleanup(a.Close)
	if _, err := s.Open(a, "/work", o.saved, o.launch, o.left); err != nil {
		t.Fatal(err)
	}
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

// asked is the one request the broker holds open, once it does.
func asked(t *testing.T, b *broker.Broker) proto.PermissionRequest {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
		if open := b.Open(); len(open) == 1 {
			return open[0]
		}
	}
	t.Fatal("nothing asked")
	return proto.PermissionRequest{}
}
