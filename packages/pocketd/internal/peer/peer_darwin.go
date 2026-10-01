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
	var pid int
	var perr error
	if err := raw.Control(func(fd uintptr) {
		pid, perr = unix.GetsockoptInt(int(fd), unix.SOL_LOCAL, unix.LOCAL_PEERPID)
	}); err != nil {
		return 0, err
	}
	return pid, perr
}
