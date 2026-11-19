//go:build unix

package terminal

import (
	"cmp"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/creack/pty"
	"golang.org/x/sys/unix"

	"pocketd/internal/holder"
	"pocketd/internal/proc"
)

// ptySession is a process on a pty pocketd holds itself.
type ptySession struct {
	f *os.File
	p *os.Process
}

func (s ptySession) Read(b []byte) (int, error)  { return s.f.Read(b) }
func (s ptySession) Write(b []byte) (int, error) { return s.f.Write(b) }
func (s ptySession) Resize(cols, rows int) error { return setsize(s.f, cols, rows) }
func (s ptySession) Pid() int                    { return s.p.Pid }
func (s ptySession) Stop(grace time.Duration)    { proc.EndSession(s.p.Pid, grace) }
func (s ptySession) Close() error                { return s.f.Close() }
func (s ptySession) Checkpoint([]byte)           {}
func (s ptySession) Foreground() (int, error)    { return proc.Foreground(s.f) }

func (s ptySession) Wait() int {
	if st, err := s.p.Wait(); err == nil {
		return st.ExitCode()
	}
	return -1
}

// open starts spec's process: under a holder when the Manager has one.
func (m *Manager) open(bin string, spec Spec) (session, error) {
	if m.Holder != nil {
		return m.Holder.Start(holder.Spec{ID: spec.ID, Cmd: spec.Cmd, Bin: bin, Args: spec.Args, Cwd: spec.Cwd, Env: spec.Env,
			Cols: spec.Cols, Rows: spec.Rows, Local: spec.Local}, nil)
	}
	f, p, err := start(bin, spec)
	if err != nil {
		return nil, err
	}
	return ptySession{f, p}, nil
}

// adopt takes a's pty. A holder can't learn the exit code of a process it
// didn't start, and this pocketd, its parent, reaps it.
func (m *Manager) adopt(a Adopted) (session, error) {
	f, p, err := adopt(a.FD, a.Pid)
	if err != nil {
		return nil, err
	}
	if m.Holder == nil {
		return ptySession{f, p}, nil
	}
	defer f.Close()
	c, err := m.Holder.Start(holder.Spec{ID: a.ID, Cmd: a.Cmd, Args: a.Args, Cwd: a.Cwd, Cols: a.Cols, Rows: a.Rows, Local: a.Local, AdoptPid: a.Pid}, f)
	if err != nil {
		return nil, err
	}
	go p.Wait()
	return c, nil
}

// LookPath resolves cmd against the caller's PATH, not the daemon's: a
// daemon started by launchd has a minimal PATH. The last PATH entry wins, as
// it does for exec.Cmd.Env.
func LookPath(cmd string, env []string) (string, error) {
	if strings.Contains(cmd, "/") {
		return cmd, nil
	}
	path, found := "", false
	for _, kv := range env {
		if p, ok := strings.CutPrefix(kv, "PATH="); ok {
			path, found = p, true
		}
	}
	if !found {
		return exec.LookPath(cmd)
	}
	for _, dir := range filepath.SplitList(path) {
		f := filepath.Join(dir, cmd)
		if st, err := os.Stat(f); err == nil && !st.IsDir() && st.Mode()&0o111 != 0 {
			return f, nil
		}
	}
	return "", &exec.Error{Name: cmd, Err: exec.ErrNotFound}
}

func start(bin string, spec Spec) (*os.File, *os.Process, error) {
	cmd := exec.Command(bin, spec.Args...)
	cmd.Dir = spec.Cwd
	cmd.Env = spec.Env
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(spec.Cols), Rows: uint16(spec.Rows)})
	if err != nil {
		return nil, nil, err
	}
	if f, err = pollable(f); err != nil {
		cmd.Process.Kill()
		cmd.Wait()
		return nil, nil, err
	}
	return f, cmd.Process, nil
}

func adopt(fd, pid int) (*os.File, *os.Process, error) {
	syscall.CloseOnExec(fd)
	f := os.NewFile(uintptr(fd), "/dev/ptmx")
	p, _ := os.FindProcess(pid) // never fails on unix
	return f, p, nil
}

// pollable moves f to a non-blocking fd the runtime poller owns, so Close
// ends a pending read. creack/pty's file blocks a thread instead.
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

func group(sess session) (int, error) {
	return sess.(interface{ Foreground() (int, error) }).Foreground()
}

func (s *Terminal) stop(grace time.Duration) {
	s.mu.Lock()
	sess := s.sess
	s.mu.Unlock()
	sess.Stop(grace)
}
