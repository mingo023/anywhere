package hosts

import (
	"context"
	"testing"
	"time"
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
