package codex

import (
	"context"
	"fmt"
	"testing"
	"time"

	"pocketd/internal/codex/codextest"
)

type watchLog chan string

func (l watchLog) Connected()                         { l <- "connected" }
func (l watchLog) ThreadStarted(id string, root bool) { l <- fmt.Sprintf("started %s %v", id, root) }
func (l watchLog) ThreadStatus(id, typ string, flags []string) {
	l <- fmt.Sprintf("status %s %s %v", id, typ, flags)
}
func (l watchLog) ThreadClosed(id string) { l <- "closed " + id }

func (l watchLog) want(t *testing.T, want ...string) {
	t.Helper()
	for _, w := range want {
		select {
		case got := <-l:
			if got != w {
				t.Fatalf("got %q, want %q", got, w)
			}
		case <-time.After(5 * time.Second):
			t.Fatalf("never got %q", w)
		}
	}
}

func watch(t *testing.T, sock string) watchLog {
	ctx, cancel := context.WithCancel(context.Background())
	l := make(watchLog, 10)
	done := make(chan struct{})
	go func() {
		Watch(ctx, sock, l)
		close(done)
	}()
	t.Cleanup(func() {
		cancel()
		select {
		case <-done:
		case <-time.After(time.Second):
			t.Error("Watch outlived its ctx")
		}
	})
	return l
}

func TestWatchForwardsThreadBroadcasts(t *testing.T) {
	srv := codextest.Start(t, sock(t), codextest.OK)
	l := watch(t, srv.Sock)
	l.want(t, "connected")
	srv.Broadcast("thread/started", `{"thread":{"id":"th1","parentThreadId":null,"threadSource":"user","ephemeral":false}}`)
	srv.Broadcast("thread/started", `{"thread":{"id":"th2","parentThreadId":"th1","threadSource":"user","ephemeral":false}}`)
	srv.Broadcast("thread/started", `{"thread":{"id":"th3","parentThreadId":null,"threadSource":"exec","ephemeral":false}}`)
	srv.Broadcast("thread/started", `{"thread":{"id":"th4","parentThreadId":null,"threadSource":"user","ephemeral":true}}`)
	srv.Broadcast("turn/started", `{"threadId":"th1"}`)
	srv.Broadcast("thread/status/changed", `{"threadId":"th1","status":{"type":"active","activeFlags":["waitingOnApproval"]}}`)
	srv.Broadcast("thread/status/changed", `{"threadId":"th1","status":{"type":"idle"}}`)
	srv.Broadcast("thread/closed", `{"threadId":"th1"}`)
	l.want(t, "started th1 true", "started th2 false", "started th3 false", "started th4 false",
		"status th1 active [waitingOnApproval]", "status th1 idle []", "closed th1")
}

func TestWatchDropsBroadcastsWithoutAThreadID(t *testing.T) {
	srv := codextest.Start(t, sock(t), codextest.OK)
	l := watch(t, srv.Sock)
	l.want(t, "connected")
	srv.Broadcast("thread/started", `{"thread":{"parentThreadId":null,"threadSource":"user","ephemeral":false}}`)
	srv.Broadcast("thread/status/changed", `{"status":{"type":"idle"}}`)
	srv.Broadcast("thread/closed", `{}`)
	srv.Broadcast("thread/closed", `{"threadId":"th1"}`)
	l.want(t, "closed th1")
}

func TestWatchRedialsAfterTheServerRestarts(t *testing.T) {
	WatchRetry = 10 * time.Millisecond
	path := sock(t)
	srv := codextest.Start(t, path, codextest.OK)
	l := watch(t, path)
	l.want(t, "connected")
	srv.Close()
	srv = codextest.Start(t, path, codextest.OK)
	l.want(t, "connected")
	srv.Broadcast("thread/closed", `{"threadId":"th1"}`)
	l.want(t, "closed th1")
}
