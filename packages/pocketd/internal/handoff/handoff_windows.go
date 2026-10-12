package handoff

import "errors"

// Check fails: Windows has no exec to hand ptys across, so pocketd restarts instead.
func Check(File) error { return errors.New("handoff: unsupported on Windows") }
