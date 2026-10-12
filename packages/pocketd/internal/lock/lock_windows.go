package lock

import (
	"os"

	"golang.org/x/sys/windows"
)

var errBusy = windows.ERROR_LOCK_VIOLATION

// lockNB locks one byte far past the pid text: Windows locks are mandatory,
// so locking the text itself would keep Holder from reading it.
func lockNB(f *os.File, exclusive bool) error {
	flags := uint32(windows.LOCKFILE_FAIL_IMMEDIATELY)
	if exclusive {
		flags |= windows.LOCKFILE_EXCLUSIVE_LOCK
	}
	return windows.LockFileEx(windows.Handle(f.Fd()), flags, 0, 1, 0, &windows.Overlapped{OffsetHigh: 0x7fffffff})
}

func lockWait(f *os.File) error {
	return windows.LockFileEx(windows.Handle(f.Fd()), windows.LOCKFILE_EXCLUSIVE_LOCK, 0, 1, 0, &windows.Overlapped{OffsetHigh: 0x7fffffff})
}
