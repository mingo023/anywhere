package hosts

import (
	"context"
	"encoding/json"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/ops"
)

func TestALinkThatReachedPocketdRetriesQuicklyAfterItDrops(t *testing.T) {
	var b Backoff
	var waits []time.Duration
	for range 3 {
		_, wait, _ := b.Fail(FailRefused)
		waits = append(waits, wait)
	}
	if waits[0] != time.Second || waits[1] != 2*time.Second || waits[2] != 4*time.Second {
		t.Fatalf("%v", waits)
	}
	b.Reset()
	link, wait, retry := b.Fail(FailDropped)
	if link != (Link{State: StateRetrying, Failure: FailDropped}) || wait != time.Second || !retry {
		t.Fatalf("%+v %v %v", link, wait, retry)
	}
}

func TestARefusedLoginWaitsForReconnect(t *testing.T) {
	var b Backoff
	link, _, retry := b.Fail(FailAuth)
	if link != (Link{State: StateFailed, Failure: FailAuth}) || retry {
		t.Fatalf("%+v %v", link, retry)
	}
}

func TestHostsThatDroppedTogetherRetryAtDifferentTimes(t *testing.T) {
	var waits []time.Duration
	for range 20 {
		w := jitter(8*time.Second, FailDropped)
		if w <= 4*time.Second || w > 8*time.Second {
			t.Fatalf("%v", w)
		}
		waits = append(waits, w)
	}
	for _, w := range waits {
		if w != waits[0] {
			return
		}
	}
	t.Fatalf("every wait was %v", waits[0])
}

func TestEventsFromAClosedTunnelAreSkipped(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	events := make(chan event, 4)
	events <- event{evExited, 1}
	events <- event{evReady, 2}
	if e, ok := next(ctx, events, 2, time.Second); !ok || e != (event{evReady, 2}) {
		t.Fatalf("%+v %v", e, ok)
	}
	events <- event{evExited, 2}
	if e, ok := next(ctx, events, 0, time.Millisecond); ok {
		t.Fatalf("a closed tunnel's exit cut a retry wait short: %+v", e)
	}
	cancel()
	if e, ok := next(ctx, events, 0, -1); !ok || e.kind != evStop {
		t.Fatalf("%+v %v", e, ok)
	}
}

func TestAThrottledHostNeverRetriesBeforeItsPenaltyEnds(t *testing.T) {
	for range 20 {
		if w := jitter(20*time.Second, FailThrottled); w < 20*time.Second {
			t.Fatalf("%v", w)
		}
	}
}

// watching is a supervisor whose tunnel is up to a pocketd that answers its nth status only when answers says;
// asked counts them. The rest it holds open unanswered.
func watching(t *testing.T, answers func(n int32) bool) (s *supervisor, socket string, asked *atomic.Int32) {
	t.Helper()
	// t.TempDir is past a unix socket path's limit on macOS.
	dir, err := os.MkdirTemp("/tmp", "hl")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	socket = filepath.Join(dir, "p.sock")
	ln, err := net.Listen("unix", socket)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close() })
	asked = &atomic.Int32{}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			var m ops.Msg
			json.NewDecoder(c).Decode(&m)
			if answers(asked.Add(1)) {
				json.NewEncoder(c).Encode(ops.Msg{Status: &ops.Status{Version: "1.2.0"}})
			}
			go func() {
				io.Copy(io.Discard, c)
				c.Close()
			}()
		}
	}()
	m := &Manager{links: map[string]Link{}, sups: map[string]*supervisor{}, changed: make(chan struct{}), remotes: map[string]*remote{}}
	s = &supervisor{m: m, id: "devbox", events: make(chan event, 16), done: make(chan struct{}), every: 10 * time.Millisecond, within: 50 * time.Millisecond}
	m.sups[s.id] = s
	s.events <- event{evReady, 1}
	return s, socket, asked
}

type watched struct {
	e       end
	failure Failure
}

func watch(s *supervisor, ctx context.Context, socket string) <-chan watched {
	out := make(chan watched, 1)
	go func() {
		e, failure := s.watch(ctx, socket, "", 1, &Backoff{})
		out <- watched{e, failure}
	}()
	return out
}

func (s *supervisor) link() Link {
	s.m.mu.Lock()
	defer s.m.mu.Unlock()
	return s.m.links[s.id]
}

func TestAHostWhosePocketdTakesConnectionsButNeverAnswersStopsConnectingAndRetries(t *testing.T) {
	s, socket, _ := watching(t, func(int32) bool { return false })
	select {
	case got := <-watch(s, context.Background(), socket):
		if got != (watched{endFailed, FailTimedOut}) {
			t.Fatalf("%+v", got)
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("still %+v", s.link())
	}
	if link, _, retry := new(Backoff).Fail(FailTimedOut); link.State != StateRetrying || !retry {
		t.Fatalf("%+v %v", link, retry)
	}
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); !ok(); time.Sleep(5 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatal(what)
		}
	}
}

func TestAHostThatAnswersItsSecondStatusComesOnline(t *testing.T) {
	s, socket, _ := watching(t, func(n int32) bool { return n >= 2 })
	ctx, cancel := context.WithCancel(context.Background())
	done := watch(s, ctx, socket)
	eventually(t, "never online", func() bool { return s.link().State == StateOnline })
	cancel()
	if got := <-done; got.e != endStop {
		t.Fatalf("%+v", got)
	}
}

func TestAnOnlineHostThatStopsAnsweringForAWhileStaysOnline(t *testing.T) {
	s, socket, asked := watching(t, func(n int32) bool { return n == 1 || n >= 8 })
	ctx, cancel := context.WithCancel(context.Background())
	done := watch(s, ctx, socket)
	eventually(t, "never asked past its misses", func() bool { return asked.Load() >= 9 })
	select {
	case got := <-done:
		t.Fatalf("a slow host's link ended: %+v", got)
	default:
	}
	if l := s.link(); l.State != StateOnline {
		t.Fatalf("%+v", l)
	}
	cancel()
	<-done
}
