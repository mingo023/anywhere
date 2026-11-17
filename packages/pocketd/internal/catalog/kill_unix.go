//go:build unix

package catalog

import (
	"os/exec"
	"syscall"
)

// killGroup makes cmd lead its own group, so a cancel kills what it started too.
func killGroup(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error { return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL) }
}
