package state

import (
	"bytes"
	"context"
	"encoding/json"
	"log"
	"sync"
	"time"
)

var WriteEvery = time.Second

// Writer saves a snapshot whenever it changes.
type Writer struct {
	path string
	snap func() File

	mu     sync.Mutex
	last   []byte
	frozen bool
	failed bool
}

func NewWriter(path string, snap func() File) *Writer {
	return &Writer{path: path, snap: snap}
}

// Run saves every WriteEvery until ctx ends or Freeze.
func (w *Writer) Run(ctx context.Context) {
	tick := time.NewTicker(WriteEvery)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			w.mu.Lock()
			if !w.frozen {
				w.save()
			}
			w.mu.Unlock()
		}
	}
}

// Freeze saves once more and stops Run from writing, so Terminals that end
// while pocketd shuts down stay in the file.
func (w *Writer) Freeze() error {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.frozen = true
	return w.save()
}

// save logs only the first of a run of failures; the next change retries.
func (w *Writer) save() error {
	raw, err := json.MarshalIndent(w.snap(), "", "  ")
	if err != nil || bytes.Equal(raw, w.last) {
		return err
	}
	if err := write(w.path, raw); err != nil {
		if !w.failed {
			log.Printf("state: %v", err)
		}
		w.failed = true
		return err
	}
	w.last, w.failed = raw, false
	return nil
}
