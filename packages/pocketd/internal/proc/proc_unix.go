//go:build unix

package proc

import (
	"os"
	"syscall"

	"golang.org/x/sys/unix"
)

// Foreground returns the PTY's foreground process group; 0 means none.
func Foreground(f *os.File) (int, error) {
	// Not f.Fd(): it would switch the PTY to blocking mode and hang the reader goroutine on Close.
	rc, err := f.SyscallConn()
	if err != nil {
		return 0, err
	}
	var pgid int
	var ioErr error
	if err := rc.Control(func(fd uintptr) { pgid, ioErr = unix.IoctlGetInt(int(fd), unix.TIOCGPGRP) }); err != nil {
		return 0, err
	}
	return pgid, ioErr
}

// ExitCode is st's exit status, or 128 plus the signal that ended it, as a shell reports one.
func ExitCode(st *os.ProcessState) int {
	if ws, ok := st.Sys().(syscall.WaitStatus); ok && ws.Signaled() {
		return 128 + int(ws.Signal())
	}
	return st.ExitCode()
}
