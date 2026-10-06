# pocketd Upgrades In Place Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** When `pocketd upgrade` is run, or the binary pocketd runs from is replaced by a different one, pocketd execs the new binary in its own pid. Every Terminal, shell, agent and open permission prompt survives; clients see a reconnect. If the new binary fails a dry run, the old pocketd keeps running and the desktop shows a toast.

**Architecture:**
- **Upgrade (old image).** `upgrader.run` (cmd/pocketd) does these steps in order:
  1. Returns `errBusy` while a launch is in flight.
  2. Runs `<exe> --version`.
  3. `Daemon.Handoff()` pauses every Terminal's PTY reader with a past read deadline, so unread output waits in the kernel, and describes each Terminal and its Agent.
  4. Writes `handoff.json`.
  5. Dry-runs `<exe> handoff --check <file>`.
  6. Calls `syscall.Exec(exe, serve --handoff <file>)`.
- **Adopt (new image).** `serve --handoff` reads the file. `Daemon.Adopt` then:
  - wraps each PTY fd in a new `Terminal` and replays its screen into a fresh VT;
  - lists each Agent under its old id through the restore-hint machinery, marked `handoff`, so it gets no restore notice;
  - resumes reading.
- **Permission prompts.** A Claude hook redials and resends when its socket drops (B1). Codex re-binds through the hint, which replays pending approvals.
- **Triggers.** A self-watch (every 3s) upgrades when the binary's contents change, whether Sparkle swapped the bundle or `make pocketd` rebuilt it. `pocketd upgrade` forces one.

**Toolset:**
- pocketd unit test: `cd packages/pocketd && go test ./internal/<pkg> -run '<TestName>' -count=1` (or `./cmd/pocketd`)
- pocketd e2e: `cd packages/pocketd && go test ./e2e -run '<TestName>' -count=1`
- pocketd all: `cd packages/pocketd && go vet ./... && go test -race -count=1 ./...`
- desktop: `cd packages/desktop && cargo test -p agents <name>` / `cargo test -p pocket <name>`; gates: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Full gate (repo root): `scripts/check.sh`. It uses a scratch `POCKET_HOME`. Never point tests at the live pocketd.

**Relation to the release plan:** `docs/plans/2026-10-06-release-and-updates.md`. This plan is independent of it and can merge in any order relative to its PR 1–5, but its PR 6 Task 6.4 (publishing v0.1.0) waits for all of this plan: the first release must already contain the hook retry and the old side of the handoff (ADR 0005).

**Read first:**
- `docs/adr/0005-pocketd-upgrades-in-place.md`: the decision and its rules.
- `CLAUDE.md` (repo root): test style (sentence names, no mocks), desktop layout.
- `packages/pocketd/internal/terminal/terminal.go`: `Spawn`, `pump`, `Resize`. The pty file and its reader are what change.
- `packages/pocketd/internal/daemon/restore.go`: `hint`, `expect`, `adopt`, `finish`. A handed-down Agent rides the same path.
- `packages/pocketd/internal/daemon/presence.go:85`: `attach`, where a handed-down hint settles.
- `packages/pocketd/cmd/pocketd/serve.go`: startup order: lock, reap, daemon, restore, listeners.
- `packages/pocketd/e2e/harness_test.go`: `Start`, `serve`, `Pocketd`, `StartClaude`.

## Why this approach

- **Exec in place, same pid.** launchd keeps the job, shells stay our children (their exit codes stay reapable), and `proc.Reap` spares them because their parent is still us. Rejected: a successor process (the shells would be orphaned), and restarting only when idle (could wait for days).
- **Pause by read deadline.** The PTY master is moved to a non-blocking fd the runtime poller owns. A deadline in the past stops `Read` without closing anything, and the kernel holds output until the next image reads it. This also means `Resize` must stop using `pty.Setsize`: it calls `Fd()`, which turns blocking mode back on.
- **The handoff file is versioned.** It is JSON with `format`, and `testdata/format1.json` pins the first release's shape, because a release must read the file the previous release wrote.
- **The dry run uses the new binary.** It runs on the real file with the fds already inheritable, so it proves the fds survive an exec and every screen replays, before the old image commits.
- **Adoption reuses restore hints.** The poller finds the agent's process as it does after a restart, and `adopt` binds it to the waiting Agent. The only differences are a `handoff` flag on the hint, which skips the notice and report, and carried name, phase and seq.
- **Out of scope:** passing listeners, carrying the Launcher's in-flight state, carrying scrollback, and phone UI.

## Tasks at a glance

| PR | Task | What | Main files | Risk |
|---|---|---|---|---|
| **B1 Hook retry** | 1.1 | hook redials until answered or 10s | `cmd/pocketd/hook.go` | low |
| **B2 Terminal handoff** | 2.1 | pollable pty, Pause/Resume, setsize, `proc` | `internal/terminal/terminal.go` | high |
| | 2.2 | Handoff / Adopt across a real exec | `internal/terminal/terminal.go` | high |
| **B3 Handoff file** | 3.1 | `handoff` package: format, Write/Read/Check | `internal/handoff/handoff.go` | low |
| | 3.2 | `pocketd handoff --check` | `cmd/pocketd/handoff.go`, `main.go` | low |
| **B4 Upgrade** | 4.1 | e2e harness + tests (fail until 4.5) | `e2e/harness_test.go`, `e2e/upgrade_test.go` | med |
| | 4.2 | carry name, phase, seq | `internal/timeline/timeline.go`, `internal/agent/agent.go` | low |
| | 4.3 | Daemon Handoff / Adopt, handed-over hints | `internal/daemon/handoff.go`, `restore.go`, `presence.go` | high |
| | 4.4 | launch in flight, `ops:upgrade`, HostState, events doc | `internal/launch`, `internal/ops`, `internal/peer`, `internal/proto` | low |
| | 4.5 | upgrader, `serve --handoff`, `pocketd upgrade` | `cmd/pocketd/upgrade.go`, `serve.go`, `main.go` | high |
| **B5 Triggers** | 5.1 | e2e: replacing the binary upgrades (fails until 5.2) | `e2e/*` | med |
| | 5.2 | self-watch | `cmd/pocketd/selfwatch.go`, `serve.go` | med |
| | 5.3 | `make pocketd` only builds | `Makefile` | low |
| | 5.4 | desktop toast | `crates/agents/src/agents.rs`, `crates/pocket/src/desktop/alerts.rs` | low |

---

## Notes for the implementer

Deviations from ADR 0005's first draft (the ADR now matches):

- **Listeners are not passed across exec. Only PTY masters are.**
  - Every client already reconnects: the Claude hook (with B1's retry), the desktop's reconnect loop, and phones.
  - Go sets `SO_REUSEADDR`, so the new image rebinds the same port, and `ops.Listen` removes and rebinds the socket.
  - Passing listeners would mean a second adopt path in `reach` and `ops` for a gap of under a second.
- **The lock is released and taken again, not carried.**
  - Exec closes the lock fd (CLOEXEC), which drops the flock, and the new image runs `lock.Acquire` again.
  - The only process that could take the lock in that window is a hand-started `pocketd serve`. launchd won't start one, because the job's pid is still alive.
- **The e2e tests are written first, but inside the PR that makes them pass** (B4, B5), not in a separate PR. A PR of failing tests would break `scripts/check.sh` on main.
- **PR order puts the hook retry first** (B1), as ADR 0005 requires: the old binary's hook has to retry when the first upgrade happens.
- **`ops:upgrade` is `Own`.** A process inside an Anywhere terminal is a PTY peer, so `pocketd upgrade` run there is refused with `scope_denied`, the same as `pocketd config set` today. Nothing needs it from there: the self-watch covers `make pocketd`.
- **The self-watch compares contents, not versions.**
  - A dirty dev build reports `<rev>-dirty` before and after a rebuild, so a version check would miss it.
  - A sha256 of the binary catches a rebuild and skips a touch, an identical rebuild, or a Sparkle update that left pocketd unchanged.
  - So `make pocketd` only builds, and no longer calls `launchctl kickstart -k`.
- **Agent state carried across exec beyond `state.Terminal`:** `Named`, `Phase`, `Epoch` and `Seq`. Phase is read from `Summary().Status`, so the only new getter is `Agent.Named()`.
- **Agents without a Conversation come back as new Agents with new ids**, exactly as a restore does today. Example: a claude in an untrusted folder, which sends no hooks. `pr.save` writes nothing for them.
- **The failure toast reuses `Desktop.error`.** `HostState.UpgradeFailed` is `omitempty`, so the server goldens don't change. The phone's TS `Schema.Struct` ignores excess keys, so `packages/protocol` is untouched.
- **When pocketd is down, a hook now waits up to 10s before it falls back.** Only processes inside pocketd terminals run the hook (`POCKETD_PTY`), and those die with pocketd, so in practice this is the upgrade gap.

Risks:

- **kqueue might not poll a PTY master on macOS.** If it doesn't, `SetReadDeadline` returns `os.ErrNoDeadline` and Pause fails.
  - Task 2.1's `TestAPausedTerminalKeepsItsOutputUntilResumed` catches it before anything else is built.
  - Fallback: the pump `unix.Poll`s `[pty, wake pipe]` and Pause writes to the pipe.
- **`syscall.Exec` from a multithreaded Go process on darwin.** Task 2.2's `TestATerminalSurvivesAnExec` execs the test binary into itself with a live PTY, which retires the risk in B2.
- **Sparkle or SMAppService replacing the bundle under a running agent.** If launchd kills the job when its `BundleProgram` vanishes, the in-place upgrade never runs: the result is a normal restart, and terminals are lost. This needs a manual test on a build from the release plan's PR 3. Nothing in this plan can detect it.
- **Codex approvals carrying over is inferred from source, not tested.** The claim is that `thread/resume` replays pending approvals after a disconnect. The e2e covers Claude only; fakecodex doesn't model the replay.
- **Writers block during the gap.** Output waits in the kernel PTY buffer, a few KB. Writers block when it fills: nothing is lost, but a chatty build stalls for the length of the gap.
- **Processes started during the handoff inherit every master**, until the dry run ends: the dry run itself, git, and the shellenv capture. A Terminal spawned during the handoff window isn't handed down, and the exec closes its master.
- **Clients and in-flight state that drop at exec:**
  - `pocketd run` and `pocketd attach` clients see EOF and exit. Their Terminals survive.
  - `pocketd hook exit` has no retry, so a setup's exit report during the gap is lost.
  - The Launcher's `pending` and `found` maps are not carried.
- **Non-permission hooks can be sent twice** if the socket drops after pocketd acted but before it answered. They are idempotent: status and transcript.
- **The VT snapshot redraws the screen only, not scrollback.** The desktop reattaches and gets that snapshot, as on any reconnect.
- **The login env is recaptured in the background after exec.** A spawn in the first second gets pocketd's own env.
- **The IOKit assertion dies with the old image.** The kick after Adopt makes the keeper take it again.
- **The `upgradeFailed` toast shows again after a desktop restart**, for as long as the old pocketd keeps running.
- **A handoff file the new image can't read** makes `serve` return an error, which exits pocketd. launchd restarts it, it runs the normal restore, and the reap kills the orphaned shells. The dry run ran the same binary on the same file, so this needs a disk fault.
- **The state writer can be cut off mid-write by the exec.** `atomicfile` leaves at worst a temp file.

---

## PR B1: The hook retries across a dropped socket

**Scope:** `pocketd hook` redials and resends until pocketd answers or 10s pass. Nothing else changes, and nothing drops the socket yet.
**Depends on:** nothing
**Done when:** `go test ./cmd/pocketd` passes, and `go test ./e2e -run 'Approve|Reconnecting'` still passes.

### Task 1.1: Resend a hook whose socket drops

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/hook.go`
- Create: `packages/pocketd/cmd/pocketd/hook_test.go`

**Context:**
- Today `hook` dials once, sends once and reads once, and prints nothing on any failure, so Claude goes on as if there were no hook.
- During an upgrade, the exec closes the hook's connection mid-wait. The new image asks the user again only if the hook resends.
- An `error` reply means pocketd heard the hook (for example, a refused peer), so resending it is pointless.
- Unix socket paths are capped at 104 bytes on macOS, so tests use `/tmp`.

**Step 1: Write the failing tests**

`cmd/pocketd/hook_test.go`:
```go
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
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'Hook' -count=1`
Expected: FAIL. Build error `undefined: relay`.

**Step 3: Implement**

Replace `cmd/pocketd/hook.go`:
```go
package main

import (
	"io"
	"os"
	"time"

	"pocketd/internal/ops"
)

// hookRetry is how long a hook keeps redialing. An upgrading pocketd drops
// the socket for a moment, and its successor asks the user again.
var hookRetry = 10 * time.Second

// hook runs for every Claude Code hook in pocketd's plugin. Printing nothing
// lets Claude go on as if there were no hook, so every failure falls back to
// that. pocketd finds the claude it came from by this process's ancestry.
func hook(sock string) error {
	payload, err := io.ReadAll(os.Stdin)
	pty := os.Getenv("POCKETD_PTY")
	if err != nil || pty == "" {
		return nil
	}
	os.Stdout.Write(relay(sock, pty, payload, hookRetry))
	return nil
}

// relay sends the hook until pocketd answers or wait has passed.
func relay(sock, pty string, payload []byte, wait time.Duration) []byte {
	for deadline := time.Now().Add(wait); ; time.Sleep(100 * time.Millisecond) {
		if out, ok := ask(sock, pty, payload); ok || time.Now().After(deadline) {
			return out
		}
	}
}

// ask sends the hook once. ok means pocketd answered, even with an error:
// only a socket that drops is worth resending to.
func ask(sock, pty string, payload []byte) (out []byte, ok bool) {
	c, err := ops.Dial(sock)
	if err != nil {
		return nil, false
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "hook", ID: pty, Data: payload}) != nil {
		return nil, false
	}
	m, err := c.Recv()
	if err != nil {
		return nil, false
	}
	if m.Ev == "hook" {
		return m.Data, true
	}
	return nil, true
}
```

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'Hook' -count=1 && go test ./e2e -run 'TestPhoneApprovesTool|TestReconnectingPhoneSeesOpenRequest' -count=1`
Expected: PASS.

---

## PR B2: A Terminal can pause, hand its pty over and be adopted

**Scope:** The terminal package only:
- the pty master becomes a pollable non-blocking fd;
- Pause, Resume, Handoff and `Manager.Adopt` are added;
- Resize stops calling `Fd()`;
- the process is held as `*os.Process`.

No caller uses the new API yet, and behaviour is otherwise unchanged.
**Depends on:** nothing
**Done when:** `go test -race ./internal/terminal ./internal/daemon ./e2e` passes.

### Task 2.1: A pollable pty that pauses

**Files:**
- Modify: `packages/pocketd/internal/terminal/terminal.go:3-21` (imports), `:59-71` (Terminal), `:116-160` (Spawn), `:206-229` (pump), `:255` (Pid), `:278-283` (Attach), `:318-328` (Resize)
- Test: `packages/pocketd/internal/terminal/terminal_test.go`

**Context:**
- `creack/pty` opens `/dev/ptmx` blocking, so `os.NewFile` doesn't register it with the poller, and `SetReadDeadline` fails with `os.ErrNoDeadline`. Dup'ing it with `F_DUPFD_CLOEXEC` (atomic, so no concurrent fork inherits it), making it non-blocking and wrapping it again gives a poller-owned file.
- `proc.Foreground` already avoids `Fd()` for this reason (`proc_darwin.go:22`). `pty.Setsize` calls `Fd()`, so Resize gets its own ioctl through `SyscallConn`.
- `exec.Cmd.Wait` becomes `os.Process.Wait`. An adopted Terminal has only a pid, and `os.FindProcess` gives a waitable `*os.Process` for our own child.
- `pty.StartWithSize` already closes the slave, and sets no stdio copy goroutines, so dropping `cmd` loses nothing.

**Step 1: Write the failing tests**

Append to `terminal_test.go`:
```go
func TestAPausedTerminalKeepsItsOutputUntilResumed(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; echo "got:$x"; sleep 5`)
	waitScreen(t, s, "ready")
	if err := s.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	s.Write([]byte("hi\r"))
	time.Sleep(300 * time.Millisecond)
	if strings.Contains(s.Screen(), "got:hi") {
		t.Fatal("a paused terminal read the pty")
	}
	s.Resume()
	waitScreen(t, s, "got:hi")
}

func TestAResizedTerminalCanStillPause(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; stty size; sleep 5`)
	waitScreen(t, s, "ready")
	s.Resize(60, 10)
	s.Write([]byte("\r"))
	waitScreen(t, s, "10 60")
	if err := s.Pause(500 * time.Millisecond); err != nil {
		t.Fatalf("pause after a resize: %v", err)
	}
	s.Resume()
}

func TestPausingAnExitedTerminalSaysItIsClosed(t *testing.T) {
	s := spawn(t, NewManager(), `exit 0`)
	<-s.Done()
	if err := s.Pause(time.Second); !errors.Is(err, ErrClosed) {
		t.Fatalf("err = %v", err)
	}
}
```
Add `"errors"` to the test imports.

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/terminal -run 'Pause' -count=1`
Expected: FAIL. Build error `s.Pause undefined`.

**Step 3: Implement**

Imports: add `"cmp"` and `"golang.org/x/sys/unix"`.

Add after the `Event` type:
```go
var ErrClosed = errors.New("terminal closed")
```

`Terminal` replaces `cmd *exec.Cmd` with `proc *os.Process` and gains two fields:
```go
type Terminal struct {
	info   Info
	mu     sync.Mutex
	pty    *os.File
	proc   *os.Process
	vt     *vt.VT
	subs   map[*subscriber]bool
	done   chan struct{}
	code   int
	closed bool
	input  func(id string, b []byte)
	origin string
	resume chan struct{} // closed by Resume; nil unless paused
	parked chan struct{} // closed once the pump stops reading for this pause
}
```

In `Spawn`, from `pty.StartWithSize` to the end:
```go
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(spec.Cols), Rows: uint16(spec.Rows)})
	if err != nil {
		return nil, err
	}
	if f, err = pollable(f); err != nil {
		cmd.Process.Kill()
		cmd.Wait()
		return nil, err
	}
	s := &Terminal{
		info:   Info{ID: spec.ID, Cmd: spec.Cmd, Args: spec.Args, Cwd: spec.Cwd, Cols: spec.Cols, Rows: spec.Rows},
		pty:    f,
		proc:   cmd.Process,
		subs:   map[*subscriber]bool{},
		done:   make(chan struct{}),
		input:  m.OnInput,
		origin: spec.Origin,
	}
	s.vt, err = vt.New(spec.Cols, spec.Rows, s.replyToQuery)
	if err != nil {
		cmd.Process.Kill()
		f.Close()
		return nil, err
	}
	m.start(s)
	return s, nil
}

func (m *Manager) start(s *Terminal) {
	m.mu.Lock()
	m.terminals[s.info.ID] = s
	m.mu.Unlock()
	go s.pump(func() {
		m.mu.Lock()
		delete(m.terminals, s.info.ID)
		m.mu.Unlock()
	})
}

// pollable moves f to a non-blocking fd the runtime poller owns, so a read
// deadline can stop the pump. creack/pty's file blocks a thread instead.
func pollable(f *os.File) (*os.File, error) {
	defer f.Close()
	fd, err := unix.FcntlInt(f.Fd(), unix.F_DUPFD_CLOEXEC, 0)
	if err != nil {
		return nil, err
	}
	if err := unix.SetNonblock(fd, true); err != nil {
		unix.Close(fd)
		return nil, err
	}
	return os.NewFile(uintptr(fd), f.Name()), nil
}

// setsize is pty.Setsize without Fd, which would make f blocking again.
func setsize(f *os.File, cols, rows int) error {
	rc, err := f.SyscallConn()
	if err != nil {
		return err
	}
	var ioErr error
	err = rc.Control(func(fd uintptr) {
		ioErr = unix.IoctlSetWinsize(int(fd), unix.TIOCSWINSZ, &unix.Winsize{Col: uint16(cols), Row: uint16(rows)})
	})
	return cmp.Or(err, ioErr)
}
```

`pump`:
```go
func (s *Terminal) pump(onExit func()) {
	buf := make([]byte, 32*1024)
	for {
		n, err := s.pty.Read(buf)
		if n > 0 {
			chunk := append([]byte(nil), buf[:n]...)
			s.mu.Lock()
			s.vt.Write(chunk)
			s.broadcast(Event{Kind: "output", Data: chunk})
			s.mu.Unlock()
		}
		if errors.Is(err, os.ErrDeadlineExceeded) {
			s.park()
			continue
		}
		if err != nil {
			break
		}
	}
	code := -1
	if st, err := s.proc.Wait(); err == nil {
		code = st.ExitCode()
	}
	onExit()
	s.mu.Lock()
	s.closed = true
	s.code = code
	s.broadcast(Event{Kind: "exit", Code: s.code})
	s.vt.Free()
	s.pty.Close()
	s.mu.Unlock()
	close(s.done)
}

// park holds the pump while paused. A Resume that beat the deadline leaves
// nothing to wait for.
func (s *Terminal) park() {
	s.mu.Lock()
	parked, resume := s.parked, s.resume
	s.mu.Unlock()
	if resume == nil {
		return
	}
	close(parked)
	<-resume
}

// Pause stops reading the pty, so output waits in the kernel, and returns
// once the pump has stopped or wait has passed.
func (s *Terminal) Pause(wait time.Duration) error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return ErrClosed
	}
	if s.resume == nil {
		if err := s.pty.SetReadDeadline(time.Now()); err != nil {
			s.mu.Unlock()
			return err
		}
		s.resume, s.parked = make(chan struct{}), make(chan struct{})
	}
	parked := s.parked
	s.mu.Unlock()
	select {
	case <-parked:
		return nil
	case <-s.done:
		return ErrClosed
	case <-time.After(wait):
		return fmt.Errorf("terminal %s: reader didn't pause", s.info.ID)
	}
}

// Resume reads the pty again and keeps it from surviving an exec.
func (s *Terminal) Resume() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.resume == nil {
		return
	}
	inherit(s.pty, false)
	s.pty.SetReadDeadline(time.Time{})
	close(s.resume)
	s.resume, s.parked = nil, nil
}

// inherit sets whether f survives exec, and returns its fd. Fd would make f blocking again.
func inherit(f *os.File, on bool) (int, error) {
	rc, err := f.SyscallConn()
	if err != nil {
		return 0, err
	}
	var fd int
	var ioErr error
	err = rc.Control(func(p uintptr) {
		fd = int(p)
		flags := unix.FD_CLOEXEC
		if on {
			flags = 0
		}
		_, ioErr = unix.FcntlInt(p, unix.F_SETFD, flags)
	})
	return fd, cmp.Or(err, ioErr)
}
```
`Pause` sets the deadline before it makes the channels, so a failed `SetReadDeadline` leaves nothing half-paused.

`Pid`:
```go
func (s *Terminal) Pid() int { return s.proc.Pid }
```
`Attach`: `return nil, nil, errors.New("terminal closed")` becomes `return nil, nil, ErrClosed`.
`Resize`: `pty.Setsize(s.pty, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})` becomes `setsize(s.pty, cols, rows)`.

**Step 4: Verify**

Run: `cd packages/pocketd && go test -race ./internal/terminal -count=1`
Expected: PASS. If `Pause` returns `deadline not supported`, kqueue refuses the PTY. Stop here and take the fallback in Notes for the implementer.

### Task 2.2: Hand a Terminal across an exec and adopt it

**Files:**
- Modify: `packages/pocketd/internal/terminal/terminal.go` (after `Resume`, and after `Spawn`)
- Test: `packages/pocketd/internal/terminal/terminal_test.go`

**Context:**
- The real exec is the riskiest step, so the test does one. `TestATerminalSurvivesAnExec` runs this test binary as a helper that:
  1. spawns a shell, pauses it and hands it over;
  2. types into it;
  3. `syscall.Exec`s itself.
- The second image adopts the fd and pid, and must show the old screen and the output written during the gap. It must also reap the shell's exit code, which only works if the shell is still its child.
- The env is filtered before the exec because darwin's `getenv` keeps the first duplicate.

**Step 1: Write the failing tests**

Append to `terminal_test.go` (imports: add `"encoding/base64"`, `"fmt"`, `"os/exec"`):
```go
func TestHandoffNeedsAPausedTerminal(t *testing.T) {
	s := spawn(t, NewManager(), `sleep 5`)
	if _, err := s.Handoff(); err == nil {
		t.Fatal("handed over a terminal that is still reading")
	}
}

func TestATerminalSurvivesAnExec(t *testing.T) {
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command(exe, "-test.run=^TestHandoffHelper$")
	cmd.Env = append(os.Environ(), "POCKETD_HANDOFF=before")
	out, err := cmd.CombinedOutput()
	if err != nil || !strings.Contains(string(out), "ADOPTED exit=7") {
		t.Fatalf("%v\n%s", err, out)
	}
}

// TestHandoffHelper is both images of TestATerminalSurvivesAnExec.
func TestHandoffHelper(t *testing.T) {
	switch os.Getenv("POCKETD_HANDOFF") {
	case "before":
		handBefore(t)
	case "after":
		handAfter(t)
	default:
		t.Skip("runs only under TestATerminalSurvivesAnExec")
	}
}

func handBefore(t *testing.T) {
	s, err := NewManager().Spawn(Spec{ID: "t-1", Cmd: "sh", Args: []string{"-c", `echo before; read x; sleep 0.3; echo "after $x"; sleep 1; exit 7`}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "before")
	if err := s.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	h, err := s.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	s.Write([]byte("go\r"))
	exe, _ := os.Executable()
	env := slices.DeleteFunc(os.Environ(), func(kv string) bool { return strings.HasPrefix(kv, "POCKETD_HANDOFF") })
	env = append(env, "POCKETD_HANDOFF=after", "POCKETD_HANDOFF_FD="+strconv.Itoa(h.FD), "POCKETD_HANDOFF_PID="+strconv.Itoa(h.Pid),
		"POCKETD_HANDOFF_SCREEN="+base64.StdEncoding.EncodeToString(h.Screen))
	t.Fatal(syscall.Exec(exe, []string{exe, "-test.run=^TestHandoffHelper$"}, env))
}

func handAfter(t *testing.T) {
	fd, _ := strconv.Atoi(os.Getenv("POCKETD_HANDOFF_FD"))
	pid, _ := strconv.Atoi(os.Getenv("POCKETD_HANDOFF_PID"))
	screen, _ := base64.StdEncoding.DecodeString(os.Getenv("POCKETD_HANDOFF_SCREEN"))
	s, err := NewManager().Adopt(Adopted{ID: "t-1", Cmd: "sh", Cols: 40, Rows: 5, FD: fd, Pid: pid, Screen: screen})
	if err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "after go")
	if !strings.Contains(s.Screen(), "before") {
		t.Fatalf("the old screen is gone:\n%s", s.Screen())
	}
	fmt.Printf("ADOPTED exit=%d\n", s.ExitCode())
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/terminal -run 'Handoff|SurvivesAnExec' -count=1`
Expected: FAIL. Build error `s.Handoff undefined`.

**Step 3: Implement**

After `Resume`:
```go
// Handed is what the next image needs to adopt a Terminal.
type Handed struct {
	FD, Pid int
	Screen  []byte
}

// Handoff readies a paused Terminal for exec: its pty survives it, and
// Screen redraws what it shows.
func (s *Terminal) Handoff() (Handed, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return Handed{}, ErrClosed
	}
	if s.resume == nil {
		return Handed{}, fmt.Errorf("terminal %s: handoff needs a paused terminal", s.info.ID)
	}
	fd, err := inherit(s.pty, true)
	if err != nil {
		return Handed{}, err
	}
	return Handed{FD: fd, Pid: s.proc.Pid, Screen: s.vt.Snapshot()}, nil
}
```

After `Manager.start`:
```go
// Adopted is a Terminal the image before exec handed down.
type Adopted struct {
	ID, Cmd             string
	Args                []string
	Cwd                 string
	Cols, Rows, FD, Pid int
	Screen              []byte
}

// Adopt takes over a handed-down Terminal. Its fd is still non-blocking,
// so the new file is pollable too.
func (m *Manager) Adopt(a Adopted) (*Terminal, error) {
	syscall.CloseOnExec(a.FD)
	f := os.NewFile(uintptr(a.FD), "/dev/ptmx")
	p, _ := os.FindProcess(a.Pid) // never fails on unix
	s := &Terminal{
		info:  Info{ID: a.ID, Cmd: a.Cmd, Args: a.Args, Cwd: a.Cwd, Cols: a.Cols, Rows: a.Rows},
		pty:   f,
		proc:  p,
		subs:  map[*subscriber]bool{},
		done:  make(chan struct{}),
		input: m.OnInput,
	}
	var err error
	if s.vt, err = vt.New(a.Cols, a.Rows, s.replyToQuery); err != nil {
		f.Close()
		return nil, err
	}
	s.vt.Write(a.Screen)
	m.start(s)
	return s, nil
}
```

**Step 4: Verify**

Run: `cd packages/pocketd && go test -race ./internal/terminal ./internal/daemon -count=1 && go test ./e2e -count=1`
Expected: PASS. The helper shows as SKIP in normal runs.

---

## PR B3: The handoff file and its dry run

**Scope:** The `handoff` package and the `pocketd handoff --check <file>` command. Nothing writes a handoff yet.
**Depends on:** B2 (the dry-run test hands over a real Terminal)
**Done when:** `go test ./internal/handoff ./cmd/pocketd` passes.

### Task 3.1: The handoff package

**Files:**
- Create: `packages/pocketd/internal/handoff/handoff.go`, `packages/pocketd/internal/handoff/handoff_test.go`, `packages/pocketd/internal/handoff/testdata/format1.json`

**Context:**
- `Saved` reuses `state.Terminal`, which already carries what a restore needs (agent id, conversation, transcript path, launch).
- `Agent` is what an Agent keeps beyond that.
- `Check` runs in the dry-run child, which inherited the fds because `Handoff` cleared close-on-exec. `F_GETFD` proves each fd is open and will survive the real exec.

**Step 1: Write the failing tests**

`internal/handoff/handoff_test.go`:
```go
package handoff

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"golang.org/x/sys/unix"

	"pocketd/internal/state"
)

func TestAFileReadsBackAsWritten(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	f := File{Format: Format, Terminals: []Terminal{{
		Saved: state.Terminal{TerminalID: "t-1", LaunchDir: "/w", Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1", AgentID: "a-1"},
		Cmd:   "zsh", Args: []string{"-l"}, Cwd: "/w", FD: 7, Pid: 42, Screen: []byte("\x1b[Hhi"),
		Agent: &Agent{Named: "Fix login", Phase: "working", Epoch: 2, Seq: 9},
	}}}
	if err := Write(path, f); err != nil {
		t.Fatal(err)
	}
	got, err := Read(path)
	if err != nil || !reflect.DeepEqual(got, f) {
		t.Fatalf("read %+v, %v", got, err)
	}
}

func TestTheFirstReleasesFileStillReads(t *testing.T) {
	f, err := Read("testdata/format1.json")
	if err != nil {
		t.Fatal(err)
	}
	term := f.Terminals[0]
	if term.Saved.TerminalID != "t-1" || term.Saved.AgentID != "a-1" || term.FD != 7 || term.Pid != 42 ||
		string(term.Screen) != "hi" || term.Agent.Named != "Fix login" || term.Agent.Seq != 9 {
		t.Fatalf("format 1 = %+v", f)
	}
}

func TestANewerFormatIsRefused(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	os.WriteFile(path, []byte(`{"format":99,"terminals":[]}`), 0o600)
	if _, err := Read(path); err == nil || !strings.Contains(err.Error(), "format 99") {
		t.Fatalf("err = %v", err)
	}
}

func TestCheckRefusesAClosedFD(t *testing.T) {
	err := Check(File{Format: Format, Terminals: []Terminal{{Saved: state.Terminal{TerminalID: "t-1"}, FD: 65000}}})
	if err == nil || !strings.Contains(err.Error(), "t-1") {
		t.Fatalf("err = %v", err)
	}
}

func TestCheckRefusesAnFDThatClosesOnExec(t *testing.T) {
	devnull, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatal(err)
	}
	defer devnull.Close()
	f := File{Format: Format, Terminals: []Terminal{{Saved: state.Terminal{TerminalID: "t-1"}, FD: int(devnull.Fd())}}}
	if Check(f) == nil {
		t.Fatal("an fd that closes on exec passed")
	}
	unix.FcntlInt(devnull.Fd(), unix.F_SETFD, 0)
	if err := Check(f); err != nil {
		t.Fatal(err)
	}
}
```

`internal/handoff/testdata/format1.json` is the first release's file. Never edit it; a new format adds `format2.json`.
```json
{"format":1,"terminals":[{"saved":{"terminalId":"t-1","launchDir":"/w","cols":80,"rows":24,"provider":"claude","conversationId":"c1","agentId":"a-1","createdAt":7,"status":"working"},"cmd":"zsh","args":["-l"],"cwd":"/w","fd":7,"pid":42,"screen":"aGk=","agent":{"named":"Fix login","phase":"working","epoch":2,"seq":9}}]}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/handoff -count=1`
Expected: FAIL. Build error `undefined: File`.

**Step 3: Implement**

`internal/handoff/handoff.go`:
```go
// Package handoff is the file one pocketd leaves for the binary it execs
// into: its live Terminals, their open pty fds, and their Agents.
package handoff

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"golang.org/x/sys/unix"

	"pocketd/internal/atomicfile"
	"pocketd/internal/state"
)

// Format is the format this release writes; MinFormat is the oldest it
// reads. A release must read the file the release before it wrote.
const (
	Format    = 1
	MinFormat = 1
)

type File struct {
	Format    int        `json:"format"`
	Terminals []Terminal `json:"terminals"`
}

type Terminal struct {
	Saved  state.Terminal `json:"saved"`
	Cmd    string         `json:"cmd"`
	Args   []string       `json:"args,omitempty"`
	Cwd    string         `json:"cwd"`
	FD     int            `json:"fd"`
	Pid    int            `json:"pid"`
	Screen []byte         `json:"screen"`
	Agent  *Agent         `json:"agent,omitempty"`
}

// Agent is what an Agent keeps across the exec beyond its saved entry.
type Agent struct {
	Named string `json:"named,omitempty"`
	Phase string `json:"phase"`
	Epoch int64  `json:"epoch"`
	Seq   int64  `json:"seq"`
}

func Path(home string) string { return filepath.Join(home, "handoff.json") }

func Write(path string, f File) error {
	raw, err := json.Marshal(f)
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}

func Read(path string) (File, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return File{}, err
	}
	var f File
	if err := json.Unmarshal(raw, &f); err != nil {
		return File{}, err
	}
	if f.Format < MinFormat || f.Format > Format {
		return File{}, fmt.Errorf("handoff format %d; this pocketd reads %d to %d", f.Format, MinFormat, Format)
	}
	return f, nil
}

// Check is the dry run's test: every fd in f is open here and survives exec.
func Check(f File) error {
	for _, t := range f.Terminals {
		flags, err := unix.FcntlInt(uintptr(t.FD), unix.F_GETFD, 0)
		if err != nil {
			return fmt.Errorf("terminal %s: fd %d: %w", t.Saved.TerminalID, t.FD, err)
		}
		if flags&unix.FD_CLOEXEC != 0 {
			return fmt.Errorf("terminal %s: fd %d closes on exec", t.Saved.TerminalID, t.FD)
		}
	}
	return nil
}
```

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./internal/handoff -count=1`
Expected: PASS.

### Task 3.2: `pocketd handoff --check`

**Files:**
- Create: `packages/pocketd/cmd/pocketd/handoff.go`, `packages/pocketd/cmd/pocketd/handoff_test.go`
- Modify: `packages/pocketd/cmd/pocketd/main.go:30-35`

**Context:** The old image runs the new binary with this command before it execs. The check reads the file with the new binary's code, checks every fd, and replays every screen into a VT, which is the step that runs ghostty's parser on old output.

**Step 1: Write the failing tests**

`cmd/pocketd/handoff_test.go`:
```go
package main

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestTheDryRunRefusesAFileItCantRead(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	os.WriteFile(path, []byte(`{"format":99,"terminals":[]}`), 0o600)
	if err := handoffCheck(path); err == nil {
		t.Fatal("a newer format passed the dry run")
	}
}

func TestTheDryRunAcceptsAPausedTerminalsFile(t *testing.T) {
	term, err := terminal.NewManager().Spawn(terminal.Spec{Cmd: "sh", Args: []string{"-c", "echo hi; sleep 5"}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	if err := term.Pause(time.Second); err != nil {
		t.Fatal(err)
	}
	defer term.Resume()
	h, err := term.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "handoff.json")
	f := handoff.File{Format: handoff.Format, Terminals: []handoff.Terminal{{Saved: state.Terminal{TerminalID: "t-1", Cols: 40, Rows: 5}, FD: h.FD, Pid: h.Pid, Screen: h.Screen}}}
	if err := handoff.Write(path, f); err != nil {
		t.Fatal(err)
	}
	if err := handoffCheck(path); err != nil {
		t.Fatal(err)
	}
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'DryRun' -count=1`
Expected: FAIL. Build error `undefined: handoffCheck`.

**Step 3: Implement**

`cmd/pocketd/handoff.go`:
```go
package main

import (
	"pocketd/internal/handoff"
	"pocketd/internal/vt"
)

// handoffCheck is the dry run an upgrading pocketd runs with the new binary:
// the file reads, every fd in it came through, and every screen replays.
func handoffCheck(path string) error {
	f, err := handoff.Read(path)
	if err != nil {
		return err
	}
	if err := handoff.Check(f); err != nil {
		return err
	}
	for _, t := range f.Terminals {
		v, err := vt.New(t.Saved.Cols, t.Saved.Rows, func([]byte) {})
		if err != nil {
			return err
		}
		v.Write(t.Screen)
		v.Free()
	}
	return nil
}
```

`main.go`: add before `case os.Args[1] == "serve":`
```go
	case len(os.Args) == 4 && os.Args[1] == "handoff" && os.Args[2] == "--check":
		err = handoffCheck(os.Args[3])
```
It stays out of `usage`, because only pocketd runs it.

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'DryRun' -count=1 && go build -o /tmp/pk-check ./cmd/pocketd && /tmp/pk-check handoff --check /nonexistent; echo $?`
Expected: PASS, then `pocketd: open /nonexistent: no such file or directory` and `1`.

---

## PR B4: pocketd upgrades in place

**Scope:**
- the `pocketd upgrade` CLI and `ops:upgrade`;
- the upgrader and `serve --handoff`;
- Daemon Handoff/Adopt;
- carried Agent state;
- `HostState.UpgradeFailed`.

No automatic trigger yet.
**Depends on:** B1, B2, B3
**Done when:** `go test ./e2e -run Upgrade -count=1` passes, and so does `go vet ./... && go test -race -count=1 ./...`.

### Task 4.1: e2e tests (fail until 4.5)

**Files:**
- Modify: `packages/pocketd/e2e/harness_test.go:47-58` (Harness), `:121-133` (serve), plus new helpers
- Modify: `packages/pocketd/e2e/phone_test.go:20-45` (Message)
- Create: `packages/pocketd/e2e/upgrade_test.go`

**Context:**
- Each test upgrades to the same binary it runs, which `pocketd upgrade` always does.
- `ownBinary` serves from a copy in `h.Home`, so a test can replace it. The replacement is a rename, not an overwrite, as Sparkle does: on macOS, overwriting a running binary in place gets it killed.
- The counter shell prints `n=1, n=2, …` every 200ms. Consecutive counts on one screen that span the upgrade prove that nothing was lost during the gap.

**Step 1: Write the failing tests**

`harness_test.go`: add a field `exe string` to `Harness` (after `exited`). In `serve`, change:
```go
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), "serve")
```
to
```go
	cmd := exec.Command(cmp.Or(h.exe, filepath.Join(binDir, "pocketd")), "serve")
```
(import `"cmp"`). Append:
```go
// ownBinary serves pocketd from a copy in h.Home, which a test may replace.
func ownBinary(h *Harness) {
	h.exe = filepath.Join(h.Home, "bin", "pocketd")
	copyFile(h.t, filepath.Join(binDir, "pocketd"), h.exe)
}

// replaceBinary swaps h.exe for src by rename, as Sparkle does. Writing over
// a running binary gets it killed on macOS.
func (h *Harness) replaceBinary(src string) {
	h.t.Helper()
	copyFile(h.t, src, h.exe+".new")
	if err := os.Rename(h.exe+".new", h.exe); err != nil {
		h.t.Fatal(err)
	}
}

func copyFile(t *testing.T, src, dst string) {
	t.Helper()
	b, err := os.ReadFile(src)
	if err == nil {
		err = os.MkdirAll(filepath.Dir(dst), 0o755)
	}
	if err == nil {
		err = os.WriteFile(dst, b, 0o755)
	}
	if err != nil {
		t.Fatal(err)
	}
}

// Upgrade runs `pocketd upgrade` and fails the test unless it succeeds.
func (h *Harness) Upgrade() {
	h.t.Helper()
	if out, code := h.Pocketd("upgrade"); code != 0 {
		h.t.Fatalf("upgrade exited %d: %s", code, out)
	}
}

// Status is pocketd's status, or the zero Status while it doesn't answer.
func (h *Harness) Status() ops.Status {
	c, err := ops.Dial(h.Sock)
	if err != nil {
		return ops.Status{}
	}
	defer c.Close()
	c.Send(ops.Msg{Op: "status"})
	m, err := c.Recv()
	if err != nil || m.Status == nil {
		return ops.Status{}
	}
	return *m.Status
}
```

`phone_test.go`, `Message`: in `Agents []struct{…}` add `Restore string \`json:"restore"\``, and add a field:
```go
	Host struct {
		UpgradeFailed string `json:"upgradeFailed"`
	} `json:"host"`
```

`e2e/upgrade_test.go`:
```go
package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"syscall"
	"testing"

	"pocketd/internal/ops"
)

// counting spawns a shell that prints n=1, n=2, … every 200ms and returns
// its terminal and pid.
func counting(h *Harness) (string, int) {
	h.t.Helper()
	f := filepath.Join(h.Home, "counter.pid")
	id := h.Spawn("sh", "-c", `echo $$ > `+f+`; i=0; while :; do i=$((i+1)); echo n=$i; sleep 0.2; done`)
	var pid int
	h.eventually("the counter's pid", func() bool {
		b, _ := os.ReadFile(f)
		pid, _ = strconv.Atoi(strings.TrimSpace(string(b)))
		return pid > 0
	})
	return id, pid
}

var countLine = regexp.MustCompile(`(?m)^n=(\d+)$`)

// countsPast waits until id's screen counts past n and returns every count it shows.
func (h *Harness) countsPast(id string, n int) []int {
	h.t.Helper()
	var got []int
	h.eventually(fmt.Sprintf("a count past %d", n), func() bool {
		got = nil
		for _, m := range countLine.FindAllStringSubmatch(h.Screen(id), -1) {
			c, _ := strconv.Atoi(m[1])
			got = append(got, c)
		}
		return len(got) > 0 && got[len(got)-1] > n
	})
	return got
}

func TestAnUpgradeKeepsTheShellRunning(t *testing.T) {
	h := Start(t)
	id, shell := counting(h)
	before := h.countsPast(id, 3)
	last := before[len(before)-1]
	pid := h.Status().PID

	h.Upgrade()

	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	if m, err := c.Recv(); err != nil || len(m.Items) != 1 || m.Items[0].ID != id || m.Items[0].Cmd != "sh" {
		t.Fatalf("list = %+v, %v", m, err)
	}
	after := h.countsPast(id, last+5)
	if after[0] > last {
		t.Fatalf("the screen before the upgrade is gone: %v, last was %d", after, last)
	}
	for i := 1; i < len(after); i++ {
		if after[i] != after[i-1]+1 {
			t.Fatalf("output was lost: %v", after)
		}
	}
}

func TestAnUpgradeKeepsTheAgentAndReasksItsOpenPermission(t *testing.T) {
	h, phone, term, id := StartClaude(t)
	h.Prompt(term, "run ls")
	asked := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })

	h.Upgrade()

	again := h.Phone()
	req := again.WaitFor("the request asked again", func(m Message) bool {
		return m.Type == "permission.request" && m.Request.AgentID == id && m.Request.RequestID != asked.Request.RequestID
	})
	again.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	again.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "r1" })
	h.WaitScreen(term, "hook: allow")
	again.Send(map[string]any{"type": "agent.list", "id": "l"})
	list := again.WaitFor("agent.list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "l" })
	if len(list.Agents) != 1 || list.Agents[0].ID != id || list.Agents[0].Restore != "" {
		t.Fatalf("agents after the upgrade: %s", list.Raw)
	}
}

func TestAFailedDryRunKeepsTheOldPocketd(t *testing.T) {
	h := Start(t, ownBinary)
	owner := h.Owner("host.v1")
	id, shell := counting(h)
	last := h.countsPast(id, 0)
	pid := h.Status().PID
	bad := filepath.Join(t.TempDir(), "pocketd")
	os.WriteFile(bad, []byte("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocketd v-bad' && exit 0\nexit 1\n"), 0o755)
	h.replaceBinary(bad)

	out, code := h.Pocketd("upgrade")

	if code == 0 || !strings.Contains(out, "check") {
		t.Fatalf("upgrade exited %d: %s", code, out)
	}
	owner.WaitFor("upgradeFailed", func(m Message) bool { return m.Type == "host.changed" && m.Host.UpgradeFailed == "v-bad" })
	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	h.countsPast(id, last[len(last)-1]+5)
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./e2e -run 'Upgrade|FailedDryRun' -count=1`
Expected: FAIL. `upgrade exited 2: usage: pocketd serve | …`

### Task 4.2: Carry an Agent's name, phase and seq

**Files:**
- Modify: `packages/pocketd/internal/timeline/timeline.go` (after `Clear`, line 65)
- Modify: `packages/pocketd/internal/agent/agent.go:109-126` (Restored, Registry.Restore), and add `Named` near `ID()` (line 191)
- Test: `packages/pocketd/internal/timeline/timeline_test.go`, `packages/pocketd/internal/agent/agent_test.go`

**Context:**
- After the exec, the new image's Timeline is empty and a Claude transcript is tailed again from the start.
- Moving to epoch+1 makes every phone refetch, and keeping `seq` means no phone's `sinceSeq` matches a replayed item. This is the same rule `Clear` follows.

**Step 1: Write the failing tests**

`timeline_test.go`:
```go
func TestContinueStartsANewEpochPastTheOldSeq(t *testing.T) {
	tl := New()
	tl.Continue(3, 12)
	if epoch, seq := tl.State(); epoch != 4 || seq != 12 {
		t.Fatalf("state = %d, %d", epoch, seq)
	}
	it, _ := tl.Apply(Event{Kind: "user", Text: "hi"}, 1)
	if it.Seq != 13 || it.ID != "i13" {
		t.Fatalf("first item after continue: %+v", it)
	}
}
```
`agent_test.go`:
```go
func TestARestoredAgentContinuesItsNamePhaseAndSeq(t *testing.T) {
	r := NewRegistry(hub.New())
	a := r.Restore(Restored{ID: "a-1", Provider: "claude", Named: "Fix login", Phase: "needsYou", Epoch: 3, Seq: 12}, fakeDriver{})
	s := a.Summary()
	if s.Title != "Fix login" || s.Status != "needsYou" || s.Epoch != 4 || s.MaxSeq != 12 || a.Named() != "Fix login" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestARestoredAgentWithoutAPhaseIsIdle(t *testing.T) {
	a := NewRegistry(hub.New()).Restore(Restored{ID: "a-1", Provider: "claude"}, fakeDriver{})
	if s := a.Summary(); s.Status != "idle" || s.MaxSeq != 0 {
		t.Fatalf("summary = %+v", s)
	}
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/timeline ./internal/agent -run 'Continue|Restored' -count=1`
Expected: FAIL. Build errors `tl.Continue undefined` and `unknown field Named in struct literal`.

**Step 3: Implement**

`timeline.go`, after `Clear`:
```go
// Continue picks up a timeline from the image before an upgrade: a new
// epoch so phones refetch, and seq past every item they hold.
func (t *Timeline) Continue(epoch, seq int64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.epoch, t.seq = epoch+1, seq
}
```
`agent.go`, `Restored` gains:
```go
	// Named, Phase, Epoch and Seq carry an Agent across an upgrade's exec.
	Named, Phase string
	Epoch, Seq   int64
```
`Registry.Restore`: in the `&Agent{…}` literal, `phase: "idle"` becomes `phase: cmp.Or(x.Phase, "idle")` and `named: x.Named` is added. Then, before `r.mu.Lock()`:
```go
	if x.Seq > 0 {
		a.Timeline.Continue(x.Epoch, x.Seq)
	}
```
After `func (a *Agent) Provider() string`:
```go
func (a *Agent) Named() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.named
}
```

**Step 4: Verify**

Run: same command. Expected: PASS.

### Task 4.3: The Daemon hands over and adopts

**Files:**
- Create: `packages/pocketd/internal/daemon/handoff.go`, `packages/pocketd/internal/daemon/handoff_test.go`
- Modify: `packages/pocketd/internal/daemon/restore.go:30-38` (hint), `:108` (Restore's call), `:136-170` (expect), `:222-245` (finish)
- Modify: `packages/pocketd/internal/daemon/presence.go:85` (attach)

**Context:**
- `Handoff` pauses everything first, then takes one `Snapshot`, so every entry describes a stopped reader. `Snapshot` already gives each Terminal its Agent's saved entry, and only for Agents with a Conversation.
- `Adopt` lists each Agent through `expect`, exactly as `Restore` does. The difference is that `h.handoff` is set and the carried state is applied.
- When the poller finds the provider, `adopt` binds it, and `attach` settles the hint at once through `handedOver`:
  - it marks the claude `hooked`, so the 5s attach timer doesn't mark it unattached;
  - it calls `finish(h, "")`, which for a handoff hint swaps in the live driver and returns before the notice and the report.
- Codex binds through the hint as on a restore (`attach` → `bind` → `thread/resume`).

**Step 1: Write the failing tests**

`internal/daemon/handoff_test.go`:
```go
package daemon

import (
	"testing"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestHandoffDescribesEachTerminalAndItsAgent(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	eventually(t, "the transcript", func() bool { return pr.a.Summary().MaxSeq > 0 })
	pr.a.SetNamed("Fix login")

	terms, err := d.Handoff()
	if err != nil {
		t.Fatal(err)
	}
	defer d.CancelHandoff()
	s := pr.a.Summary()
	if len(terms) != 1 {
		t.Fatalf("handed %d terminals", len(terms))
	}
	got := terms[0]
	if got.Saved.TerminalID != term.Info().ID || got.Saved.AgentID != s.ID || got.Saved.ConversationID != "c1" || got.Pid != term.Pid() ||
		got.Agent == nil || got.Agent.Named != "Fix login" || got.Agent.Epoch != s.Epoch || got.Agent.Seq != s.MaxSeq {
		t.Fatalf("handed = %+v", got)
	}
	f := handoff.File{Format: handoff.Format, Terminals: terms}
	if err := handoff.Check(f); err != nil {
		t.Fatalf("before cancel: %v", err)
	}
	d.CancelHandoff()
	if handoff.Check(f) == nil {
		t.Fatal("the pty still survives exec after CancelHandoff")
	}
}

func TestAHandedOverAgentKeepsItsIDAndShowsNoRestoreNotice(t *testing.T) {
	defer func(w time.Duration) { ClaudeAttachWait = w }(ClaudeAttachWait)
	ClaudeAttachWait = 100 * time.Millisecond
	d := newDaemon(t)
	reported := make(chan string, 1)
	d.OnRestore = func(_ string, _ bool, _ int64, outcome, reason string) { reported <- outcome + " " + reason }
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
		TranscriptPath: transcript(t, "hi"), Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}

	d.expect(saved, term, "", &handoff.Agent{Named: "Fix login", Phase: "working", Epoch: 3, Seq: 12})
	pr := waitPresent(t, d, term)

	select {
	case r := <-reported:
		t.Fatalf("a handoff reported a restore: %q", r)
	case <-time.After(300 * time.Millisecond):
	}
	s := pr.a.Summary()
	if s.ID != "a-1" || s.Restore != "" || !s.Attached || s.Title != "Fix login" || s.Status != "working" || s.Epoch != 4 || s.MaxSeq < 12 {
		t.Fatalf("summary = %+v", s)
	}
	if d.hintOf(pr) != nil {
		t.Fatal("the hint is still waiting")
	}
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/daemon -run 'Handoff|HandedOver' -count=1`
Expected: FAIL. Build errors `d.Handoff undefined` and `too many arguments in call to d.expect`.

**Step 3: Implement**

`internal/daemon/handoff.go`:
```go
package daemon

import (
	"errors"
	"fmt"
	"log"
	"time"

	"pocketd/internal/handoff"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

// HandoffPause is how long Handoff waits for each Terminal's reader to stop.
var HandoffPause = 2 * time.Second

// Handoff pauses every Terminal, so unread output waits in the kernel, and
// describes each, with its Agent, for the pocketd that execs next. If the
// exec doesn't happen, CancelHandoff resumes them.
func (d *Daemon) Handoff() ([]handoff.Terminal, error) {
	all := d.Terminals.All()
	for _, t := range all {
		if err := t.Pause(HandoffPause); err != nil && !errors.Is(err, terminal.ErrClosed) {
			return nil, fmt.Errorf("pause %s: %w", t.Info().ID, err)
		}
	}
	saved := map[string]state.Terminal{}
	for _, e := range d.Snapshot().Terminals {
		saved[e.TerminalID] = e
	}
	out := []handoff.Terminal{}
	for _, t := range all {
		info := t.Info()
		h, err := t.Handoff()
		if errors.Is(err, terminal.ErrClosed) {
			continue
		}
		if err != nil {
			return nil, err
		}
		e, ok := saved[info.ID]
		if !ok {
			e = state.Terminal{TerminalID: info.ID, LaunchDir: info.Cwd, Cols: info.Cols, Rows: info.Rows}
		}
		out = append(out, handoff.Terminal{Saved: e, Cmd: info.Cmd, Args: info.Args, Cwd: info.Cwd,
			FD: h.FD, Pid: h.Pid, Screen: h.Screen, Agent: d.carried(e.AgentID)})
	}
	return out, nil
}

func (d *Daemon) CancelHandoff() {
	for _, t := range d.Terminals.All() {
		t.Resume()
	}
}

// carried is what agentID's Agent keeps across the exec beyond its saved entry.
func (d *Daemon) carried(agentID string) *handoff.Agent {
	if agentID == "" {
		return nil
	}
	a, err := d.Agents.Get(agentID)
	if err != nil {
		return nil
	}
	s := a.Summary()
	phase := "idle"
	if s.Status == "working" || s.Status == "needsYou" {
		phase = s.Status
	}
	return &handoff.Agent{Named: a.Named(), Phase: phase, Epoch: s.Epoch, Seq: s.MaxSeq}
}

// Adopt takes over what the pocketd before exec handed down. Unlike Restore,
// nothing is spawned, so the login env is read in the background.
func (d *Daemon) Adopt(f handoff.File) {
	if d.Capture != nil {
		go d.Recapture()
	}
	for _, e := range f.Terminals {
		t, err := d.Terminals.Adopt(terminal.Adopted{ID: e.Saved.TerminalID, Cmd: e.Cmd, Args: e.Args, Cwd: e.Cwd,
			Cols: e.Saved.Cols, Rows: e.Saved.Rows, FD: e.FD, Pid: e.Pid, Screen: e.Screen})
		if err != nil {
			log.Printf("adopt %s: %v", e.Saved.TerminalID, err)
			continue
		}
		if e.Agent != nil && e.Saved.AgentID != "" {
			d.expect(e.Saved, t, "", e.Agent)
		}
	}
}

// handedOver settles a hint the pocketd before exec handed down. Its agent
// never stopped, so it stays attached and gets no restore notice.
func (d *Daemon) handedOver(pr *presence, h *hint) {
	pr.mu.Lock()
	pr.hooked = true
	pr.mu.Unlock()
	d.finish(h, "")
}
```

`restore.go`:
- `hint` gains a field after `timer`:
  ```go
  	handoff bool // handed down by the pocketd before exec; its agent never stopped
  ```
- In `Restore`, `d.expect(e, t, reason)` becomes `d.expect(e, t, reason, nil)`.
- `expect`'s signature becomes `func (d *Daemon) expect(e state.Terminal, t *terminal.Terminal, reason string, carried *handoff.Agent)`, and its first lines become:
  ```go
  	h := &hint{saved: e, started: time.Now(), handoff: carried != nil}
  	x := agent.Restored{ID: e.AgentID, Cwd: e.LaunchDir, Provider: e.Provider, Conversation: e.ConversationID,
  		Origin: e.Origin, Fallback: filepath.Base(e.LaunchDir), CreatedAt: e.CreatedAt, Done: e.Status == "done", Failed: e.Failed, Pinned: e.Pinned}
  	if carried != nil {
  		x.Named, x.Phase, x.Epoch, x.Seq = carried.Named, carried.Phase, carried.Epoch, carried.Seq
  	}
  ```
- Import `pocketd/internal/handoff`.
- In `finish`, right after the first `d.mu.Unlock()` that follows the `if reason == ""` block, add:
  ```go
  	if h.handoff && reason == "" {
  		return
  	}
  ```

`presence.go`, at the top of `attach`:
```go
	if h := d.hintOf(pr); h != nil && h.handoff {
		defer d.handedOver(pr, h)
	}
```

**Step 4: Verify**

Run: `cd packages/pocketd && go test -race ./internal/daemon -count=1`
Expected: PASS, including every existing restore test.

### Task 4.4: Launch in flight, the upgrade op, HostState, events

**Files:**
- Modify: `packages/pocketd/internal/launch/receipts.go` (append), `packages/pocketd/internal/launch/launch.go` (after `New`, line 82)
- Modify: `packages/pocketd/internal/ops/ops.go:91-112` (Server), `:238` (before `case "config-set"`)
- Modify: `packages/pocketd/internal/peer/check.go:33`, `packages/pocketd/internal/peer/check_test.go:42`
- Modify: `packages/pocketd/internal/proto/messages.go:297-300`
- Modify: `packages/pocketd/internal/events/events.go:17-21`
- Test: `packages/pocketd/internal/launch/receipts_test.go`, `packages/pocketd/internal/peer/check_test.go`

**Context:**
- A create in flight holds a receipt whose `at` is still zero, and an upgrade must wait for it (ADR 0005).
- On success, `ops:upgrade` sends nothing: the exec closes the socket.
- `HostState` stays comparable, which `Monitor.Set` relies on, and `omitempty` keeps the golden files unchanged.

**Step 1: Write the failing tests**

`receipts_test.go`:
```go
func TestAReceiptIsInFlightUntilItFinishes(t *testing.T) {
	r := newReceipts(time.Now)
	if r.inFlight() {
		t.Fatal("no receipts, yet in flight")
	}
	x, _ := r.take("k", proto.LaunchSpec{})
	if !r.inFlight() {
		t.Fatal("a running create isn't in flight")
	}
	r.finish(x, Result{})
	if r.inFlight() {
		t.Fatal("a finished create is still in flight")
	}
}
```
`check_test.go`, add a row to the matrix after `"ops:config-set"`:
```go
		"ops:upgrade":           {"", "scope_denied", "scope_denied", "scope_denied"},
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./internal/launch ./internal/peer -run 'InFlight|Matrix|Check' -count=1`
Expected: FAIL. Build error `r.inFlight undefined`, and once that builds, `ops:upgrade by principal 0: "scope_denied", want ""`.

**Step 3: Implement**

`receipts.go`:
```go
// inFlight reports whether a create hasn't finished.
func (r *receipts) inFlight() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, x := range r.m {
		if x.at.IsZero() {
			return true
		}
	}
	return false
}
```
`launch.go`:
```go
// Starting reports whether a create is running; an upgrade waits for it.
func (l *Launcher) Starting() bool { return l.receipts.inFlight() }
```
`peer/check.go`, after `"ops:config-set": Own,`:
```go
	"ops:upgrade":           Own,
```
`ops.go`, in `Server`, after `ConfigSet`:
```go
	// Upgrade execs pocketd's binary on disk. It returns only on failure;
	// on success the caller sees the socket close.
	Upgrade func() error
```
and before `case "config-set":`
```go
		case "upgrade":
			if s.Upgrade == nil {
				c.Send(Msg{Ev: "error", Error: "upgrade unsupported"})
				continue
			}
			if err := s.Upgrade(); err != nil {
				c.Send(Msg{Ev: "error", Error: err.Error()})
			}
			continue
```
`proto/messages.go`:
```go
type HostState struct {
	Tailnet      bool `json:"tailnet"`
	KeepingAwake bool `json:"keepingAwake"`
	// UpgradeFailed is the version pocketd last failed to upgrade to.
	UpgradeFailed string `json:"upgradeFailed,omitempty"`
}
```
`events.go` doc: append `upgrade{version,ok,reason}` after `refusal{surface,code}` (`…, refusal{surface,code}, upgrade{version,ok,reason}.`).

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./internal/launch ./internal/peer ./internal/ops ./internal/proto -count=1`
Expected: PASS. The goldens are unchanged.

### Task 4.5: The upgrader, `serve --handoff` and `pocketd upgrade`

**Files:**
- Create: `packages/pocketd/cmd/pocketd/upgrade.go`, `packages/pocketd/cmd/pocketd/upgrade_test.go`
- Modify: `packages/pocketd/cmd/pocketd/serve.go:45` (signature), `:146-151` (restore), `:134-145` (upgrader), `:200-201` (ops.Server)
- Modify: `packages/pocketd/cmd/pocketd/main.go:15` (usage), `:32-33` (dispatch)

**Context:**
- **What dies at exec.** `syscall.Exec` never returns on success, so no defer in `serve` runs: no `stop` event, no `CloseAll` (ADR rule). The ops socket, listeners, logs, events and lock fds are all CLOEXEC and close.
- **Reason codes.** Each failure is logged and emitted with a reason code, never a path: `not_runnable`, `pause`, `write`, `check`, `exec`. It also sets `HostState.UpgradeFailed`, which shows the desktop's toast.
- **The new image.** It starts in `serve` as usual: the lock, then the reap (which spares the shells, still our children), then `--handoff` instead of `state.json`.

**Step 1: Write the failing tests**

`cmd/pocketd/upgrade_test.go`:
```go
package main

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/daemon"
	"pocketd/internal/handoff"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/terminal"
)

// fakePocketd is a script that answers --version as pocketd v-bad and fails anything else.
func fakePocketd(t *testing.T) string {
	f := filepath.Join(t.TempDir(), "pocketd")
	if err := os.WriteFile(f, []byte("#!/bin/sh\n[ \"$1\" = --version ] && echo 'pocketd v-bad' && exit 0\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return f
}

func TestAFailedDryRunResumesTheTerminalsAndFlagsTheHost(t *testing.T) {
	d := &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New())}
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "sh", Args: []string{"-c", `read x; echo "got:$x"; sleep 5`}, Cols: 40, Rows: 5})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	mon := host.NewMonitor()
	u := &upgrader{exe: fakePocketd(t), home: t.TempDir(), d: d, starting: func() bool { return false }, mon: mon}

	if err := u.run(); err == nil || !strings.Contains(err.Error(), "check") {
		t.Fatalf("err = %v", err)
	}
	if got := mon.State().UpgradeFailed; got != "v-bad" {
		t.Fatalf("upgradeFailed = %q", got)
	}
	if _, err := os.Stat(handoff.Path(u.home)); !os.IsNotExist(err) {
		t.Fatalf("the handoff file is left: %v", err)
	}
	term.Write([]byte("hi\r"))
	for deadline := time.Now().Add(5 * time.Second); !strings.Contains(term.Screen(), "got:hi"); time.Sleep(20 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatalf("the terminal stopped reading:\n%s", term.Screen())
		}
	}
}

func TestAnUpgradeWaitsForALaunchInFlight(t *testing.T) {
	u := &upgrader{exe: fakePocketd(t), starting: func() bool { return true }}
	if err := u.run(); !errors.Is(err, errBusy) {
		t.Fatalf("err = %v", err)
	}
}

func TestABinaryThatIsntPocketdHasNoVersion(t *testing.T) {
	if v, err := binaryVersion("/bin/echo"); err == nil {
		t.Fatalf("version = %q", v)
	}
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'Upgrade|DryRun|Version' -count=1`
Expected: FAIL. Build error `undefined: upgrader`.

**Step 3: Implement**

`cmd/pocketd/upgrade.go`:
```go
package main

import (
	"bytes"
	"cmp"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"

	"pocketd/internal/daemon"
	"pocketd/internal/events"
	"pocketd/internal/handoff"
	"pocketd/internal/host"
	"pocketd/internal/ops"
	"pocketd/internal/proto"
)

var errBusy = errors.New("a session is being created; upgrade after it")

// upgrader hands pocketd's live state to the binary at exe and execs it in
// this pid (ADR 0005).
type upgrader struct {
	exe, home string
	d         *daemon.Daemon
	starting  func() bool
	evs       *events.Log
	mon       *host.Monitor
	mu        sync.Mutex
}

// run returns only when the upgrade didn't happen; the old pocketd goes on.
func (u *upgrader) run() error {
	u.mu.Lock()
	defer u.mu.Unlock()
	if u.starting() {
		return errBusy
	}
	to, err := binaryVersion(u.exe)
	if err != nil {
		return u.failed("", "not_runnable", err)
	}
	terms, err := u.d.Handoff()
	if err != nil {
		u.d.CancelHandoff()
		return u.failed(to, "pause", err)
	}
	path := handoff.Path(u.home)
	if err := handoff.Write(path, handoff.File{Format: handoff.Format, Terminals: terms}); err != nil {
		return u.abort(path, to, "write", err)
	}
	if err := dryRun(u.exe, path); err != nil {
		return u.abort(path, to, "check", err)
	}
	ok := true
	u.evs.Emit(events.Event{Kind: "upgrade", Version: to, OK: &ok})
	err = syscall.Exec(u.exe, []string{u.exe, "serve", "--handoff", path}, os.Environ())
	return u.abort(path, to, "exec", err)
}

func (u *upgrader) abort(path, to, reason string, err error) error {
	u.d.CancelHandoff()
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

// dryRun has the new binary check the handoff file. It inherits the pty fds,
// since Handoff cleared their close-on-exec.
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
	c, err := ops.Dial(sock)
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
```

`serve.go`:
- `func serve(sock string) error {` becomes `func serve(sock, handed string) error {`, and import `pocketd/internal/handoff`.
- Replace lines 146-151 (`statePath := …` through `d.Restore(…)`) with:
  ```go
  	statePath := state.Path(d.Home)
  	if handed != "" {
  		f, err := handoff.Read(handed)
  		os.Remove(handed)
  		if err != nil {
  			return fmt.Errorf("handoff: %w", err)
  		}
  		d.Adopt(f)
  		// The awake assertion died with the old image.
  		select {
  		case kick <- struct{}{}:
  		default:
  		}
  	} else {
  		saved, err := state.Load(statePath)
  		if err != nil {
  			log.Print(err)
  		}
  		d.Restore(saved, cmp.Or(os.Getenv("POCKETD_RESTORE_SHELL"), shellenv.LoginShell()))
  	}
  ```
- After the `d.OnRestore = …` block (line 145):
  ```go
  	up := &upgrader{exe: exe, home: home, d: d, starting: l.Starting, evs: evs, mon: mon}
  ```
- In the `ops.Server` literal, after `ConfigSet:  configSetter(l, settings),`:
  ```go
  		Upgrade:    up.run,
  ```

`main.go`:
- `usage`: append ` | upgrade` after `status [--json]`.
- Dispatch: replace `case os.Args[1] == "serve":` and its body with:
  ```go
  	case len(os.Args) == 4 && os.Args[1] == "serve" && os.Args[2] == "--handoff":
  		err = serve(sock, os.Args[3])
  	case os.Args[1] == "serve":
  		err = serve(sock, "")
  	case slices.Equal(os.Args[1:], []string{"upgrade"}):
  		err = upgradeCmd(sock, os.Stdout)
  ```

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./cmd/pocketd -count=1 && go test ./e2e -run 'Upgrade|FailedDryRun' -count=1 -v`
Expected: PASS. The e2e log shows two `start` events with the same pid in `events.jsonl` and one `upgrade{ok:true}` between them. Then run `go vet ./... && go test -race -count=1 ./...`, which should PASS.

---

## PR B5: Upgrades happen on their own

**Scope:**
- the self-watch;
- `make pocketd` only builds; the self-watch picks the binary up;
- the desktop shows a toast for a failed upgrade.

**Depends on:** B4
**Done when:** `go test ./e2e -run Replacing -count=1` passes, the desktop gates pass, and `scripts/check.sh` passes.

### Task 5.1: e2e: replacing the binary upgrades pocketd (fails until 5.2)

**Files:**
- Modify: `packages/pocketd/e2e/harness_test.go:33-39` (TestMain)
- Modify: `packages/pocketd/e2e/upgrade_test.go`

**Context:** `pocketd-next` is the same code with another version stamped in, which is what a Sparkle update looks like from the daemon's side.

**Step 1: Write the failing test**

`TestMain`, after the build loop:
```go
	next := exec.Command("go", "build", "-ldflags", "-X main.version=e2e-next", "-o", filepath.Join(dir, "pocketd-next"), "../cmd/pocketd")
	if out, err := next.CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "build pocketd-next: %v\n%s", err, out)
		os.Exit(1)
	}
```
`upgrade_test.go`:
```go
func TestReplacingTheBinaryUpgradesPocketd(t *testing.T) {
	h := Start(t, ownBinary)
	id, shell := counting(h)
	last := h.countsPast(id, 0)
	pid := h.Status().PID

	h.replaceBinary(filepath.Join(binDir, "pocketd-next"))

	h.eventually("pocketd e2e-next", func() bool { return h.Status().Version == "e2e-next" })
	if s := h.Status(); s.PID != pid {
		t.Fatalf("pocketd pid %d, was %d", s.PID, pid)
	}
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died")
	}
	h.countsPast(id, last[len(last)-1]+5)
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./e2e -run 'TestReplacingTheBinaryUpgradesPocketd' -count=1`
Expected: FAIL. `timed out waiting for pocketd e2e-next`.

### Task 5.2: The self-watch

**Files:**
- Create: `packages/pocketd/cmd/pocketd/selfwatch.go`, `packages/pocketd/cmd/pocketd/selfwatch_test.go`
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (after `up := …`)

**Context:**
- Sparkle replaces the bundle by rename, so the path gets a new inode. `go build -o` replaces or rewrites the file, so its mtime or size changes.
- A stat check runs every tick; the sha256 runs only when the stat changed. The sum it compares against is the one this image started from.
- A dev rebuild keeps its `<rev>-dirty` version, so comparing versions would miss it. Comparing contents doesn't.
- A failed upgrade marks the binary seen, so the watch doesn't retry it every 3s; the next change to the file retries. Only `errBusy` leaves it for the next tick.

**Step 1: Write the failing tests**

`cmd/pocketd/selfwatch_test.go`:
```go
package main

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// writePocketd puts a script at path, by rename as Sparkle does.
func writePocketd(t *testing.T, path, script string) {
	t.Helper()
	if err := os.WriteFile(path+".tmp", []byte("#!/bin/sh\n"+script+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(path+".tmp", path); err != nil {
		t.Fatal(err)
	}
}

func watching(t *testing.T) *selfWatch {
	exe := filepath.Join(t.TempDir(), "pocketd")
	writePocketd(t, exe, "echo 'pocketd abc-dirty'")
	st, err := os.Stat(exe)
	if err != nil {
		t.Fatal(err)
	}
	sum, err := fileSum(exe)
	if err != nil {
		t.Fatal(err)
	}
	return &selfWatch{exe: exe, sum: sum, seen: st}
}

func TestARebuiltBinaryUpgradesEvenWithTheSameVersion(t *testing.T) {
	w := watching(t)
	calls := 0
	upgrade := func() error { calls++; return nil }
	w.check(upgrade)
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty' # rebuilt")
	w.check(upgrade)
	w.check(upgrade)
	if calls != 1 {
		t.Fatalf("%d upgrades, want 1", calls)
	}
}

func TestATouchedOrIdenticalBinaryDoesNot(t *testing.T) {
	w := watching(t)
	later := time.Now().Add(time.Hour)
	os.Chtimes(w.exe, later, later)
	calls := 0
	upgrade := func() error { calls++; return nil }
	w.check(upgrade)
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty'")
	w.check(upgrade)
	if calls != 0 {
		t.Fatalf("%d upgrades, want 0", calls)
	}
}

func TestABusyUpgradeIsRetriedNextTick(t *testing.T) {
	w := watching(t)
	writePocketd(t, w.exe, "echo 'pocketd 1.1.0'")
	results := []error{errBusy, nil}
	calls := 0
	upgrade := func() error { calls++; return results[calls-1] }
	w.check(upgrade)
	w.check(upgrade)
	w.check(upgrade)
	if calls != 2 {
		t.Fatalf("%d upgrades, want 2", calls)
	}
}

func TestAFailedUpgradeWaitsForTheNextChange(t *testing.T) {
	w := watching(t)
	writePocketd(t, w.exe, "exit 1")
	results := []error{errors.New("dry run failed"), nil}
	calls := 0
	upgrade := func() error { calls++; return results[calls-1] }
	w.check(upgrade)
	w.check(upgrade)
	if calls != 1 {
		t.Fatalf("%d upgrades before the fix, want 1", calls)
	}
	writePocketd(t, w.exe, "echo 'pocketd abc-dirty' # fixed")
	w.check(upgrade)
	if calls != 2 {
		t.Fatalf("%d upgrades after the fix, want 2", calls)
	}
}
```

**Step 2: Run to verify failure**

Run: `cd packages/pocketd && go test ./cmd/pocketd -run 'Binary|Busy|FailedUpgrade' -count=1`
Expected: FAIL. Build error `undefined: selfWatch`.

**Step 3: Implement**

`cmd/pocketd/selfwatch.go`:
```go
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
	if sum, err := fileSum(w.exe); err == nil && sum != w.sum && errors.Is(upgrade(), errBusy) {
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
```
`serve.go`, after `up := …`:
```go
	if st, err := os.Stat(exe); err == nil {
		if sum, err := fileSum(exe); err == nil {
			go (&selfWatch{exe: exe, sum: sum, seen: st}).run(ctx, up.run)
		}
	}
```

**Step 4: Verify**

Run: `cd packages/pocketd && go test ./cmd/pocketd -count=1 && go test ./e2e -run 'Upgrade|FailedDryRun|Replacing' -count=1`
Expected: PASS. In `TestAFailedDryRunKeepsTheOldPocketd`, the watch may fail first. Its `host.changed` is the one the owner sees, and the CLI's own attempt still fails at `check`.

### Task 5.3: `make pocketd` leaves the upgrade to the self-watch

**Files:**
- Modify: `Makefile:10`

**Step 1: Implement**

Line 10 becomes:
```make
	launchctl print $(SERVICE) >/dev/null 2>&1 || $(POCKETD) daemon install
```

**Step 2: Verify (manual, on the dev machine)**

1. Note `pocketd status`'s pid, change any Go file under `packages/pocketd`, and run `make pocketd`, from any terminal, including one inside Anywhere Dev.
2. Within about 3s, `pocketd.log` shows the upgrade, and `pocketd status` shows the same pid.
3. Any open Anywhere Dev terminal keeps its shell and screen.
4. Run `make pocketd` again with no change. Nothing upgrades.

### Task 5.4: The desktop shows a failed upgrade once

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs:158-161` (Host), tests at `:698,700,719,722`
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:6` (imports), `:116` (on_agents), tests module at `:187`
- Modify: `packages/desktop/crates/pocket/src/sidebar/host.rs:47,49,55` (test literals)

**Context:**
- `upgradeFailed` arrives in `hello.ok` and `host.changed`.
- The decision is a free function over two `Host`s (CLAUDE.md: no `Window` or `Context` in logic). `on_agents` calls it before `apply` replaces `agents.host`.
- It reuses the error toast. There is no new view, so no capture is needed.

**Step 1: Write the failing tests**

`agents.rs` tests:
```rust
    #[test]
    fn a_host_frame_carries_a_failed_upgrade() {
        let frame = serde_json::from_str::<Frame>(r#"{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false,"upgradeFailed":"1.2.0"}}"#).unwrap();
        let Some(Event::Host(host)) = host_event(&frame) else { panic!("no host event") };
        assert_eq!(host.upgrade_failed, "1.2.0");
    }
```
`alerts.rs` tests: change the imports to `use super::{ALLOW, Alerts, DENY, Notice, body, upgrade_toast};` and `use agents::{Agents, Host, Permission, Summary};`, then add:
```rust
    #[test]
    fn an_upgrade_that_fails_shows_one_toast() {
        let failed = Host { upgrade_failed: "1.2.0".into(), ..Default::default() };
        let first = upgrade_toast(Some(&Host::default()), &failed);
        let again = upgrade_toast(Some(&failed), &failed);
        assert_eq!((first.is_some(), again), (true, None));
    }

    #[test]
    fn a_host_change_without_a_failed_upgrade_shows_no_toast() {
        assert_eq!(upgrade_toast(None, &Host { tailnet: true, ..Default::default() }), None);
    }
```

**Step 2: Run to verify failure**

Run: `cd packages/desktop && cargo test -p agents a_host_frame_carries_a_failed_upgrade`
Expected: FAIL. `no field upgrade_failed on type Host`.

**Step 3: Implement**

`agents.rs`, `Host`:
```rust
pub struct Host {
    pub tailnet: bool,
    pub keeping_awake: bool,
    /// The version pocketd last failed to upgrade to; empty when none failed.
    pub upgrade_failed: String,
}
```
Then add `, ..Default::default()` to the `Host { tailnet: …, keeping_awake: … }` literals at `agents.rs:698,700,719,722` and `sidebar/host.rs:47,49,55`.

`alerts.rs`: add `Host` to the `use agents::{…}` line. Before `let connected = …` in `on_agents`:
```rust
        if let Event::Host(host) = &ev
            && let Some(message) = upgrade_toast(self.agents.host.as_ref(), host)
        {
            self.error = Some(message);
        }
```
and, as a free function after `impl Desktop`:
```rust
/// The toast for a failed pocketd upgrade, once per version it failed on.
pub(crate) fn upgrade_toast(prev: Option<&Host>, next: &Host) -> Option<String> {
    let failed = &next.upgrade_failed;
    (!failed.is_empty() && prev.is_none_or(|p| &p.upgrade_failed != failed))
        .then(|| format!("Couldn't update pocketd to {failed}. The running one keeps going."))
}
```

**Step 4: Verify**

Run: `cd packages/desktop && cargo test -p agents host && cargo test -p pocket upgrade && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS with no new clippy warnings. Then, from the repo root, `scripts/check.sh` should PASS.
