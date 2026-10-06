package main

import (
	"bufio"
	"context"
	"errors"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/terminal"
)

// sockPath is short enough for a unix socket on macOS; t.TempDir() isn't.
func sockPath(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "pk")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	return filepath.Join(dir, "s")
}

// answering serves hooks on ln with hook, counting calls.
func answering(ln interface{ Close() error }, serve func(*ops.Server) error, hook func() ([]byte, error)) *atomic.Int32 {
	var calls atomic.Int32
	go serve(&ops.Server{Terminals: terminal.NewManager(), Hook: func(context.Context, peer.Principal, ops.Msg) ([]byte, error) {
		calls.Add(1)
		return hook()
	}})
	return &calls
}

func TestAHookResendsWhenTheSocketDropsBeforeTheAnswer(t *testing.T) {
	sock := sockPath(t)
	ln, err := ops.Listen(sock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close() })
	dropped := make(chan struct{})
	calls := answering(ln, func(s *ops.Server) error {
		c, err := ln.Accept()
		if err != nil {
			return err
		}
		bufio.NewReader(c).ReadBytes('\n')
		c.Close()
		close(dropped)
		return s.Serve(ln)
	}, func() ([]byte, error) { return []byte(`{"ok":true}`), nil })

	out := relay(sock, "t-1", []byte(`{}`), 5*time.Second)
	<-dropped
	if string(out) != `{"ok":true}` || calls.Load() != 1 {
		t.Fatalf("out = %q after %d answered sends", out, calls.Load())
	}
}

func TestAHookGivesUpAfterItsDeadline(t *testing.T) {
	start := time.Now()
	out := relay(sockPath(t), "t-1", []byte(`{}`), 300*time.Millisecond)
	if took := time.Since(start); out != nil || took < 300*time.Millisecond || took > 2*time.Second {
		t.Fatalf("out = %q after %v", out, took)
	}
}

func TestAnErrorReplyIsNotResent(t *testing.T) {
	sock := sockPath(t)
	ln, err := ops.Listen(sock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close() })
	calls := answering(ln, func(s *ops.Server) error { return s.Serve(ln) },
		func() ([]byte, error) { return nil, errors.New("no agent here") })

	start := time.Now()
	out := relay(sock, "t-1", []byte(`{}`), 5*time.Second)
	if out != nil || calls.Load() != 1 || time.Since(start) > time.Second {
		t.Fatalf("out = %q, %d sends, %v", out, calls.Load(), time.Since(start))
	}
}
