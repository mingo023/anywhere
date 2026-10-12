//go:build unix && !darwin

package shellenv

const fallbackShell = "/bin/sh"

var kept = []string{"HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK"}
