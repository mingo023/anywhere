package main

import (
	"context"
	"crypto/sha256"
	"errors"
	"os"
	"time"
)

// selfWatch upgrades pocketd when the binary it runs from changes, as when
// Sparkle swaps the app bundle or `make pocketd` rebuilds it. It compares
// contents, since a dev rebuild keeps its version.
type selfWatch struct {
	exe  string
	sum  [32]byte
	seen os.FileInfo
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

// check upgrades once per changed binary; a busy upgrader leaves it for the next check.
func (w *selfWatch) check(upgrade func() error) {
	st, err := os.Stat(w.exe)
	if err != nil || os.SameFile(st, w.seen) && st.ModTime().Equal(w.seen.ModTime()) && st.Size() == w.seen.Size() {
		return
	}
	sum, err := fileSum(w.exe)
	if err != nil {
		return
	}
	if sum != w.sum && errors.Is(upgrade(), errBusy) {
		return
	}
	w.seen = st
}

func fileSum(path string) ([32]byte, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return [32]byte{}, err
	}
	return sha256.Sum256(b), nil
}
