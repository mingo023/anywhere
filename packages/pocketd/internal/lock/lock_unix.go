//go:build unix

package lock

import (
	"os"
	"syscall"

	"golang.org/x/sys/unix"
)

var errBusy = syscall.EWOULDBLOCK

func lockNB(f *os.File, exclusive bool) error {
	how := syscall.LOCK_SH
	if exclusive {
		how = syscall.LOCK_EX
	}
	return syscall.Flock(int(f.Fd()), how|syscall.LOCK_NB)
}

// Keep copies l's fd to one that survives exec. The flock belongs to the open
// file, so the exec'd image holds it with no gap.
func (l *Lock) Keep() (int, error) {
	rc, err := l.f.SyscallConn()
	if err != nil {
		return 0, err
	}
	var fd int
	var dupErr error
	if err := rc.Control(func(p uintptr) { fd, dupErr = unix.FcntlInt(p, unix.F_DUPFD, 0) }); err != nil {
		return 0, err
	}
	return fd, dupErr
}

// Adopt is the lock an image before exec kept as fd.
func Adopt(fd int, home string) *Lock {
	syscall.CloseOnExec(fd)
	return &Lock{os.NewFile(uintptr(fd), path(home))}
}
