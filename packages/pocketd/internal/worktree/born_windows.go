package worktree

import (
	"os"
	"syscall"
)

func born(path string) int64 {
	st, err := os.Stat(path)
	if err != nil {
		return 0
	}
	if a, ok := st.Sys().(*syscall.Win32FileAttributeData); ok {
		return a.CreationTime.Nanoseconds()
	}
	return st.ModTime().UnixNano()
}
