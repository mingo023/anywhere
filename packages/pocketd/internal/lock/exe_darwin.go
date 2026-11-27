package lock

import (
	"bytes"
	"path/filepath"

	"golang.org/x/sys/unix"
)

// kern.procargs2 is a 4-byte argc, then the exec path. Symlinks are resolved to match Linux's /proc/<pid>/exe.
func exe(pid int) string {
	b, err := unix.SysctlRaw("kern.procargs2", pid)
	if err != nil || len(b) < 4 {
		return ""
	}
	path, _, _ := bytes.Cut(b[4:], []byte{0})
	if real, err := filepath.EvalSymlinks(string(path)); err == nil {
		return real
	}
	return string(path)
}
