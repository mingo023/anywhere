//go:build !windows

package service

import "errors"

var errUnsupported = errors.New("service: only Windows is supported on this branch")

func Install(Spec) error              { return errUnsupported }
func Uninstall(Spec) error            { return errUnsupported }
func Status(Spec) (bool, bool, error) { return false, false, errUnsupported }
