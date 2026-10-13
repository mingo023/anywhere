//go:build !darwin && !linux && !windows

package service

import (
	"fmt"
	"runtime"
)

var errUnsupported = fmt.Errorf("pocketd service: not supported on %s", runtime.GOOS)

func Install(Spec) error { return errUnsupported }

func Uninstall(Spec) error { return errUnsupported }

func Status(Spec) (installed, running bool, err error) { return false, false, errUnsupported }
