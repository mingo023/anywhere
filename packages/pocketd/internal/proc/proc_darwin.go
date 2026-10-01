// Package proc reads the processes behind a PTY from the macOS kernel.
package proc

import (
	"bytes"
	"encoding/binary"
	"os"
	"slices"

	"golang.org/x/sys/unix"
)

type Proc struct {
	Pid  int
	Sid  int // set by Orphans only
	Argv []string
	Env  []string
}

// Foreground returns the PTY's foreground process group; 0 means none.
func Foreground(f *os.File) (int, error) {
	// Not f.Fd(): it would switch the PTY to blocking mode and hang the reader goroutine on Close.
	rc, err := f.SyscallConn()
	if err != nil {
		return 0, err
	}
	var pgid int
	var ioErr error
	if err := rc.Control(func(fd uintptr) { pgid, ioErr = unix.IoctlGetInt(int(fd), unix.TIOCGPGRP) }); err != nil {
		return 0, err
	}
	return pgid, ioErr
}

func Members(pgid int) ([]int, error) {
	kps, err := unix.SysctlKinfoProcSlice("kern.proc.pgrp", pgid)
	if err != nil {
		return nil, err
	}
	pids := make([]int, len(kps))
	for i, kp := range kps {
		pids[i] = int(kp.Proc.P_pid)
	}
	slices.Sort(pids)
	return pids, nil
}

// Read falls back to the kernel's 16-byte name when argv is unreadable:
// another user's process, a zombie, or one that is mid-exec. The kernel
// never gives the env of Apple's own binaries (/bin/sh, /bin/sleep).
func Read(pid int) (Proc, error) {
	if b, err := unix.SysctlRaw("kern.procargs2", pid); err == nil {
		if argv, env := parse(b); len(argv) > 0 && argv[0] != "" {
			return Proc{Pid: pid, Argv: argv, Env: env}, nil
		}
	}
	kp, err := unix.SysctlKinfoProc("kern.proc.pid", pid)
	if err != nil {
		return Proc{}, err
	}
	return Proc{Pid: pid, Argv: []string{unix.ByteSliceToString(kp.Proc.P_comm[:])}}, nil
}

func Parent(pid int) (int, error) {
	kp, err := unix.SysctlKinfoProc("kern.proc.pid", pid)
	if err != nil {
		return 0, err
	}
	return int(kp.Eproc.Ppid), nil
}

// parse reads kern.procargs2: argc, the exec path, NUL padding, argc argv
// strings, then env strings up to an empty one. A process that rewrites its
// title in place (node's process.title) leaves the title plus NUL fill there,
// so its trailing empty argv are dropped and its env is lost.
func parse(b []byte) (argv, env []string) {
	if len(b) < 4 {
		return nil, nil
	}
	argc := int(binary.NativeEndian.Uint32(b))
	_, rest, _ := bytes.Cut(b[4:], []byte{0})
	rest = bytes.TrimLeft(rest, "\x00")
split:
	for s := range bytes.SplitSeq(rest, []byte{0}) {
		switch {
		case len(argv) < argc:
			argv = append(argv, string(s))
		case len(s) == 0:
			break split
		default:
			env = append(env, string(s))
		}
	}
	for len(argv) > 1 && argv[len(argv)-1] == "" {
		argv = argv[:len(argv)-1]
	}
	return argv, env
}
