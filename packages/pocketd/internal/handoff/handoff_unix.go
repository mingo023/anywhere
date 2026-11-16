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
	for _, t := range f.Terminals {
		flags, err := unix.FcntlInt(uintptr(t.FD), unix.F_GETFD, 0)
		if err != nil {
			return fmt.Errorf("terminal %s: fd %d: %w", t.Saved.TerminalID, t.FD, err)
		}
		if flags&unix.FD_CLOEXEC != 0 {
			return fmt.Errorf("terminal %s: fd %d closes on exec", t.Saved.TerminalID, t.FD)
		}
	}
	if f.MCPFD != 0 {
		flags, err := unix.FcntlInt(uintptr(f.MCPFD), unix.F_GETFD, 0)
		if err != nil {
			return fmt.Errorf("mcp listener: fd %d: %w", f.MCPFD, err)
		}
		if flags&unix.FD_CLOEXEC != 0 {
			return fmt.Errorf("mcp listener: fd %d closes on exec", f.MCPFD)
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
	return fd, dupErr
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
