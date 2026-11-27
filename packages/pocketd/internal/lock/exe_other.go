//go:build !darwin

package lock

import (
	"os"
	"strconv"
)

func exe(pid int) string {
	path, _ := os.Readlink("/proc/" + strconv.Itoa(pid) + "/exe")
	return path
}
