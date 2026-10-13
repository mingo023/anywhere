//go:build unix && !darwin

package shellenv

var fallbackShells = []string{"/bin/bash"}

var kept = []string{"HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK"}
