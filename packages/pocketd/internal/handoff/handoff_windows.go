package handoff

import (
	"errors"
	"net"
)

var errUnsupported = errors.New("handoff: unsupported on Windows")

// Check fails: Windows has no exec to hand ptys across, so pocketd restarts instead.
func Check(File) error { return errUnsupported }

func Keep(net.Listener) (int, error)    { return 0, errUnsupported }
func Release(int)                       {}
func Inherit(int) (net.Listener, error) { return nil, errUnsupported }
