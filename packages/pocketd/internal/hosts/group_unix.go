//go:build unix

package hosts

import (
	"os/exec"
	"syscall"
)

// ownGroup keeps ssh out of the foreground group of a terminal pocketd was started from.
func ownGroup(cmd *exec.Cmd) { cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true} }
