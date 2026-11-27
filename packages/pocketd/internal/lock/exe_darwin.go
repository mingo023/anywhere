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
	raw, _, _ := bytes.Cut(b[4:], []byte{0})
	path := string(raw)
	if !filepath.IsAbs(path) {
		return path
	}
	if real, err := filepath.EvalSymlinks(path); err == nil {
		return real
	}
	return path
}
