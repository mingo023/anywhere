package state

import (
	"context"
	"os"
	"strconv"
	"sync/atomic"
	"testing"
	"time"
)

func running(t *testing.T, path string, n *atomic.Int64) *Writer {
	t.Helper()
	old := WriteEvery
	WriteEvery = 10 * time.Millisecond
	t.Cleanup(func() { WriteEvery = old })
	w := NewWriter(path, func() File {
		return File{Version: Version, Terminals: []Terminal{{TerminalID: strconv.FormatInt(n.Load(), 10)}}}
	})
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		w.Run(ctx)
		close(done)
	}()
	t.Cleanup(func() {
		cancel()
		<-done
	})
	return w
}

func saved(path string) string {
	f, _ := Load(path)
	if len(f.Terminals) == 0 {
		return ""
	}
	return f.Terminals[0].TerminalID
}

func waitSaved(t *testing.T, path, want string) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(10 * time.Millisecond) {
		if saved(path) == want {
			return
		}
	}
	t.Fatalf("the file never held %q", want)
}

func TestWriterSkipsUnchangedSnapshots(t *testing.T) {
	path := Path(t.TempDir())
	var n atomic.Int64
	running(t, path, &n)
	waitSaved(t, path, "0")
	before, _ := os.Stat(path)
	time.Sleep(100 * time.Millisecond)
	if after, _ := os.Stat(path); !os.SameFile(before, after) {
		t.Fatal("an unchanged snapshot was written again")
	}
	n.Store(1)
	waitSaved(t, path, "1")
}

func TestFreezeStopsLaterWrites(t *testing.T) {
	path := Path(t.TempDir())
	var n atomic.Int64
	w := running(t, path, &n)
	n.Store(1)
	if err := w.Freeze(); err != nil {
		t.Fatal(err)
	}
	n.Store(2)
	time.Sleep(100 * time.Millisecond)
	if got := saved(path); got != "1" {
		t.Fatalf("the file holds %q after Freeze, want 1", got)
	}
}
