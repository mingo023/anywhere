//go:build unix

package proc

import (
	"errors"
	"slices"
	"time"

	"golang.org/x/sys/unix"
)

// Alive reports whether pid exists, even when another user owns it.
func Alive(pid int) bool {
	err := unix.Kill(pid, 0)
	return err == nil || errors.Is(err, unix.EPERM)
}

// Kill sends SIGTERM to each pid, waits up to grace for all to exit, then SIGKILLs the rest.
func Kill(pids []int, grace time.Duration) {
	for _, pid := range pids {
		unix.Kill(pid, unix.SIGTERM)
	}
	for deadline := time.Now().Add(grace); time.Now().Before(deadline) && slices.ContainsFunc(pids, Alive); {
		time.Sleep(50 * time.Millisecond)
	}
	for _, pid := range pids {
		if Alive(pid) {
			unix.Kill(pid, unix.SIGKILL)
		}
	}
}
