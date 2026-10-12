package peer

import (
	"net"
	"unsafe"

	"golang.org/x/sys/windows"
)

// sioAFUnixGetPeerPID is SIO_AF_UNIX_GETPEERPID from afunix.h, _WSAIOR(IOC_VENDOR, 256).
const sioAFUnixGetPeerPID = 0x58000100

// pidOwns is false: a same-user process on Windows can't be told from the
// owner by its pid, so the owner proves itself with a token instead.
const pidOwns = false

func PID(c *net.UnixConn) (int, error) {
	raw, err := c.SyscallConn()
	if err != nil {
		return 0, err
	}
	var pid uint32
	var n uint32
	var perr error
	if err := raw.Control(func(fd uintptr) {
		perr = windows.WSAIoctl(windows.Handle(fd), sioAFUnixGetPeerPID, nil, 0, (*byte)(unsafe.Pointer(&pid)), uint32(unsafe.Sizeof(pid)), &n, nil, 0)
	}); err != nil {
		return 0, err
	}
	return int(pid), perr
}
