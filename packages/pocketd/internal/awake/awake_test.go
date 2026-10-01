package awake

import (
	"context"
	"sync/atomic"
	"testing"
	"time"
)

type fakeSleeper struct{ calls chan string }

func (s fakeSleeper) Hold(string) error { s.calls <- "hold"; return nil }
func (s fakeSleeper) Release() error    { s.calls <- "release"; return nil }

type rig struct {
	calls  chan string
	kick   chan struct{}
	linger chan time.Time
	busy   atomic.Bool
}

func start(t *testing.T) *rig {
	r := &rig{calls: make(chan string, 8), kick: make(chan struct{}), linger: make(chan time.Time, 1)}
	k := &Keeper{S: fakeSleeper{r.calls}, Linger: 2 * time.Minute, After: func(time.Duration) <-chan time.Time { return r.linger }}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go k.Run(ctx, r.kick, r.busy.Load)
	return r
}

// set kicks twice: the second send returns only once the first kick is handled.
func (r *rig) set(busy bool) {
	r.busy.Store(busy)
	r.kick <- struct{}{}
	r.kick <- struct{}{}
}

func (r *rig) expect(t *testing.T, want string) {
	t.Helper()
	select {
	case got := <-r.calls:
		if got != want {
			t.Fatalf("got %s, want %s", got, want)
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("no %s", want)
	}
}

func (r *rig) none(t *testing.T) {
	t.Helper()
	select {
	case got := <-r.calls:
		t.Fatalf("unexpected %s", got)
	default:
	}
}

func TestKeeperHoldsWhileAnAgentIsWorking(t *testing.T) {
	r := start(t)
	r.set(false)
	r.none(t)
	r.set(true)
	r.expect(t, "hold")
	r.set(true)
	r.none(t)
}

func TestKeeperReleasesAfterTheLinger(t *testing.T) {
	r := start(t)
	r.set(true)
	r.expect(t, "hold")
	r.set(false)
	r.none(t)
	r.linger <- time.Now()
	r.expect(t, "release")
}

func TestKeeperKeepsHoldingWhenWorkResumesWithinTheLinger(t *testing.T) {
	r := start(t)
	r.set(true)
	r.expect(t, "hold")
	r.set(false)
	r.set(true)
	r.linger <- time.Now()
	r.set(true)
	r.none(t)
}

func TestIOKitHoldsThenReleases(t *testing.T) {
	s := &IOKit{}
	if err := s.Hold("pocketd test"); err != nil {
		t.Fatal(err)
	}
	if err := s.Release(); err != nil {
		t.Fatal(err)
	}
}
