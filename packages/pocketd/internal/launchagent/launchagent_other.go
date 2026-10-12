//go:build !darwin

package launchagent

import "errors"

var errNoLaunchd = errors.New("pocketd daemon needs launchd (macOS); run pocketd serve under your own supervisor")

func Loaded(int) (bool, error) { return false, nil }

func Service(int, string) string { return "none" }

func Install(Spec, string, int) (bool, error) { return false, errNoLaunchd }

func Uninstall(string, int) error { return errNoLaunchd }
