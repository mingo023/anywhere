package worktree

import "golang.org/x/sys/unix"

// born falls back to mtime where the filesystem keeps no birth time.
func born(path string) int64 {
	var st unix.Statx_t
	if unix.Statx(unix.AT_FDCWD, path, 0, unix.STATX_BTIME|unix.STATX_MTIME, &st) != nil {
		return 0
	}
	t := st.Mtime
	if st.Mask&unix.STATX_BTIME != 0 {
		t = st.Btime
	}
	return t.Sec*1e9 + int64(t.Nsec)
}
