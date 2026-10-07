package main

import (
	"context"
	"crypto/sha256"
	"errors"
	"os"
	"time"
)

const (
	retryFirst = 30 * time.Second
	retryMax   = 5 * time.Minute
)

// selfWatch upgrades pocketd when the binary it runs from changes, as when
// Sparkle swaps the app bundle or `make pocketd` rebuilds it. It compares
// contents, since a dev rebuild keeps its version.
type selfWatch struct {
	exe  string
	sum  [32]byte
	seen os.FileInfo
	now  func() time.Time
	// retryAt is when a busy or failed upgrade gets another go; zero when none is due.
	retryAt time.Time
	backoff time.Duration
}

func (w *selfWatch) run(ctx context.Context, upgrade func() error) {
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(3 * time.Second):
			w.check(upgrade)
		}
	}
}

// check upgrades once per changed binary. A busy upgrader is tried again next
// tick; a failed one with backoff, since the app keeps speaking the new
// binary's protocol and a pause may only have been slow.
func (w *selfWatch) check(upgrade func() error) {
	st, err := os.Stat(w.exe)
	if err != nil {
		return
	}
	same := os.SameFile(st, w.seen) && st.ModTime().Equal(w.seen.ModTime()) && st.Size() == w.seen.Size()
	if same && (w.retryAt.IsZero() || w.now().Before(w.retryAt)) {
		return
	}
	sum, err := fileSum(w.exe)
	if err != nil {
		return
	}
	if !same {
		w.backoff = 0
	}
	w.seen, w.retryAt = st, time.Time{}
	if sum == w.sum {
		return
	}
	switch err := upgrade(); {
	case err == nil:
	case errors.Is(err, errBusy):
		w.retryAt = w.now()
	default:
		w.backoff = min(max(2*w.backoff, retryFirst), retryMax)
		w.retryAt = w.now().Add(w.backoff)
	}
}

func fileSum(path string) ([32]byte, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return [32]byte{}, err
	}
	return sha256.Sum256(b), nil
}
