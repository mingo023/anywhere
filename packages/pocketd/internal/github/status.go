package github

import "strings"

// Status is whether gh is installed and signed in to github.com: State is
// missing, signedOut or ready, Login the active one of Accounts.
type Status struct {
	State, Version, Login string
	Accounts              []string
}

var (
	versionArgv = []string{"gh", "--version"}
	authArgv    = []string{"gh", "auth", "status", "--hostname", "github.com"}
)

// status reads what versionArgv and authArgv printed, auth's stderr merged
// into its output.
func status(version string, versionErr error, auth string, authErr error) Status {
	fields := strings.Fields(version)
	if versionErr != nil || len(fields) < 3 {
		return Status{State: string(Missing)}
	}
	s := Status{State: string(SignedOut), Version: fields[2]}
	if authErr != nil {
		return s
	}
	if all, active := accounts(auth); active != "" {
		s.State, s.Login, s.Accounts = "ready", active, all
	}
	return s
}

// accounts is every github.com account and the active one: newer gh lists
// each and marks one active, older gh says "as <login>".
func accounts(auth string) (all []string, active string) {
	for l := range strings.Lines(auth) {
		l = strings.TrimSpace(l)
		_, rest, ok := strings.Cut(l, "Logged in to github.com account ")
		if !ok {
			_, rest, ok = strings.Cut(l, "Logged in to github.com as ")
		}
		switch {
		case ok:
			if f := strings.Fields(rest); len(f) > 0 {
				all = append(all, f[0])
			}
			if strings.Contains(l, " as ") && len(all) > 0 {
				active = all[len(all)-1]
			}
		case strings.Contains(l, "Active account: true") && len(all) > 0:
			active = all[len(all)-1]
		}
	}
	return all, active
}

// unauthorized: GitHub refused the token, as it does one revoked by signing in again.
func unauthorized(stderr string) bool { return strings.Contains(stderr, "HTTP 401") }
