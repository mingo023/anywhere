package broker

import (
	"context"
	"encoding/json"
	"strconv"
	"testing"
	"time"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

func ask(b *Broker, ctx context.Context, agentID, key string) <-chan string {
	out := make(chan string, 1)
	go func() { out <- b.Ask(ctx, agentID, "Bash", proto.ToolDetail{Kind: "shell", Command: "ls"}, key) }()
	return out
}

func waitOpen(t *testing.T, b *Broker, n int) []proto.PermissionRequest {
	t.Helper()
	for range 100 {
		if open := b.Open(); len(open) == n {
			return open
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("want %d open", n)
	return nil
}

func types(ch <-chan []byte) []string {
	var out []string
	for {
		select {
		case raw := <-ch:
			var m struct{ Type, Decision string }
			json.Unmarshal(raw, &m)
			out = append(out, m.Type+":"+m.Decision)
		default:
			return out
		}
	}
}

func TestPhoneAnswers(t *testing.T) {
	h := hub.New()
	msgs, _ := h.Subscribe()
	b := New(h)
	got := ask(b, context.Background(), "a1", "k")
	req := waitOpen(t, b, 1)[0]
	if !b.Resolve(req.RequestID, "allow") || <-got != "allow" {
		t.Fatal("not allowed")
	}
	if b.Resolve(req.RequestID, "deny") {
		t.Fatal("resolved twice")
	}
	if ts := types(msgs); len(ts) != 2 || ts[0] != "permission.request:" || ts[1] != "permission.resolved:allow" {
		t.Fatal(ts)
	}
}

func TestDesktopAnswersFirst(t *testing.T) {
	b := New(hub.New())
	got := ask(b, context.Background(), "a1", "k")
	waitOpen(t, b, 1)
	b.Dismiss("other", "allow")
	waitOpen(t, b, 1)
	b.Dismiss("k", "allow")
	if d := <-got; d != "" {
		t.Fatalf("got %q", d)
	}
	waitOpen(t, b, 0)
}

func TestHookGoneClosesRequest(t *testing.T) {
	b := New(hub.New())
	ctx, cancel := context.WithCancel(context.Background())
	got := ask(b, ctx, "a1", "k")
	waitOpen(t, b, 1)
	cancel()
	if <-got != "" {
		t.Fatal("want empty")
	}
	waitOpen(t, b, 0)
}

func TestTimeoutDenies(t *testing.T) {
	Timeout = 20 * time.Millisecond
	defer func() { Timeout = 10 * time.Minute }()
	b := New(hub.New())
	if d := <-ask(b, context.Background(), "a1", "k"); d != "deny" {
		t.Fatal(d)
	}
}

func TestDenyAllForClosedAgent(t *testing.T) {
	b := New(hub.New())
	mine := ask(b, context.Background(), "a1", "k1")
	other := ask(b, context.Background(), "a2", "k2")
	waitOpen(t, b, 2)
	b.DenyAll("a1")
	if <-mine != "deny" {
		t.Fatal("a1 not denied")
	}
	if len(waitOpen(t, b, 1)) != 1 {
		t.Fatal()
	}
	b.DenyAll("a2")
	<-other
}

func TestOpenInCreationOrder(t *testing.T) {
	b := New(hub.New())
	var answers []<-chan string
	for n := range 20 {
		answers = append(answers, ask(b, context.Background(), "a1", strconv.Itoa(n)))
		waitOpen(t, b, n+1)
	}
	for n, req := range b.Open() {
		if req.RequestID != "perm-"+strconv.Itoa(n+1) {
			t.Fatalf("%d: %s", n, req.RequestID)
		}
	}
	b.DenyAll("a1")
	for _, a := range answers {
		<-a
	}
}

func TestDismissTakesOldestOfIdenticalRequests(t *testing.T) {
	b := New(hub.New())
	first := ask(b, context.Background(), "a1", "k")
	waitOpen(t, b, 1)
	second := ask(b, context.Background(), "a1", "k")
	waitOpen(t, b, 2)
	b.Dismiss("k", "allow")
	if d := <-first; d != "" {
		t.Fatalf("got %q", d)
	}
	if open := waitOpen(t, b, 1); open[0].RequestID != "perm-2" {
		t.Fatal(open[0].RequestID)
	}
	b.DenyAll("a1")
	<-second
}

func TestResolveWinsOverHookGone(t *testing.T) {
	for range 500 {
		b := New(hub.New())
		ctx, cancel := context.WithCancel(context.Background())
		got := ask(b, ctx, "a1", "k")
		var id string
		for id == "" {
			if open := b.Open(); len(open) == 1 {
				id = open[0].RequestID
			}
		}
		cancel()
		if b.Resolve(id, "allow") {
			if d := <-got; d != "allow" {
				t.Fatalf("resolved allow, Ask returned %q", d)
			}
		} else {
			<-got
		}
	}
}

func TestRequestPublishedBeforeVisible(t *testing.T) {
	for range 500 {
		h := hub.New()
		msgs, _ := h.Subscribe()
		b := New(h)
		got := ask(b, context.Background(), "a1", "k")
		for len(b.Open()) == 0 {
		}
		if ts := types(msgs); len(ts) != 1 || ts[0] != "permission.request:" {
			t.Fatalf("open before published: %v", ts)
		}
		b.DenyAll("a1")
		<-got
	}
}
