package codex

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/codex/codextest"
)

type recorder struct {
	notes    chan string
	requests chan string
}

func (r *recorder) Notify(method string, _ json.RawMessage) { r.notes <- method }
func (r *recorder) Request(id json.RawMessage, method string, _ json.RawMessage) {
	r.requests <- string(id) + " " + method
}

func sock(t *testing.T) string {
	dir, _ := os.MkdirTemp("/tmp", "cx")
	t.Cleanup(func() { os.RemoveAll(dir) })
	return filepath.Join(dir, "s.Sock")
}

func dial(t *testing.T, s *codextest.Server) (*Client, *recorder) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	r := &recorder{notes: make(chan string, 10), requests: make(chan string, 10)}
	c, err := Dial(ctx, s.Sock, r)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(c.Close)
	return c, r
}

func TestHandshakeThenCall(t *testing.T) {
	s := codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/loaded/list" {
			return map[string]any{"data": []string{"th1"}}, ""
		}
		if method == "thread/resume" {
			return nil, "no rollout found"
		}
		return map[string]any{}, ""
	})
	c, _ := dial(t, s)
	if s.Next("initialize").ID == nil || s.Next("initialized").Method != "initialized" {
		t.Fatal("handshake")
	}
	res, err := c.Call(context.Background(), "thread/loaded/list", map[string]any{})
	if err != nil || string(res) != `{"data":["th1"]}` {
		t.Fatalf("%s %v", res, err)
	}
	if _, err := c.Call(context.Background(), "thread/resume", nil); err == nil || err.Error() != "thread/resume: no rollout found" {
		t.Fatal(err)
	}
}

func TestRoutesNotificationsAndServerRequests(t *testing.T) {
	s := codextest.Start(t, sock(t), codextest.OK)
	c, r := dial(t, s)
	s.Push("turn/started", nil, `{}`)
	s.Push("item/commandExecution/requestApproval", "req-7", `{}`)
	if got := <-r.notes; got != "turn/started" {
		t.Fatal(got)
	}
	if got := <-r.requests; got != `"req-7" item/commandExecution/requestApproval` {
		t.Fatal(got)
	}
	c.Reply(json.RawMessage(`"req-7"`), map[string]string{"decision": "accept"})
	if got := s.Reply(`"req-7"`); got != `{"decision":"accept"}` {
		t.Fatal(got)
	}
}

func TestCallForgetsAbandonedCalls(t *testing.T) {
	release := make(chan struct{})
	t.Cleanup(func() { close(release) })
	s := codextest.Start(t, sock(t), func(method string, _ json.RawMessage) (any, string) {
		if method == "slow" {
			<-release
		}
		return map[string]any{}, ""
	})
	c, _ := dial(t, s)
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	if _, err := c.Call(ctx, "slow", nil); err == nil {
		t.Fatal("slow call returned")
	}
	c.Close()
	if _, err := c.Call(context.Background(), "after-close", nil); err == nil {
		t.Fatal("call on closed conn succeeded")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if len(c.pending) != 0 {
		t.Fatalf("%d calls still pending", len(c.pending))
	}
}

type caller struct {
	c   chan *Client
	got chan error
}

func (h *caller) Notify(string, json.RawMessage) {
	_, err := (<-h.c).Call(context.Background(), "thread/loaded/list", map[string]any{})
	h.got <- err
}
func (h *caller) Request(json.RawMessage, string, json.RawMessage) {}

func TestHandlerMayCall(t *testing.T) {
	s := codextest.Start(t, sock(t), codextest.OK)
	h := &caller{c: make(chan *Client, 1), got: make(chan error, 1)}
	c, err := Dial(context.Background(), s.Sock, h)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(c.Close)
	h.c <- c
	s.Push("turn/started", nil, `{}`)
	select {
	case err := <-h.got:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Call from a handler deadlocked")
	}
}
