#!/bin/sh
# The no-self-approval release gate. Runs against a scratch pocketd, never the one in use.
# The residual probes (nohup, launchctl submit, TIOCSTI) are logged, not failed.
set -eu
cd "$(dirname "$0")/../packages/pocketd"
unset POCKETD_SOCK
REDTEAM_RESIDUALS=1 go test -count=1 -run '^TestRedTeamFromInsideATerminal$' -v ./e2e
