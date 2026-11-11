package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"strconv"
	"sync"
	"testing"
	"time"
)

// peer is the other end of a Conn's wire, driven by a test.
type peer struct {
	in     chan []byte // to the conn
	out    chan []byte // from the conn
	closed chan struct{}
	once   sync.Once
}

func (p *peer) Read(context.Context) ([]byte, error) {
	select {
	case b := <-p.in:
		return b, nil
	case <-p.closed:
		return nil, errors.New("closed")
	}
}

func (p *peer) Write(_ context.Context, msg []byte) error {
	select {
	case p.out <- msg:
		return nil
	case <-p.closed:
		return errors.New("closed")
	}
}

func (p *peer) Close() { p.once.Do(func() { close(p.closed) }) }

func (p *peer) say(t *testing.T, msg string) {
	t.Helper()
	select {
	case p.in <- []byte(msg):
	case <-time.After(5 * time.Second):
		t.Fatalf("conn never read %s", msg)
	}
}

func (p *peer) next(t *testing.T) Message {
	t.Helper()
	select {
	case raw := <-p.out:
		var m Message
		json.Unmarshal(raw, &m)
		return m
	case <-time.After(5 * time.Second):
		t.Fatal("conn wrote nothing")
		return Message{}
	}
}

func connect(t *testing.T, version string) (*Conn, *peer, chan Message) {
	p := &peer{in: make(chan []byte), out: make(chan []byte, 10), closed: make(chan struct{})}
	handled := make(chan Message, 10)
	c := Connect(p, version, func(m Message) { handled <- m })
	t.Cleanup(c.Close)
	return c, p, handled
}

type reply struct {
	m   Message
	err error
}

func call(c *Conn, ctx context.Context, method string) chan reply {
	ch := make(chan reply, 1)
	go func() {
		m, err := c.Call(ctx, method, map[string]int{"n": 1})
		ch <- reply{m, err}
	}()
	return ch
}

func TestCallReturnsTheReplyToItsID(t *testing.T) {
	c, p, _ := connect(t, "2.0")
	got := call(c, context.Background(), "add")
	req := p.next(t)
	if req.Version != "2.0" || req.Method != "add" || string(req.Params) != `{"n":1}` {
		t.Fatalf("request %+v", req)
	}
	p.say(t, `{"jsonrpc":"2.0","id":`+string(req.ID)+`,"error":{"code":-32601,"message":"no add"}}`)
	r := <-got
	if r.err != nil || r.m.Error == nil || r.m.Error.Code != -32601 || r.m.Error.Message != "no add" {
		t.Fatalf("%+v %v", r.m, r.err)
	}
}

func TestAConnWithNoVersionSendsNone(t *testing.T) {
	c, p, _ := connect(t, "")
	c.Reply(json.RawMessage(`"r1"`), map[string]bool{"ok": true})
	raw := <-p.out
	if string(raw) != `{"id":"r1","result":{"ok":true}}` {
		t.Fatalf("%s", raw)
	}
}

func TestTheHandlerTakesCallsNotificationsAndRepliesToCallsItDidntMake(t *testing.T) {
	_, p, handled := connect(t, "2.0")
	p.say(t, `{"jsonrpc":"2.0","id":"q1","method":"ask","params":{}}`)
	p.say(t, `{"jsonrpc":"2.0","method":"note"}`)
	p.say(t, `not json`)
	p.say(t, `{"jsonrpc":"2.0","id":"x","result":{}}`)
	for _, want := range []string{`ask "q1"`, `note `, ` "x"`} {
		m := <-handled
		if got := m.Method + " " + string(m.ID); got != want {
			t.Fatalf("%q, want %q", got, want)
		}
	}
}

func TestALateReplyToAnAbandonedCallReachesNoOne(t *testing.T) {
	c, p, handled := connect(t, "2.0")
	ctx, cancel := context.WithCancel(context.Background())
	got := call(c, ctx, "slow")
	req := p.next(t)
	cancel()
	if r := <-got; r.err != context.Canceled {
		t.Fatal(r.err)
	}
	p.say(t, `{"jsonrpc":"2.0","id":`+string(req.ID)+`,"result":{}}`)
	p.say(t, `{"jsonrpc":"2.0","method":"after"}`)
	if m := <-handled; m.Method != "after" {
		t.Fatalf("handler took %+v", m)
	}
}

func TestACallAfterCloseFails(t *testing.T) {
	c, _, _ := connect(t, "2.0")
	c.Close()
	if _, err := c.Call(context.Background(), "late", nil); err == nil {
		t.Fatal("call on a closed conn succeeded")
	}
}

func TestARefusalWhoseCodeIsNoIntegerStillAnswersItsCall(t *testing.T) {
	c, p, _ := connect(t, "")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	got := call(c, ctx, "add")
	req := p.next(t)
	p.say(t, `{"id":`+string(req.ID)+`,"error":{"code":"E_ADD","message":"no add"}}`)
	r := <-got
	if r.err != nil || r.m.Error == nil || r.m.Error.Message != "no add" {
		t.Fatalf("%+v %v", r.m, r.err)
	}
}

func TestACallTheOtherEndClosesOnFailsWithErrClosed(t *testing.T) {
	c, p, _ := connect(t, "2.0")
	got := call(c, context.Background(), "hang")
	p.next(t)
	p.Close()
	if r := <-got; r.err != ErrClosed {
		t.Fatal(r.err)
	}
	select {
	case <-c.Done():
	default:
		t.Fatal("Done is open")
	}
}

func TestIDsStartAnywhere(t *testing.T) {
	a, pa, _ := connect(t, "2.0")
	b, pb, _ := connect(t, "2.0")
	call(a, context.Background(), "m")
	call(b, context.Background(), "m")
	ia, _ := strconv.Atoi(string(pa.next(t).ID))
	ib, _ := strconv.Atoi(string(pb.next(t).ID))
	if ia == ib {
		t.Fatalf("both conns called by id %d", ia)
	}
}
