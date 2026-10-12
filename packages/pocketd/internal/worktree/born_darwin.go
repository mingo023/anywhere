package worktree

import "syscall"

func born(path string) int64 {
	var st syscall.Stat_t
	if syscall.Stat(path, &st) != nil {
		return 0
	}
	return st.Birthtimespec.Nano()
}
