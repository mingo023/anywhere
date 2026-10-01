package proc

import (
	"errors"
	"slices"
	"time"

	"golang.org/x/sys/unix"
)

// zombie is SZOMB from <sys/proc.h>; x/sys/unix has no name for it.
const zombie = 5

// Session lists the process groups of sid's live members. kern.proc.session
// is ENOENT on darwin and Eproc.Sess is zeroed, so it asks getsid per process.
func Session(sid int) ([]int, error) {
	kps, err := unix.SysctlKinfoProcSlice("kern.proc.all")
	if err != nil {
		return nil, err
	}
	uid := uint32(unix.Getuid())
	var pgids []int
	for _, kp := range kps {
		if kp.Proc.P_stat == zombie || kp.Eproc.Pcred.P_ruid != uid {
			continue
		}
		if s, err := unix.Getsid(int(kp.Proc.P_pid)); err != nil || s != sid {
			continue
		}
		if pg := int(kp.Eproc.Pgid); !slices.Contains(pgids, pg) {
			pgids = append(pgids, pg)
		}
	}
	return pgids, nil
}

// Orphans lists uid's live processes that launchd adopted, with their env where readable.
func Orphans(uid int) ([]Proc, error) {
	kps, err := unix.SysctlKinfoProcSlice("kern.proc.all")
	if err != nil {
		return nil, err
	}
	var ps []Proc
	for _, kp := range kps {
		if kp.Eproc.Ppid != 1 || kp.Proc.P_stat == zombie || kp.Eproc.Pcred.P_ruid != uint32(uid) {
			continue
		}
		p, err := Read(int(kp.Proc.P_pid))
		if err != nil {
			continue
		}
		if p.Sid, err = unix.Getsid(p.Pid); err != nil {
			continue
		}
		ps = append(ps, p)
	}
	return ps, nil
}

// Alive reports whether pid exists, even when another user owns it.
func Alive(pid int) bool {
	err := unix.Kill(pid, 0)
	return err == nil || errors.Is(err, unix.EPERM)
}

// Kill sends SIGTERM to each pid, waits up to grace for all to exit, then SIGKILLs the rest.
func Kill(pids []int, grace time.Duration) {
	for _, pid := range pids {
		unix.Kill(pid, unix.SIGTERM)
	}
	for deadline := time.Now().Add(grace); time.Now().Before(deadline) && slices.ContainsFunc(pids, Alive); {
		time.Sleep(50 * time.Millisecond)
	}
	for _, pid := range pids {
		if Alive(pid) {
			unix.Kill(pid, unix.SIGKILL)
		}
	}
}
