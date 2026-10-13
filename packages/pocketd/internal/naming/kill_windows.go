package naming

import (
	"os/exec"
	"strconv"
)

// killGroup makes a timeout kill cmd's whole process tree.
func killGroup(cmd *exec.Cmd) {
	cmd.Cancel = func() error {
		return exec.Command("taskkill", "/T", "/F", "/PID", strconv.Itoa(cmd.Process.Pid)).Run()
	}
}
