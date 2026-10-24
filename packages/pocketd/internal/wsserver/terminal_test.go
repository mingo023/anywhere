//go:build unix

package wsserver

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/devices"
	"pocketd/internal/locals"
	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
)

// shells opens sh where the login shell would go, so tests don't read the user's dotfiles.
type shells struct{ m *terminal.Manager }

func (s shells) Spawn(m ops.Msg) (*terminal.Terminal, error) {
	return s.m.Spawn(terminal.Spec{Cmd: "sh", Env: append(os.Environ(), "PS1=$ "), Cwd: m.Cwd, Cols: m.Cols, Rows: m.Rows, Local: m.Local})
}

// withTerminals serves terminal.v1 over terms, with project the one registered project.
func withTerminals(t *testing.T, terms *terminal.Manager, project string) func(*Server) {
	t.Cleanup(func() { terms.CloseAll(0) })
	dir := t.TempDir()
	path := filepath.Join(dir, "desktop.json")
	raw, _ := json.Marshal(map[string]any{"projects": []string{project}, "repos": map[string]any{}})
	if err := os.WriteFile(path, raw, 0o600); err != nil {
		t.Fatal(err)
	}
	return func(s *Server) {
		s.Terminals, s.TerminalHost, s.Registry, s.Locals = terms, shells{terms}, registry.New(path), locals.Open(dir)
	}
}

func run(t *testing.T, terms *terminal.Manager, script string) *terminal.Terminal {
	t.Helper()
	s, err := terms.Spawn(terminal.Spec{Cmd: "sh", Args: []string{"-c", script}, Cwd: "/", Cols: 80, Rows: 24})
	if err != nil {
		t.Fatal(err)
	}
	return s
}

// frame is one message: JSON in msg, or a binary terminal frame.
type frame struct {
	msg     map[string]any
	kind    byte
	id      string
	payload []byte
}

func (p *phone) next() frame {
	p.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	kind, raw, err := p.ws.Read(ctx)
	if err != nil {
		p.t.Fatal(err)
	}
	if kind == websocket.MessageText {
		var m map[string]any
		json.Unmarshal(raw, &m)
		return frame{msg: m}
	}
	n := int(raw[1])
	return frame{kind: raw[0], id: string(raw[2 : 2+n]), payload: raw[2+n:]}
}

// until reads until a frame contains want, and returns the output bytes read.
func (p *phone) until(want string) []byte {
	p.t.Helper()
	var out []byte
	for !bytes.Contains(out, []byte(want)) {
		f := p.next()
		if f.msg != nil {
			continue
		}
		out = append(out, f.payload...)
	}
	return out
}

func (p *phone) input(id, text string) {
	p.send(`{"type":"terminal.input","id":"i","terminalId":"` + id + `","data":"` + base64.StdEncoding.EncodeToString([]byte(text)) + `"}`)
}

func waitScreen(t *testing.T, s *terminal.Terminal, want string) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for !strings.Contains(s.Screen(), want) {
		if time.Now().After(deadline) {
			t.Fatalf("screen never showed %q:\n%s", want, s.Screen())
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestAnAttachGetsTheScreenThenItsOutputInOrder(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	s := run(t, terms, `echo before; read x; i=0; while [ $i -lt 300 ]; do echo line$i; i=$((i+1)); done; sleep 30`)
	id := s.Info().ID
	waitScreen(t, s, "before")
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
	if f := p.next(); f.msg["type"] != "terminal.attached" || f.msg["id"] != "a" || f.msg["cols"] != 80.0 || f.msg["rows"] != 24.0 {
		t.Fatalf("%+v", f)
	}
	if f := p.next(); f.kind != proto.FrameSnapshot || f.id != id || !bytes.Contains(f.payload, []byte("before")) {
		t.Fatalf("%+v", f)
	}
	p.input(id, "\n")
	out := string(p.until("line299"))
	at := -1
	for i := range 300 {
		next := strings.Index(out, "line"+strconv.Itoa(i)+"\r\n")
		if next <= at {
			t.Fatalf("line%d came out of order:\n%s", i, out)
		}
		at = next
	}
}

func TestAPhoneOpensAShellInATreeAndTypesIntoIt(t *testing.T) {
	terms := terminal.NewManager()
	project := t.TempDir()
	_, _, p := local(t, peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes}), withTerminals(t, terms, project))
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.create","id":"c","requestId":"r1","cwd":"` + project + `","cols":50,"rows":20}`)
	var id string
	var announced bool
	for id == "" || !announced {
		f := p.next()
		switch f.msg["type"] {
		case "terminal.created":
			if f.msg["id"] != "c" || f.msg["requestId"] != "r1" {
				t.Fatalf("%v", f.msg)
			}
			id = f.msg["terminalId"].(string)
		case "agent.creating":
			if _, has := f.msg["id"]; has || f.msg["requestId"] != "r1" || f.msg["cwd"] != project || f.msg["setup"] != false {
				t.Fatalf("%v", f.msg)
			}
			announced = true
		}
	}
	s := terms.Get(id)
	if info := s.Info(); info.Cwd != project || info.Cols != 50 || info.Rows != 20 {
		t.Fatalf("%+v", info)
	}
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
	p.input(id, "echo hi-$((40+2))\n")
	p.until("hi-42")
}

func TestACreateOutsideEveryProjectIsNotATree(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, t.TempDir()))
	helloWith(p, proto.CapTerminal)
	for _, raw := range []string{
		`{"type":"terminal.create","id":"c","requestId":"r1","cwd":"` + t.TempDir() + `"}`,
		`{"type":"terminal.create","id":"c","requestId":"r1"}`,
		`{"type":"terminal.create","id":"c","requestId":"r1","cwd":"relative"}`,
	} {
		p.send(raw)
		if m := p.recv(); m["code"] != proto.CodeNotATree {
			t.Fatalf("%s: %v", raw, m)
		}
	}
	p.send(`{"type":"terminal.create","id":"c","requestId":"r1","localId":"gone"}`)
	if m := p.recv(); m["code"] != proto.CodeUnknownLocal {
		t.Fatalf("%v", m)
	}
	if n := len(terms.List()); n != 0 {
		t.Fatalf("%d terminals opened", n)
	}
}

func TestTheListIsPushedWholeOnSpawnAndExit(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	helloWith(p, proto.CapTerminal)
	s := run(t, terms, `read x`)
	id := s.Info().ID
	m := p.recv()
	list, _ := m["terminals"].([]any)
	if m["type"] != "terminal.list" || m["id"] != nil || len(list) != 1 || list[0].(map[string]any)["id"] != id || list[0].(map[string]any)["cwd"] != "/" {
		t.Fatalf("%v", m)
	}
	s.Write([]byte("\n"))
	if m := p.recv(); m["type"] != "terminal.list" || len(m["terminals"].([]any)) != 0 {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"terminal.list","id":"l"}`)
	if m := p.recv(); m["type"] != "terminal.list" || m["id"] != "l" {
		t.Fatalf("%v", m)
	}
}

func TestAnAttachedPhoneHearsResizesAndTheExit(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	s := run(t, terms, `read x; exit 3`)
	id := s.Info().ID
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
	p.next()
	p.next()
	p.send(`{"type":"terminal.resize","id":"r","terminalId":"` + id + `","cols":40,"rows":12}`)
	got := map[string]map[string]any{}
	for len(got) < 2 {
		if f := p.next(); f.msg != nil && f.msg["type"] != "terminal.list" {
			got[f.msg["type"].(string)] = f.msg
		}
	}
	if got["ack"]["id"] != "r" || got["terminal.resized"]["cols"] != 40.0 || got["terminal.resized"]["rows"] != 12.0 {
		t.Fatalf("%v", got)
	}
	p.input(id, "\n")
	for {
		f := p.next()
		if f.msg["type"] == "terminal.exited" {
			if f.msg["terminalId"] != id || f.msg["code"] != 3.0 {
				t.Fatalf("%v", f.msg)
			}
			break
		}
	}
	p.send(`{"type":"terminal.attach","id":"b","terminalId":"` + id + `"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "b" {
		t.Fatalf("%v", m)
	}
}

func TestATerminalThatEndedIsRefusedAsEndedUnderTheRequestsID(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"t-gone"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "a" || m["code"] != proto.CodeTerminalEnded {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"terminal.input","id":"i","terminalId":"t-gone","data":"eA=="}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "i" || m["code"] != proto.CodeTerminalEnded {
		t.Fatalf("%v", m)
	}
}

// A phone that stops reading fills its socket, then its queue; the shell and
// the other viewer go on, and the stalled phone gets a fresh snapshot once it reads again.
func TestAStalledPhoneStallsNoOneAndCatchesUpWithASnapshot(t *testing.T) {
	const size = 32 << 20
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	s := run(t, terms, `read x; head -c `+strconv.Itoa(size)+` /dev/zero | tr '\000' x; echo; echo END; sleep 30`)
	id := s.Info().ID
	stalled := &phone{t: t, url: p.url}
	ws, _, err := p.dial(nil)
	if err != nil {
		t.Fatal(err)
	}
	stalled.ws = ws
	p.ws.SetReadLimit(-1)
	stalled.ws.SetReadLimit(-1)
	for _, q := range []*phone{p, stalled} {
		helloWith(q, proto.CapTerminal)
		q.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
		q.next()
		if f := q.next(); f.kind != proto.FrameSnapshot {
			t.Fatalf("%+v", f)
		}
	}

	start := time.Now()
	p.input(id, "\n")
	out := p.until("END")
	took := time.Since(start)
	t.Logf("the reading phone got %d MiB in %v (%.0f MiB/s) beside a stalled one", len(out)>>20, took.Round(time.Millisecond), float64(len(out))/(1<<20)/took.Seconds())
	if n := bytes.Count(out, []byte("x")); n != size {
		t.Fatalf("the reading phone lost output: %d of %d", n, size)
	}
	waitScreen(t, s, "END")

	snapshots := 0
	var tail []byte
	for !bytes.Contains(tail, []byte("END")) {
		f := stalled.next()
		switch {
		case f.kind == proto.FrameSnapshot:
			snapshots++
			tail = f.payload
		case f.kind == proto.FrameOutput:
			tail = append(tail[max(0, len(tail)-64):], f.payload...)
		}
	}
	if snapshots != 1 {
		t.Fatalf("the stalled phone caught up with %d fresh snapshots", snapshots)
	}
}

func TestDetachingStopsTheFrames(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	s := run(t, terms, `while read x; do echo got-$x; done`)
	id := s.Info().ID
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
	p.next()
	p.next()
	p.send(`{"type":"terminal.detach","id":"d","terminalId":"` + id + `"}`)
	if m := p.recv(); m["type"] != "ack" || m["id"] != "d" {
		t.Fatalf("%v", m)
	}
	s.Write([]byte("one\n"))
	waitScreen(t, s, "got-one")
	p.send(`{"type":"terminal.list","id":"l"}`)
	if f := p.next(); f.msg["type"] != "terminal.list" {
		t.Fatalf("a frame came after the detach: %+v", f)
	}
}

func TestAProcessInATerminalCannotReachTheOthers(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, withTerminals(t, terms, "/p"))
	s := run(t, terms, `read x`)
	helloWith(p, proto.CapTerminal)
	for _, raw := range []string{
		`{"type":"terminal.list","id":"l"}`,
		`{"type":"terminal.attach","id":"a","terminalId":"` + s.Info().ID + `"}`,
	} {
		p.send(raw)
		if m := p.recv(); m["code"] != proto.CodeScopeDenied {
			t.Fatalf("%s: %v", raw, m)
		}
	}
}

// Attaching again mid-stream, as a phone does on zoom, gets a snapshot that no
// output from the attach it replaced may follow.
func TestAttachingAgainSendsNothingOlderThanTheFreshSnapshot(t *testing.T) {
	terms := terminal.NewManager()
	_, _, p := local(t, peer.OwnerOf(1), withTerminals(t, terms, "/p"))
	s := run(t, terms, `read x; seq -f L%.0f 1 100000000`)
	id := s.Info().ID
	p.ws.SetReadLimit(-1)
	helloWith(p, proto.CapTerminal)
	p.send(`{"type":"terminal.attach","id":"a","terminalId":"` + id + `"}`)
	p.next()
	p.next()
	p.input(id, "\n")
	numbered := regexp.MustCompile(`L(\d+)`)
	lines := func(b []byte) []int {
		var out []int
		for _, m := range numbered.FindAllSubmatch(b, -1) {
			n, _ := strconv.Atoi(string(m[1]))
			out = append(out, n)
		}
		return out
	}
	for round := range 20 {
		p.send(`{"type":"terminal.attach","id":"b","terminalId":"` + id + `"}`)
		for f := p.next(); f.msg["type"] != "terminal.attached"; f = p.next() {
		}
		snap := p.next()
		for snap.kind != proto.FrameSnapshot {
			snap = p.next()
		}
		latest := slices.Max(append(lines(snap.payload), 0))
		// Up to the next snapshot, should this phone fall behind: output stays contiguous.
		var after []byte
		for f := p.next(); len(after) < 128<<10 && f.kind != proto.FrameSnapshot; f = p.next() {
			after = append(after, f.payload...)
		}
		// The last number may be cut short where reading stopped.
		if got := lines(after); len(got) > 1 && slices.Min(got[:len(got)-1]) <= latest {
			t.Fatalf("round %d: output from before the snapshot (L%d) came after it (L%d)", round, slices.Min(got[:len(got)-1]), latest)
		}
	}
}
