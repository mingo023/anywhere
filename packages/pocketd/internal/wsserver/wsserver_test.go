package wsserver

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type fakeDriver struct{ prompts []string }

func (d *fakeDriver) Prompt(text string) error { d.prompts = append(d.prompts, text); return nil }
func (d *fakeDriver) Interrupt() error         { return nil }
func (d *fakeDriver) Compact() error           { return nil }
func (d *fakeDriver) Close()                   {}

type phone struct {
	t  *testing.T
	ws *websocket.Conn
}

func setup(t *testing.T, opts ...func(*Server)) (*agent.Registry, *fakeDriver, *phone) {
	h := hub.New()
	reg := agent.NewRegistry(h)
	d := &fakeDriver{}
	reg.Add("a1", "/w", "claude", d)
	s := &Server{Token: "tok", Hostname: "mac", Agents: reg, Broker: broker.New(h), Hub: h}
	for _, opt := range opts {
		opt(s)
	}
	srv := httptest.NewServer(s)
	t.Cleanup(srv.Close)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	t.Cleanup(cancel)
	ws, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http"), nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ws.CloseNow() })
	return reg, d, &phone{t, ws}
}

func (p *phone) send(raw string) {
	p.t.Helper()
	if err := p.ws.Write(context.Background(), websocket.MessageText, []byte(raw)); err != nil {
		p.t.Fatal(err)
	}
}

func (p *phone) recv() map[string]any {
	p.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, raw, err := p.ws.Read(ctx)
	if err != nil {
		p.t.Fatal(err)
	}
	var m map[string]any
	json.Unmarshal(raw, &m)
	return m
}

func (p *phone) hello() {
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":2}`)
	if m := p.recv(); m["type"] != "hello.ok" {
		p.t.Fatalf("%v", m)
	}
	if m := p.recv(); m["type"] != "agent.list" || len(m["agents"].([]any)) != 1 {
		p.t.Fatalf("%v", m)
	}
}

func (p *phone) closed() bool {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, _, err := p.ws.Read(ctx)
	return err != nil && ctx.Err() == nil
}

func TestRejectsWrongTokenAndVersion(t *testing.T) {
	for _, hello := range []string{
		`{"type":"hello","id":"h","token":"nope","clientId":"c","protocolVersion":2}`,
		`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":1}`,
	} {
		_, _, p := setup(t)
		p.send(hello)
		if m := p.recv(); m["type"] != "error" || m["message"] != "Rejected" {
			t.Fatalf("%v", m)
		}
		if !p.closed() {
			t.Fatal("rejected connection left open")
		}
	}
}

func TestSecondHelloResendsSnapshot(t *testing.T) {
	_, _, p := setup(t)
	p.hello()
	p.hello()
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.recv(); m["type"] != "agent.list" || m["id"] != "l" {
		t.Fatalf("%v", m)
	}
}

func TestClosesConnectionsThatNeverSayHello(t *testing.T) {
	_, _, p := setup(t, func(s *Server) { s.HelloTimeout = 50 * time.Millisecond })
	if !p.closed() {
		t.Fatal("unauthenticated connection left open")
	}
	_, _, p = setup(t, func(s *Server) { s.HelloTimeout = 50 * time.Millisecond })
	p.hello()
	time.Sleep(100 * time.Millisecond)
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.recv(); m["type"] != "agent.list" {
		t.Fatalf("%v", m)
	}
}

func TestHelloResendsOpenPermissionRequests(t *testing.T) {
	var b *broker.Broker
	_, _, p := setup(t, func(s *Server) { b = s.Broker })
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go b.Ask(ctx, "a1", "Bash", proto.ToolDetail{Kind: "shell", Command: "ls"}, "k")
	for len(b.Open()) == 0 {
		time.Sleep(time.Millisecond)
	}
	p.hello()
	m := p.recv()
	if req, _ := m["request"].(map[string]any); m["type"] != "permission.request" || req["requestId"] != "perm-1" {
		t.Fatalf("%v", m)
	}
}

func TestNeedsHelloFirst(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.recv(); m["message"] != "Not authenticated" || m["id"] != "l" {
		t.Fatalf("%v", m)
	}
	p.send(`garbage`)
	if m := p.recv(); m["message"] != "Malformed message" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.create","id":"c"}`)
	if m := p.recv(); m["message"] != "Malformed message" || m["id"] != "c" {
		t.Fatalf("%v", m)
	}
}

func TestPromptAcksAndReachesDriver(t *testing.T) {
	_, d, p := setup(t)
	p.hello()
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"hi"}`)
	if m := p.recv(); m["type"] != "ack" || m["id"] != "p" || len(d.prompts) != 1 {
		t.Fatalf("%v %v", m, d.prompts)
	}
	p.send(`{"type":"agent.prompt","id":"q","agentId":"zz","text":"hi"}`)
	if m := p.recv(); m["message"] != "Unknown agent: zz" || m["id"] != "q" {
		t.Fatalf("%v", m)
	}
}

func TestStreamsAndPagesTimeline(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	a, _ := reg.Get("a1")
	a.Apply(timeline.Event{Kind: "user", Text: "hi"})
	var sawStream bool
	for !sawStream {
		m := p.recv()
		sawStream = m["type"] == "agent.stream" && m["item"].(map[string]any)["text"] == "hi"
	}
	p.send(`{"type":"agent.timeline","id":"t","agentId":"a1"}`)
	for {
		m := p.recv()
		if m["type"] != "agent.timeline" {
			continue
		}
		if len(m["items"].([]any)) != 1 || m["maxSeq"] != 1.0 || m["hasOlder"] != false {
			t.Fatalf("%v", m)
		}
		return
	}
}

func TestPagesTimeline(t *testing.T) {
	reg, _, p := setup(t)
	a, _ := reg.Get("a1")
	for range 5 {
		a.Apply(timeline.Event{Kind: "user", Text: "x"})
	}
	p.hello()
	for _, c := range []struct {
		req   string
		seqs  []float64
		older bool
	}{
		{`{"type":"agent.timeline","id":"t","agentId":"a1","limit":2}`, []float64{4, 5}, true},
		{`{"type":"agent.timeline","id":"t","agentId":"a1","sinceSeq":3}`, []float64{4, 5}, true},
		{`{"type":"agent.timeline","id":"t","agentId":"a1"}`, []float64{1, 2, 3, 4, 5}, false},
	} {
		p.send(c.req)
		m := p.recv()
		for m["type"] != "agent.timeline" {
			m = p.recv()
		}
		var seqs []float64
		for _, it := range m["items"].([]any) {
			seqs = append(seqs, it.(map[string]any)["seq"].(float64))
		}
		if !reflect.DeepEqual(seqs, c.seqs) || m["hasOlder"] != c.older || m["maxSeq"] != 5.0 {
			t.Fatalf("%s: %v", c.req, m)
		}
	}
}

func TestResolvingAClosedRequestFails(t *testing.T) {
	_, _, p := setup(t)
	p.hello()
	p.send(`{"type":"permission.resolve","id":"r","requestId":"perm-9","decision":"allow"}`)
	if m := p.recv(); m["message"] != "Permission request is no longer open" || m["id"] != "r" {
		t.Fatalf("%v", m)
	}
}
