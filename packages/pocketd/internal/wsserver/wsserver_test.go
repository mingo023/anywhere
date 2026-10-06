package wsserver

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/devices"
	"pocketd/internal/events"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/names"
	"pocketd/internal/pairing"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

type fakeDriver struct{ prompts []string }

func (d *fakeDriver) Prompt(text string) error { d.prompts = append(d.prompts, text); return nil }
func (d *fakeDriver) Interrupt() error         { return nil }
func (d *fakeDriver) Compact() error           { return nil }
func (d *fakeDriver) Close()                   {}

type phone struct {
	t   *testing.T
	ws  *websocket.Conn
	url string
}

func setup(t *testing.T, opts ...func(*Server)) (*agent.Registry, *fakeDriver, *phone) {
	h := hub.New()
	reg := agent.NewRegistry(h)
	d := &fakeDriver{}
	reg.Add("a1", "/w", "claude", d)
	devs, err := devices.Open(filepath.Join(t.TempDir(), "devices.json"), "tok")
	if err != nil {
		t.Fatal(err)
	}
	s := &Server{Devices: devs, Pairing: pairing.New(time.Now), Hostname: "mac", Agents: reg, Broker: broker.New(h), Hub: h}
	for _, opt := range opts {
		opt(s)
	}
	srv := httptest.NewServer(s)
	t.Cleanup(srv.Close)
	p := &phone{t: t, url: "ws" + strings.TrimPrefix(srv.URL, "http")}
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	p.ws = ws
	return reg, d, p
}

// dial opens another socket to the same server.
func (p *phone) dial(opts *websocket.DialOptions) (*websocket.Conn, *http.Response, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	ws, resp, err := websocket.Dial(ctx, p.url, opts)
	if err == nil {
		p.t.Cleanup(func() { ws.CloseNow() })
	}
	return ws, resp, err
}

// local opens a socket the way ops hands one over: its context names who.
func local(t *testing.T, who peer.Principal, opts ...func(*Server)) (*agent.Registry, *fakeDriver, *phone) {
	var s *Server
	reg, d, _ := setup(t, append(opts, func(srv *Server) { s = srv })...)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		s.ServeHTTP(w, r.WithContext(peer.With(r.Context(), who)))
	}))
	t.Cleanup(srv.Close)
	p := &phone{t: t, url: "ws" + strings.TrimPrefix(srv.URL, "http")}
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	p.ws = ws
	return reg, d, p
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
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
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

func (p *phone) closedWith() websocket.CloseError {
	p.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	for {
		_, _, err := p.ws.Read(ctx)
		var ce websocket.CloseError
		if errors.As(err, &ce) {
			return ce
		}
		if err != nil {
			p.t.Fatalf("closed without a close frame: %v", err)
		}
	}
}

func TestAnUnknownTokenIsNotPaired(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"hello","id":"h","token":"nope","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "not_paired" || m["message"] != "Not paired" {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != 4401 || ce.Reason != "not_paired" {
		t.Fatalf("%v", ce)
	}
}

func TestAPairedPhoneGetsInUntilRevoked(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) { s = srv })
	d, token, _ := s.Devices.Add("iPhone", "ios", devices.PhoneScopes)
	p.send(`{"type":"hello","id":"h","token":"` + token + `","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["type"] != "hello.ok" {
		t.Fatalf("%v", m)
	}
	if got := s.Devices.List()[1]; got.LastAddr != "127.0.0.1" || got.LastSeenAt == 0 {
		t.Fatalf("%+v", got)
	}
	s.Devices.Revoke(d.ID)
	s.CloseDevice(d.ID, "revoked")
	if ce := p.closedWith(); ce.Code != 4401 || ce.Reason != "revoked" {
		t.Fatalf("%v", ce)
	}
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	p.ws = ws
	p.send(`{"type":"hello","id":"h","token":"` + token + `","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["code"] != "not_paired" {
		t.Fatalf("%v", m)
	}
}

func TestASecondHelloCantSwitchDevices(t *testing.T) {
	var s *Server
	_, _, p := setup(t, func(srv *Server) { s = srv })
	p.hello()
	_, token, _ := s.Devices.Add("iPhone", "ios", devices.PhoneScopes)
	p.send(`{"type":"hello","id":"h","token":"` + token + `","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["code"] != "not_paired" {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != 4401 || ce.Reason != "not_paired" {
		t.Fatalf("%v", ce)
	}
	if got := s.Devices.List()[1]; got.LastSeenAt != 0 {
		t.Fatalf("%+v", got)
	}
}

func TestAVersionMismatchNamesTheOlderSide(t *testing.T) {
	for _, c := range []struct{ hello, code, message string }{
		{`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":2}`, "client_too_old", "Update Pocket on this phone"},
		{`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":4,"protocol":{"min":4,"max":5}}`, "server_too_old", "Update Pocket on your Mac"},
	} {
		_, _, p := setup(t)
		p.send(c.hello)
		if m := p.recv(); m["type"] != "error" || m["code"] != c.code || m["message"] != c.message || m["id"] != "h" {
			t.Fatalf("%v", m)
		}
		if ce := p.closedWith(); ce.Code != 4426 || ce.Reason != c.code {
			t.Fatalf("%v", ce)
		}
	}
}

func TestTheDesktopsHelloStillGetsIn(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"desktop","protocolVersion":3}`)
	m := p.recv()
	if m["type"] != "hello.ok" || m["protocolVersion"] != 3.0 || !reflect.DeepEqual(m["caps"], []any{}) ||
		!reflect.DeepEqual(m["protocol"], map[string]any{"min": 3.0, "max": 3.0}) {
		t.Fatalf("%v", m)
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
	go b.Ask(ctx, proto.PermissionRequest{AgentID: "a1", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "ls"}}, "k")
	for len(b.Open()) == 0 {
		time.Sleep(time.Millisecond)
	}
	p.hello()
	m := p.recv()
	if req, _ := m["request"].(map[string]any); m["type"] != "permission.request" || req["requestId"] != b.Open()[0].RequestID {
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

func TestPinningAnAgentTellsEveryPhone(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	p.send(`{"type":"agent.pin","id":"p","agentId":"a1","pinned":true}`)
	acked, told := false, false
	for !acked || !told {
		switch m := p.recv(); m["type"] {
		case "ack":
			acked = true
		case "agent.update":
			told = m["agent"].(map[string]any)["pinned"] == true
		default:
			t.Fatalf("%v", m)
		}
	}
	if a, _ := reg.Get("a1"); !a.Summary().Pinned {
		t.Fatal("the agent is not pinned")
	}
}

type resumingDriver struct{ fakeDriver }

func (*resumingDriver) Prompt(string) error { return agent.ErrResuming }

func TestAPromptToAResumingAgentSaysAgentResuming(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	a, _ := reg.Get("a1")
	a.SetDriver(&resumingDriver{})
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"hi"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeAgentResuming || m["message"] != "This session is still resuming" || m["id"] != "p" {
		t.Fatalf("%v", m)
	}
}

func TestStreamsAndPagesTimeline(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	a, _ := reg.Get("a1")
	a.Record(timeline.Event{Kind: "user", Text: "hi"})
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
		a.Record(timeline.Event{Kind: "user", Text: "x"})
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

func TestAnAgentOnScreenEndsIdleUntilItsPhoneLeaves(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	p.send(`{"type":"agent.view","id":"v1","agentIds":["a1"]}`)
	if m := p.recv(); m["type"] != "ack" || m["id"] != "v1" {
		t.Fatalf("%v", m)
	}
	a, _ := reg.Get("a1")
	a.Working()
	a.TurnEnded(false)
	if s := a.Summary().Status; s != "idle" {
		t.Fatalf("viewed agent ended %s", s)
	}
	p.ws.Close(websocket.StatusNormalClosure, "")
	for deadline := time.Now().Add(5 * time.Second); a.Summary().Status != "done"; time.Sleep(10 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatal("view outlived its connection")
		}
		a.Working()
		a.TurnEnded(false)
	}
}

func TestSeenClearsDone(t *testing.T) {
	reg, _, p := setup(t)
	a, _ := reg.Get("a1")
	a.Working()
	a.TurnEnded(false)
	p.hello()
	p.send(`{"type":"agent.seen","id":"s1","agentIds":["a1"]}`)
	m := p.recv()
	for m["type"] == "agent.update" {
		m = p.recv()
	}
	if m["type"] != "ack" || m["id"] != "s1" || a.Summary().Status != "idle" {
		t.Fatalf("%v %+v", m, a.Summary())
	}
}

func fastPings(s *Server) { s.pingInterval, s.pingTimeout = 10*time.Millisecond, 50*time.Millisecond }

func TestAPhoneThatStopsAnsweringPingsLosesItsView(t *testing.T) {
	reg, _, p := setup(t, fastPings)
	p.hello()
	p.send(`{"type":"agent.view","id":"v1","agentIds":["a1"]}`)
	if m := p.recv(); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	a, _ := reg.Get("a1")
	for deadline := time.Now().Add(5 * time.Second); a.Summary().Status != "done"; time.Sleep(10 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatal("view outlived a silent phone")
		}
		a.Working()
		a.TurnEnded(false)
	}
}

func TestAPhoneAnsweringPingsKeepsItsView(t *testing.T) {
	reg, _, p := setup(t, fastPings)
	p.hello()
	p.send(`{"type":"agent.view","id":"v1","agentIds":["a1"]}`)
	if m := p.recv(); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	go func() {
		for {
			if _, _, err := p.ws.Read(context.Background()); err != nil {
				return
			}
		}
	}()
	time.Sleep(200 * time.Millisecond)
	a, _ := reg.Get("a1")
	a.Working()
	a.TurnEnded(false)
	if s := a.Summary().Status; s != "idle" {
		t.Fatalf("answering phone lost its view: %s", s)
	}
}

func TestOnlyPagesFromTheSameHostMayConnect(t *testing.T) {
	_, _, p := setup(t)
	host := strings.TrimPrefix(p.url, "ws://")
	if _, resp, err := p.dial(&websocket.DialOptions{HTTPHeader: http.Header{"Origin": {"https://evil.example"}}}); err == nil || resp.StatusCode != http.StatusForbidden {
		t.Fatalf("foreign origin: %v", err)
	}
	if _, _, err := p.dial(&websocket.DialOptions{HTTPHeader: http.Header{"Origin": {"http://" + host}}}); err != nil {
		t.Fatalf("same-host origin: %v", err)
	}
}

func TestABigFrameBeforeHelloClosesTheSocket(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"hello","id":"` + strings.Repeat("x", 5000) + `"}`)
	if ce := p.closedWith(); ce.Code != websocket.StatusMessageTooBig {
		t.Fatalf("%v", ce)
	}
	_, _, p = setup(t)
	p.hello()
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"` + strings.Repeat("x", 5000) + `"}`)
	if m := p.recv(); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
}

func TestSixteenSocketsMayWaitForHello(t *testing.T) {
	_, _, p := setup(t)
	for range 15 {
		if _, _, err := p.dial(nil); err != nil {
			t.Fatal(err)
		}
	}
	if _, resp, err := p.dial(nil); err == nil || resp.StatusCode != http.StatusServiceUnavailable {
		t.Fatalf("17th socket: %v", err)
	}
	p.hello()
	if _, _, err := p.dial(nil); err != nil {
		t.Fatalf("a hello frees a slot: %v", err)
	}
}

func TestThreeWrongTokensLockTheAddressOut(t *testing.T) {
	_, _, p := setup(t)
	for n := range 3 {
		p.send(`{"type":"hello","id":"h","token":"nope","clientId":"c","protocolVersion":3}`)
		m := p.recv()
		ce := p.closedWith()
		if n < 2 && (m["code"] != "not_paired" || ce.Code != 4401) {
			t.Fatalf("%d: %v %v", n, m, ce)
		}
		if n == 2 && (m["code"] != "rate_limited" || m["message"] != "Too many attempts" || ce.Code != websocket.StatusPolicyViolation || ce.Reason != "rate_limited") {
			t.Fatalf("%d: %v %v", n, m, ce)
		}
		if n < 2 {
			ws, _, err := p.dial(nil)
			if err != nil {
				t.Fatal(err)
			}
			p.ws = ws
		}
	}
	if _, resp, err := p.dial(nil); err == nil || resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("4th attempt: %v", err)
	}
}

func TestHelloOverTheSocketNeedsNoToken(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1))
	p.send(`{"type":"hello","id":"h","clientId":"desktop","protocolVersion":3,"caps":["scopes.v1"]}`)
	m := p.recv()
	if want := []any{"observe", "drive", "approve", "spawn", "owner"}; m["type"] != "hello.ok" || !reflect.DeepEqual(m["scopes"], want) {
		t.Fatalf("%v", m)
	}
	if m := p.recv(); m["type"] != "agent.list" {
		t.Fatalf("%v", m)
	}
}

func TestHelloOverTcpWithoutATokenIsNotPaired(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"hello","id":"h","clientId":"desktop","protocolVersion":3,"caps":["scopes.v1"]}`)
	if m := p.recv(); m["code"] != "not_paired" {
		t.Fatalf("%v", m)
	}
	if ce := p.closedWith(); ce.Code != 4401 {
		t.Fatalf("%v", ce)
	}
}

func TestScopesAreSentOnlyUnderTheirCap(t *testing.T) {
	_, _, p := setup(t)
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["type"] != "hello.ok" || m["scopes"] != nil {
		t.Fatalf("%v", m)
	}
	p.recv()
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["scopes.v1"]}`)
	if m := p.recv(); !reflect.DeepEqual(m["scopes"], []any{"observe", "drive", "approve"}) {
		t.Fatalf("%v", m)
	}
}

func TestBadPairCodesFromAPtyPeerDontLockTheOwnerOut(t *testing.T) {
	var s *Server
	setup(t, func(srv *Server) { s = srv })
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		who := peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}
		if r.URL.Path == "/owner" {
			who = peer.OwnerOf(1)
		}
		s.ServeHTTP(w, r.WithContext(peer.With(r.Context(), who)))
	}))
	t.Cleanup(srv.Close)
	url := "ws" + strings.TrimPrefix(srv.URL, "http")
	pty := &phone{t: t, url: url}
	ws, _, err := pty.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	pty.ws = ws
	for range 3 {
		pty.send(pairMsg("wrong"))
		if m := pty.recv(); m["code"] != "pair_invalid" {
			t.Fatalf("%v", m)
		}
	}
	owner := &phone{t: t, url: url + "/owner"}
	if owner.ws, _, err = owner.dial(nil); err != nil {
		t.Fatalf("owner locked out: %v", err)
	}
	owner.send(`{"type":"hello","id":"h","clientId":"desktop","protocolVersion":3,"caps":["scopes.v1"]}`)
	if m := owner.recv(); m["type"] != "hello.ok" || !reflect.DeepEqual(m["scopes"], []any{"observe", "drive", "approve", "spawn", "owner"}) {
		t.Fatalf("%v", m)
	}
}

func helloAs(p *phone) {
	p.send(`{"type":"hello","id":"h","clientId":"c","protocolVersion":3}`)
	p.recv()
	p.recv()
}

func TestResolveWithoutApproveIsRefused(t *testing.T) {
	_, _, p := local(t, peer.Principal{Kind: peer.Device, Scopes: []peer.Scope{peer.Observe, peer.Drive}})
	helloAs(p)
	p.send(`{"type":"permission.resolve","id":"r","requestId":"x","decision":"allow"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "scope_denied" ||
		m["message"] != "permission.resolve needs approve; run it outside Pocket Terminals, or against a scratch pocketd (POCKETD_SOCK)" {
		t.Fatalf("%v", m)
	}
}

func TestAPtyPeerCanReadAgentsButNotDriveThem(t *testing.T) {
	_, d, p := local(t, peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes})
	helloAs(p)
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.recv(); m["type"] != "agent.list" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"1"}`)
	if m := p.recv(); m["code"] != "scope_denied" {
		t.Fatalf("%v", m)
	}
	if len(d.prompts) != 0 {
		t.Fatalf("typed %v", d.prompts)
	}
}

func TestAPtyPeerPromptingAnAgentWithAnOpenAskIsToldSo(t *testing.T) {
	asked := make(chan string, 1)
	reg, d, p := local(t, peer.Principal{Kind: peer.PTY, Terminal: "t2", Scopes: peer.PTYScopes}, func(s *Server) {
		s.AskOpen = func(id string) bool { asked <- id; return true }
	})
	a, _ := reg.Get("a1")
	a.SetTerminal("t1")
	helloAs(p)
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"1"}`)
	if m := p.recv(); m["code"] != "ask_open" || m["message"] != "Terminal t1 is waiting on an ask; answer it from the desktop or phone" {
		t.Fatalf("%v", m)
	}
	if id := <-asked; id != "t1" || len(d.prompts) != 0 {
		t.Fatalf("typed %v, asked %q", d.prompts, id)
	}
}

func TestAnOversizedPromptIsRefusedWithItsCode(t *testing.T) {
	if e := errorReply("p", terminal.ErrPromptTooLarge); e.Code != "prompt_too_large" || e.ID != "p" {
		t.Fatalf("%+v", e)
	}
	if e := errorReply("p", errors.New("Unknown agent: zz")); e.Code != "" {
		t.Fatalf("%+v", e)
	}
}

const helloWithHost = `{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["host.v1"]}`

func TestHelloOkCarriesHostOnlyWithTheCap(t *testing.T) {
	mon := host.NewMonitor()
	mon.Set(func(h *proto.HostState) { h.Tailnet = true })
	withMonitor := func(s *Server) { s.Monitor = mon }

	_, _, p := setup(t, withMonitor)
	p.send(helloWithHost)
	m := p.recv()
	if !reflect.DeepEqual(m["host"], map[string]any{"tailnet": true, "keepingAwake": false}) || !reflect.DeepEqual(m["caps"], []any{"host.v1"}) {
		t.Fatalf("%v", m)
	}

	_, _, p = setup(t, withMonitor)
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	if m := p.recv(); m["type"] != "hello.ok" || m["host"] != nil {
		t.Fatalf("%v", m)
	}
}

func TestHostChangedReachesOnlyCapClients(t *testing.T) {
	mon := host.NewMonitor()
	withMonitor := func(s *Server) { s.Monitor = mon }
	_, _, capable := setup(t, withMonitor)
	capable.send(helloWithHost)
	capable.recv()
	capable.recv()
	_, _, old := setup(t, withMonitor)
	old.hello()

	mon.Set(func(h *proto.HostState) { h.KeepingAwake = true })
	if m := capable.recv(); m["type"] != "host.changed" || !reflect.DeepEqual(m["host"], map[string]any{"tailnet": false, "keepingAwake": true}) {
		t.Fatalf("%v", m)
	}
	old.send(`{"type":"agent.list","id":"l"}`)
	if m := old.recv(); m["type"] != "agent.list" {
		t.Fatalf("old client got %v", m)
	}
}

func TestSeenAndAnswerAreRecorded(t *testing.T) {
	home := t.TempDir()
	log, err := events.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	var b *broker.Broker
	reg, _, p := setup(t, func(s *Server) { s.Events, b = log, s.Broker })
	p.hello()
	a, _ := reg.Get("a1")
	a.Working()
	a.TurnEnded(false)
	p.send(`{"type":"agent.seen","id":"s","agentIds":["a1"]}`)
	p.send(`{"type":"agent.seen","id":"s2","agentIds":["a1"]}`)
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go b.Ask(ctx, proto.PermissionRequest{AgentID: "a1", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "ls"}}, "k")
	for len(b.Open()) == 0 {
		time.Sleep(time.Millisecond)
	}
	p.send(`{"type":"permission.resolve","id":"r","requestId":"` + b.Open()[0].RequestID + `","decision":"allow"}`)
	for m := p.recv(); m["id"] != "r"; m = p.recv() {
	}
	raw, _ := os.ReadFile(filepath.Join(home, "events.jsonl"))
	var kinds []string
	for _, line := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		var e events.Event
		json.Unmarshal([]byte(line), &e)
		kinds = append(kinds, e.Kind+" "+e.Agent+" "+e.Principal+" "+e.Decision)
	}
	if want := []string{"seen a1 device ", "answer a1 device allow"}; !reflect.DeepEqual(kinds, want) {
		t.Fatalf("%q", kinds)
	}
}

func TestProjectListAnswersWithTheRegistry(t *testing.T) {
	projects := []proto.Project{{Path: "/w", Name: "w", Worktrees: []proto.Worktree{{Name: "w", Path: "/w", Branch: "main", IsMain: true}}}}
	_, _, p := setup(t, func(s *Server) { s.Projects = func() []proto.Project { return projects } })
	p.hello()
	p.send(`{"type":"project.list","id":"p"}`)
	m := p.recv()
	got, _ := json.Marshal(m["projects"])
	if m["type"] != "project.list" || m["id"] != "p" || string(got) != `[{"name":"w","path":"/w","worktrees":[{"branch":"main","isMain":true,"name":"w","path":"/w"}]}]` {
		t.Fatalf("%v", m)
	}
}

func TestAnEmptyRegistryListsNoProjects(t *testing.T) {
	_, _, p := setup(t, func(s *Server) { s.Projects = func() []proto.Project { return nil } })
	p.hello()
	p.send(`{"type":"project.list","id":"p"}`)
	if m := p.recv(); m["type"] != "project.list" || m["projects"] == nil || len(m["projects"].([]any)) != 0 {
		t.Fatalf("%v", m)
	}
}

const helloWithNames = `{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["names.v1"]}`

func TestANamesClientGetsEveryNameThenEachRename(t *testing.T) {
	n := names.Open(t.TempDir())
	n.Rename("/w/a", "First")
	_, _, p := local(t, peer.OwnerOf(1), func(s *Server) { s.Names = n })
	p.send(helloWithNames)
	for _, want := range []string{"hello.ok", "agent.list"} {
		if m := p.recv(); m["type"] != want {
			t.Fatalf("%v", m)
		}
	}
	if m := p.recv(); m["type"] != "worktree.names" || !reflect.DeepEqual(m["names"], map[string]any{"/w/a": "First"}) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"worktree.rename","id":"r","path":"/w/a","title":"Second"}`)
	got := map[string]map[string]any{}
	for range 2 {
		m := p.recv()
		got[m["type"].(string)] = m
	}
	if got["ack"]["id"] != "r" || !reflect.DeepEqual(got["worktree.names"]["names"], map[string]any{"/w/a": "Second"}) {
		t.Fatalf("%v", got)
	}
}

func TestAClientWithoutTheNamesCapGetsNoNames(t *testing.T) {
	n := names.Open(t.TempDir())
	_, _, old := setup(t, func(s *Server) { s.Names = n })
	old.hello()
	n.Rename("/w/a", "First")
	old.send(`{"type":"agent.list","id":"l"}`)
	if m := old.recv(); m["type"] != "agent.list" {
		t.Fatalf("old client got %v", m)
	}
}
