package main

import (
	"bytes"
	"cmp"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"sync"
	"syscall"
	"time"

	"pocketd/internal/daemon"
	"pocketd/internal/events"
	"pocketd/internal/handoff"
	"pocketd/internal/host"
	"pocketd/internal/lock"
	"pocketd/internal/ops"
	"pocketd/internal/proto"
)

var (
	errBusy      = errors.New("a session is being created; upgrade after it")
	errNoHandoff = errors.New("Windows can't hand terminals to a new pocketd; restart it instead, which ends its terminals")
)

// upgrader execs the binary at exe in this pid. Holders keep the Terminals
// running (ADR 0008); the exec hands down each Agent's place, the lock and the
// MCP listener, so nothing notices but a reconnect.
type upgrader struct {
	exe, home string
	d         *daemon.Daemon
	lock      *lock.Lock
	starting  func() bool
	evs       *events.Log
	mon       *host.Monitor
	// handover runs just before the exec: the orchestrator drains its effects and flushes the store.
	// resume runs when the exec failed and this pocketd goes on.
	handover, resume func()
	// mcp is the MCP listener, handed down to the new pocketd so it never stops listening; nil when tools are off.
	mcp net.Listener
	mu  sync.Mutex
}

// run returns only when the upgrade didn't happen; the old pocketd goes on.
func (u *upgrader) run() error {
	if runtime.GOOS == "windows" {
		return errNoHandoff
	}
	u.mu.Lock()
	defer u.mu.Unlock()
	if u.starting() {
		return errBusy
	}
	to, err := binaryVersion(u.exe)
	if err != nil {
		return u.failed("", "not_runnable", err)
	}
	path := handoff.Path(u.home)
	file := handoff.File{Format: handoff.Format, Terminals: u.d.Carry()}
	if file.LockFD, err = u.lock.Keep(); err != nil {
		u.d.Release()
		return u.failed(to, "lock", err)
	}
	defer handoff.Release(file.LockFD)
	if u.mcp != nil {
		if file.MCPFD, err = handoff.Keep(u.mcp); err != nil {
			log.Printf("upgrade: mcp listener stays behind: %v", err)
		}
		defer handoff.Release(file.MCPFD)
	}
	if err := handoff.Write(path, file); err != nil {
		return u.abort(path, to, "write", err)
	}
	if err := dryRun(u.exe, path); err != nil {
		return u.abort(path, to, "check", err)
	}
	ok := true
	u.evs.Emit(events.Event{Kind: "upgrade", Version: to, OK: &ok})
	if u.handover != nil {
		u.handover()
	}
	// The exec ends every goroutine of this pocketd at once, so the next one, in this pid, opens and imports
	// the state with nothing of this one left writing. --version and handoff --check, the only runs of the
	// new binary beside this one, touch no state.
	err = syscall.Exec(u.exe, []string{u.exe, "serve", "--handoff", path}, os.Environ())
	if u.resume != nil {
		u.resume()
	}
	return u.abort(path, to, "exec", err)
}

func (u *upgrader) abort(path, to, reason string, err error) error {
	u.d.Release()
	os.Remove(path)
	return u.failed(to, reason, err)
}

func (u *upgrader) failed(to, reason string, err error) error {
	log.Printf("upgrade to %s: %s: %v", cmp.Or(to, "?"), reason, err)
	ok := false
	u.evs.Emit(events.Event{Kind: "upgrade", Version: to, OK: &ok, Reason: reason})
	u.mon.Set(func(s *proto.HostState) { s.UpgradeFailed = cmp.Or(to, "unknown") })
	return fmt.Errorf("upgrade to %s: %s: %w", cmp.Or(to, "?"), reason, err)
}

// binaryVersion runs exe --version, which also proves it starts.
func binaryVersion(exe string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, exe, "--version").Output()
	if err != nil {
		return "", err
	}
	v, ok := strings.CutPrefix(strings.TrimSpace(string(out)), "pocketd ")
	if !ok {
		return "", fmt.Errorf("not pocketd: %q", out)
	}
	return v, nil
}

// dryRun has the new binary check the handoff file. It inherits the fds the
// file names, which survive exec.
func dryRun(exe, path string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if out, err := exec.CommandContext(ctx, exe, "handoff", "--check", path).CombinedOutput(); err != nil {
		return fmt.Errorf("%w: %s", err, bytes.TrimSpace(out))
	}
	return nil
}

// upgradeCmd asks pocketd to exec its binary on disk. Success closes the
// socket mid-exec, so the answer is the next pocketd's status.
func upgradeCmd(sock string, out io.Writer) error {
	c, err := dial(sock)
	if err != nil {
		return fmt.Errorf("pocketd isn't running: %w", err)
	}
	defer c.Close()
	if err := c.Send(ops.Msg{Op: "upgrade"}); err != nil {
		return err
	}
	if m, err := c.Recv(); err == nil {
		return errors.New(cmp.Or(m.Error, "unexpected reply "+m.Ev))
	}
	for deadline := time.Now().Add(10 * time.Second); time.Now().Before(deadline); time.Sleep(100 * time.Millisecond) {
		if s, err := statusOf(sock); err == nil {
			fmt.Fprintf(out, "pocketd upgraded to %s (pid %d)\n", s.Version, s.PID)
			return nil
		}
	}
	return errors.New("pocketd didn't come back after the upgrade; see its log")
}

func statusOf(sock string) (*ops.Status, error) {
	c, err := ops.Dial(sock)
	if err != nil {
		return nil, err
	}
	defer c.Close()
	if err := c.Send(ops.Msg{Op: "status"}); err != nil {
		return nil, err
	}
	m, err := c.Recv()
	if err != nil {
		return nil, err
	}
	if m.Status == nil {
		return nil, errors.New(cmp.Or(m.Error, "no status in reply"))
	}
	return m.Status, nil
}
