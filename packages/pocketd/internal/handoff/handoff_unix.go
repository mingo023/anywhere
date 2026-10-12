//go:build unix

package handoff

import (
	"fmt"

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
	return nil
}
