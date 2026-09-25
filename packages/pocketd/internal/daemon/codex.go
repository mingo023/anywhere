package daemon

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"slices"
	"sync"
	"sync/atomic"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/codex"
	"pocketd/internal/session"
)

var CodexThreadWait = 30 * time.Second

// spawnCodex runs the TUI against the account's app-server so pocketd can
// join the same thread as a second client.
func (d *Daemon) spawnCodex(spec session.Spec) (*session.Session, error) {
	bin, err := session.LookPath(spec.Cmd, spec.Env)
	if err != nil {
		return nil, err
	}
	start := exec.Command(bin, "app-server", "daemon", "start")
	start.Env = spec.Env
	// The daemon it forks may keep our output pipes open for its lifetime.
	start.WaitDelay = time.Second
	if out, err := start.CombinedOutput(); err != nil && !errors.Is(err, exec.ErrWaitDelay) {
		return nil, fmt.Errorf("codex app-server daemon start: %v: %s", err, out)
	}
	sock := codex.Sock(spec.Env)
	// The new thread is found by diffing the loaded list around the spawn,
	// so two spawns on one account must not overlap.
	lock, _ := d.codexLocks.LoadOrStore(lockKey(sock), &sync.Mutex{})
	mu := lock.(*sync.Mutex)
	mu.Lock()
	ctx, cancel := context.WithTimeout(context.Background(), CodexThreadWait)
	before, err := codex.Loaded(ctx, sock)
	if err == nil {
		spec.Args = append([]string{"--remote", "unix://" + sock, "-C", spec.Cwd}, spec.Args...)
		var s *session.Session
		if s, err = d.Sessions.Spawn(spec); err == nil {
			go func() {
				threadID := newThread(ctx, sock, before, s.Done())
				cancel()
				mu.Unlock()
				if threadID != "" {
					d.followCodex(s, sock, spec.Cwd, threadID)
				}
			}()
			return s, nil
		}
	}
	cancel()
	mu.Unlock()
	return nil, err
}

func newThread(ctx context.Context, sock string, before []string, exited <-chan struct{}) string {
	for {
		ids, _ := codex.Loaded(ctx, sock)
		for _, id := range ids {
			if !slices.Contains(before, id) {
				return id
			}
		}
		select {
		case <-ctx.Done():
			return ""
		case <-exited:
			return ""
		case <-time.After(200 * time.Millisecond):
		}
	}
}

func lockKey(sock string) string {
	abs, _ := filepath.Abs(sock)
	if real, err := filepath.EvalSymlinks(abs); err == nil {
		return real
	}
	return abs
}

func (d *Daemon) followCodex(s *session.Session, sock, cwd, threadID string) {
	drv := &codexDriver{s: s}
	d.track(s, threadID, cwd, "codex", func(a *agent.Agent) agent.Driver { drv.a = a; return drv }, func(ctx context.Context, a *agent.Agent) {
		thread, err := codex.Open(ctx, sock, threadID, threadID, a, d.Broker)
		if err != nil {
			<-ctx.Done()
			return
		}
		drv.thread.Store(thread)
		defer thread.Close()
		select {
		case <-ctx.Done():
		case <-thread.Done():
		}
	})
}

// codexDriver types into the TUI until the thread has its first turn:
// the app-server refuses to resume it before then.
type codexDriver struct {
	s      *session.Session
	a      *agent.Agent
	thread atomic.Pointer[codex.Session]
}

func (c *codexDriver) Prompt(text string) error {
	if t := c.thread.Load(); t != nil {
		return t.Prompt(text)
	}
	return c.s.Prompt(text)
}

func (c *codexDriver) Interrupt() error {
	if t := c.thread.Load(); t != nil {
		return t.Interrupt()
	}
	return c.s.Write([]byte{0x1b})
}

func (c *codexDriver) Compact() error {
	c.a.SetCompacting()
	if t := c.thread.Load(); t != nil {
		return t.Compact()
	}
	return c.s.Prompt("/compact")
}

func (c *codexDriver) Close() { c.s.Close() }
