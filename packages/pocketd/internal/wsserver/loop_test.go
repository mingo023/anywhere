package wsserver

import (
	"bytes"
	"context"
	"encoding/json"
	"log"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/peer"
)

// gateDriver takes a prompt only once gate closes, as an agent busy with a slow step would.
type gateDriver struct {
	fakeDriver
	gate     chan struct{}
	err      error
	onPrompt func()

	mu  sync.Mutex
	got []string
}

func newGate() *gateDriver { return &gateDriver{gate: make(chan struct{})} }

func (d *gateDriver) Prompt(text string) error {
	<-d.gate
	d.mu.Lock()
	d.got = append(d.got, text)
	d.mu.Unlock()
	if d.err == nil && d.onPrompt != nil {
		d.onPrompt()
	}
	return d.err
}

func (d *gateDriver) prompts() []string {
	d.mu.Lock()
	defer d.mu.Unlock()
	return append([]string(nil), d.got...)
}

func withGate() (*gateDriver, func(*Server)) {
	d := newGate()
	return d, func(s *Server) {
		a := s.Agents.Add("a2", "/w", "claude", d)
		d.onPrompt = a.Working
	}
}

// onSocket is local() over a unix socket, as ops serves the owner's: the
// returned redial opens another conn to the same socket.
func onSocket(t *testing.T, opts ...func(*Server)) (p *phone, redial func()) {
	t.Helper()
	var s *Server
	setup(t, append(opts, func(srv *Server) { s = srv })...)
	dir, err := os.MkdirTemp("", "ws")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	sock := filepath.Join(dir, "s")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	srv := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		s.ServeHTTP(w, r.WithContext(peer.With(r.Context(), peer.OwnerOf(1))))
	}))
	srv.Listener.Close()
	srv.Listener = ln
	srv.Start()
	t.Cleanup(srv.Close)
	client := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", sock)
	}}}
	p = &phone{t: t, url: "ws://socket/"}
	redial = func() {
		ws, _, err := p.dial(&websocket.DialOptions{HTTPClient: client})
		if err != nil {
			t.Fatal(err)
		}
		p.ws = ws
	}
	redial()
	return p, redial
}

func (p *phone) helloAs(client string, caps ...string) {
	p.t.Helper()
	list := `[]`
	if len(caps) > 0 {
		list = `["` + strings.Join(caps, `","`) + `"]`
	}
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"` + client + `","protocolVersion":3,"caps":` + list + `}`)
	if m := p.recv(); m["type"] != "hello.ok" {
		p.t.Fatalf("%v", m)
	}
	if m := p.recv(); m["type"] != "agent.list" {
		p.t.Fatalf("%v", m)
	}
}

func (p *phone) msg() map[string]any {
	p.t.Helper()
	for {
		m := p.recv()
		switch m["type"] {
		case "agent.update", "agent.stream", "queue.update":
		default:
			return m
		}
	}
}

// feed reads the conn's messages on a goroutine, leaving out a feed's, so a test can tell that none arrives.
type feed struct {
	t  *testing.T
	in chan map[string]any
}

func (p *phone) feed() feed {
	f := feed{p.t, make(chan map[string]any, 64)}
	go func() {
		for {
			m := p.recvOrNil()
			if m == nil {
				close(f.in)
				return
			}
			if typ := m["type"]; typ != "agent.update" && typ != "agent.stream" {
				f.in <- m
			}
		}
	}()
	return f
}

func (p *phone) recvOrNil() map[string]any {
	_, raw, err := p.ws.Read(context.Background())
	if err != nil {
		return nil
	}
	var m map[string]any
	json.Unmarshal(raw, &m)
	return m
}

func (f feed) get() map[string]any {
	f.t.Helper()
	select {
	case m, ok := <-f.in:
		if !ok {
			f.t.Fatal("conn closed")
		}
		return m
	case <-time.After(5 * time.Second):
		f.t.Fatal("no message")
	}
	return nil
}

func (f feed) quiet(d time.Duration) {
	f.t.Helper()
	select {
	case m, ok := <-f.in:
		if !ok {
			f.t.Fatal("conn closed")
		}
		f.t.Fatalf("unexpected %v", m)
	case <-time.After(d):
	}
}

func TestASilentSocketConnIsNeverPingedSoItStaysOpen(t *testing.T) {
	p, _ := onSocket(t, fastPings)
	p.helloAs("c")
	time.Sleep(300 * time.Millisecond)
	p.send(`{"type":"agent.list","id":"l"}`)
	if m := p.msg(); m["type"] != "agent.list" || m["id"] != "l" {
		t.Fatalf("%v", m)
	}
}

func TestASilentOwnerTokenTCPConnIsStillPinged(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1), fastPings)
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	p.recv()
	p.recv()
	time.Sleep(300 * time.Millisecond)
	if !p.closed() {
		t.Fatal("a silent TCP conn stayed open")
	}
}

func TestAHandlerStalledPastThePingTimeoutDoesNotCloseASocketConn(t *testing.T) {
	gate, opt := withGate()
	p, _ := onSocket(t, fastPings, opt)
	p.helloAs("c")
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a2","text":"hi"}`)
	time.Sleep(300 * time.Millisecond)
	close(gate.gate)
	if m := p.msg(); m["type"] != "ack" || m["id"] != "p" {
		t.Fatalf("%v", m)
	}
}

func TestAHandlerStalledPastThePingTimeoutDoesNotCloseAConnThatAnswersPings(t *testing.T) {
	gate, opt := withGate()
	_, _, p := setup(t, fastPings, opt)
	p.helloAs("c")
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a2","text":"hi"}`)
	replies := make(chan map[string]any, 1)
	go func() {
		for {
			m := p.recv()
			if m["type"] == "ack" {
				replies <- m
				return
			}
		}
	}()
	time.Sleep(300 * time.Millisecond)
	close(gate.gate)
	select {
	case m := <-replies:
		if m["id"] != "p" {
			t.Fatalf("%v", m)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no ack: the conn was closed while its handler stalled")
	}
}

func TestAFullQueueStopsTheReadInsteadOfClosingTheConn(t *testing.T) {
	gate, opt := withGate()
	p, _ := onSocket(t, fastPings, opt)
	p.helloAs("c")
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a2","text":"hi"}`)
	const seen = frameQueue + 50
	for range seen {
		p.send(`{"type":"agent.seen","id":"s","agentIds":[]}`)
	}
	time.Sleep(300 * time.Millisecond)
	close(gate.gate)
	if m := p.msg(); m["type"] != "ack" || m["id"] != "p" {
		t.Fatalf("%v", m)
	}
	for n := range seen {
		if m := p.msg(); m["type"] != "ack" || m["id"] != "s" {
			t.Fatalf("reply %d: %v", n, m)
		}
	}
}

func TestAFrameSentRightAfterHelloIsReadUnderTheAuthedLimit(t *testing.T) {
	p, _ := onSocket(t)
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a1","text":"` + strings.Repeat("x", 5000) + `"}`)
	var got []string
	for len(got) < 3 {
		m := p.msg()
		got = append(got, m["type"].(string))
	}
	if got[0] != "hello.ok" || got[1] != "agent.list" || got[2] != "ack" {
		t.Fatalf("%q", got)
	}
}

func logged(t *testing.T) func() string {
	t.Helper()
	var mu sync.Mutex
	var buf bytes.Buffer
	log.SetOutput(writerFunc(func(b []byte) (int, error) {
		mu.Lock()
		defer mu.Unlock()
		return buf.Write(b)
	}))
	t.Cleanup(func() { log.SetOutput(os.Stderr) })
	return func() string {
		mu.Lock()
		defer mu.Unlock()
		return buf.String()
	}
}

type writerFunc func([]byte) (int, error)

func (f writerFunc) Write(b []byte) (int, error) { return f(b) }

func waitLog(t *testing.T, got func() string, want string) string {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(10 * time.Millisecond) {
		if s := got(); strings.Contains(s, want) {
			return s
		}
	}
	t.Fatalf("log lacks %q:\n%s", want, got())
	return ""
}

func TestAClosedConnIsLoggedWithItsClientAndHowLongItLived(t *testing.T) {
	out := logged(t)
	p, _ := onSocket(t)
	p.helloAs("phone")
	p.ws.Close(websocket.StatusNormalClosure, "bye")
	line := waitLog(t, out, `ws closed: client="phone"`)
	if !strings.Contains(line, "lived=") || !strings.Contains(line, "bye") {
		t.Fatal(line)
	}
}

func TestAHandlerRunningPastTwoSecondsIsLoggedByVerbWithoutItsText(t *testing.T) {
	out := logged(t)
	gate, opt := withGate()
	p, _ := onSocket(t, opt, func(s *Server) { s.slowHandler = 50 * time.Millisecond })
	p.helloAs("c")
	p.send(`{"type":"agent.prompt","id":"p","agentId":"a2","text":"SECRET-WORDS"}`)
	time.Sleep(100 * time.Millisecond)
	close(gate.gate)
	line := waitLog(t, out, "ws handler slow: verb=agent.prompt took=")
	if strings.Contains(line, "SECRET-WORDS") {
		t.Fatal(line)
	}
}

var _ agent.Driver = (*gateDriver)(nil)
