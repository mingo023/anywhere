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

	"pocketd/internal/proc"
)

type console = *os.File

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

func (s *Terminal) group() (int, error) { return proc.Foreground(s.pty) }

// stop signals every process group in the shell's session: job-control jobs
// get their own groups, and nohup'd children ignore the pty's SIGHUP.
// SIGHUP goes with SIGTERM because an interactive shell ignores SIGTERM.
func (s *Terminal) stop(grace time.Duration) {
	sid := s.Pid()
	signal := func(sigs ...syscall.Signal) {
		pgids, err := proc.Session(sid)
		if err != nil {
			pgids = []int{sid}
		}
		for _, pg := range pgids {
			for _, sig := range sigs {
				syscall.Kill(-pg, sig)
			}
		}
	}
	signal(syscall.SIGHUP, syscall.SIGTERM)
	for deadline := time.Now().Add(grace); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		if pgids, err := proc.Session(sid); err == nil && len(pgids) == 0 {
			return
		}
	}
	signal(syscall.SIGKILL)
}
