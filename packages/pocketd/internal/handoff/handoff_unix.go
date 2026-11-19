//go:build unix

package handoff

import (
	"fmt"
	"net"
	"os"

	"golang.org/x/sys/unix"
)

// Check is the dry run's test: every fd in f is open here and survives exec.
func Check(f File) error {
	fds := map[string]int{"mcp listener": f.MCPFD, "lock": f.LockFD}
	for _, t := range f.Terminals {
		fds["terminal "+t.Saved.TerminalID] = t.FD
	}
	for what, fd := range fds {
		if fd == 0 {
			continue
		}
		flags, err := unix.FcntlInt(uintptr(fd), unix.F_GETFD, 0)
		if err != nil {
			return fmt.Errorf("%s: fd %d: %w", what, fd, err)
		}
		if flags&unix.FD_CLOEXEC != 0 {
			return fmt.Errorf("%s: fd %d closes on exec", what, fd)
		}
	}
	return nil
}

// Keep copies ln's socket to an fd that stays open across exec, so the new
// binary listens on the same port with no gap in which a connection is refused.
// The caller closes the fd with Release when the exec doesn't happen.
func Keep(ln net.Listener) (int, error) {
	tcp, ok := ln.(*net.TCPListener)
	if !ok {
		return 0, fmt.Errorf("a %T can't be handed down", ln)
	}
	raw, err := tcp.SyscallConn()
	if err != nil {
		return 0, err
	}
	var fd int
	var dupErr error
	if err := raw.Control(func(s uintptr) { fd, dupErr = unix.FcntlInt(s, unix.F_DUPFD, 0) }); err != nil {
		return 0, err
	}
	// No fd goes down then: the listener closes on exec, and the next pocketd listens on its port afresh.
	if dupErr != nil {
		return 0, dupErr
	}
	return fd, nil
}

// Release closes an fd Keep returned; 0 is no fd.
func Release(fd int) {
	if fd > 0 {
		unix.Close(fd)
	}
}

// Inherit listens on the socket Keep handed down as fd.
func Inherit(fd int) (net.Listener, error) {
	f := os.NewFile(uintptr(fd), "handed-down listener")
	defer f.Close()
	return net.FileListener(f)
}

// Claim marks f's pty fds close-on-exec, so the holder started for one
// doesn't inherit the others.
func Claim(f File) {
	for _, t := range f.Terminals {
		if t.FD > 0 {
			unix.CloseOnExec(t.FD)
		}
	}
}
