//go:build unix

package lock

import (
	"os"
	"syscall"
)

var errBusy = syscall.EWOULDBLOCK

func lockNB(f *os.File, exclusive bool) error {
	how := syscall.LOCK_SH
	if exclusive {
		how = syscall.LOCK_EX
	}
	return syscall.Flock(int(f.Fd()), how|syscall.LOCK_NB)
}
