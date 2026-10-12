//go:build !darwin

package awake

// System is the platform's sleep holder. Off macOS there is none yet:
// a headless host doesn't idle-sleep.
func System() Sleeper { return none{} }

type none struct{}

func (none) Hold(string) error { return nil }
func (none) Release() error    { return nil }
