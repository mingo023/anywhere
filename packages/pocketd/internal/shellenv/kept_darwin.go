package shellenv

var fallbackShells = []string{"/bin/zsh", "/bin/bash"}

var kept = []string{"HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"}
