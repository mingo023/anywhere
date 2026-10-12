package peer

import (
	"net"

	"golang.org/x/sys/unix"
)

func PID(c *net.UnixConn) (int, error) {
	raw, err := c.SyscallConn()
	if err != nil {
		return 0, err
	}
	var cred *unix.Ucred
	var perr error
	if err := raw.Control(func(fd uintptr) {
		cred, perr = unix.GetsockoptUcred(int(fd), unix.SOL_SOCKET, unix.SO_PEERCRED)
	}); err != nil {
		return 0, err
	}
	if perr != nil {
		return 0, perr
	}
	return int(cred.Pid), nil
}
