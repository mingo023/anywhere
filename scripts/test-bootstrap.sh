#!/bin/sh
# Runs the desktop's host bootstrap (packages/desktop/crates/hosts/src/bootstrap.sh) for real on a Linux host over ssh:
# once downloading from a local HTTP server, once from an uploaded archive. Uses its own home and label, and removes both.
# Usage: scripts/test-bootstrap.sh [user@host]; default the OrbStack VM anywhere-linux.
# ANYWHERE_E2E_KEY / ANYWHERE_E2E_KNOWN_HOSTS: ssh key and known_hosts (default /tmp/anywhere-e2e/…);
# ANYWHERE_E2E_SSH_OPTS replaces all ssh options, e.g. for OrbStack's proxy (-p 32222, host mingo@anywhere-linux@127.0.0.1).
# ANYWHERE_E2E_SERVE_HOST: how the host reaches this Mac (default host.orb.internal).
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST=${1:-mingo@192.168.139.36}
KEY=${ANYWHERE_E2E_KEY:-/tmp/anywhere-e2e/id_ed25519}
KNOWN=${ANYWHERE_E2E_KNOWN_HOSTS:-/tmp/anywhere-e2e/known_hosts}
SERVE_HOST=${ANYWHERE_E2E_SERVE_HOST:-host.orb.internal}
PORT=8765
VERSION=0.0.0-e2e
HOME_DIR=.coding-pocket-e2e
LABEL=dev.mingo.anywhere.e2e.host.pocketd
DIST="$ROOT/packages/pocketd/dist"
POCKETD="POCKET_HOME=\$HOME/$HOME_DIR \$HOME/$HOME_DIR/bin/pocketd"
SSH_OPTS=${ANYWHERE_E2E_SSH_OPTS:-"-i $KEY -o IdentitiesOnly=yes -o UserKnownHostsFile=$KNOWN -o BatchMode=yes"}

# shellcheck disable=SC2086
remote() { ssh $SSH_OPTS "$HOST" "$@"; }

# bootstrap <home dir> [env...]: pipes the rendered script into sh -s on the host, as the desktop does.
bootstrap() {
  dir=$1
  shift
  sed -e "s|@@VERSION@@|$VERSION|g" -e "s|@@URL@@|http://$SERVE_HOST:$PORT|g" -e "s|@@HOME_DIR@@|$dir|g" -e "s|@@LABEL@@|$LABEL|g" \
    "$ROOT/packages/desktop/crates/hosts/src/bootstrap.sh" | remote env "$@" sh -s
}

failed=0
check() {
  name=$1
  shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failed=1; fi
}
has() { printf '%s\n' "$1" | grep -qxF "$2"; }
lacks() { ! printf '%s\n' "$1" | grep -qF "$2"; }
reports_linux() { remote "$POCKETD status --json" | grep -qF '"os": "linux"'; }
nothing_answers() {
  code=0
  remote "$POCKETD status" >/dev/null || code=$?
  [ "$code" = 3 ]
}

cleanup() {
  [ -z "${server:-}" ] || kill "$server" 2>/dev/null || true
  remote "$POCKETD service uninstall --label $LABEL; rm -rf \$HOME/$HOME_DIR \$HOME/$HOME_DIR-bad /tmp/pocketd-e2e.tar.gz" >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

machine=$(remote uname -m)
case "$machine" in
  aarch64 | arm64) arch=arm64 ;;
  *) arch=amd64 ;;
esac
"$ROOT/scripts/package-pocketd.sh" "$VERSION" "linux-$arch"
archive="$DIST/pocketd-linux-$arch.tar.gz"
python3 -m http.server --bind :: --directory "$DIST" "$PORT" >/dev/null 2>&1 &
server=$!
sleep 1

out=$(bootstrap "$HOME_DIR") || true
echo "$out"
check "a fresh host downloads, verifies and starts pocketd" has "$out" "ANYWHERE done"
check "the download was verified" has "$out" "ANYWHERE step verify"
check "the systemd user unit is active" test "$(remote systemctl --user is-active $LABEL)" = active
check "the socket answers status" remote "$POCKETD status >/dev/null"
check "status reports a linux host" reports_linux
check "the service says installed and running" has "$(remote "$POCKETD service status --label $LABEL --json")" "{\"label\":\"$LABEL\",\"installed\":true,\"running\":true,\"linger\":true}"

out=$(bootstrap "$HOME_DIR") || true
check "a second run finds pocketd running and touches nothing" has "$out" "ANYWHERE running"

remote "$POCKETD service uninstall --label $LABEL"
check "uninstall stops the unit" test "$(remote systemctl --user is-active $LABEL || true)" != active
check "uninstall leaves no unit file" remote "! test -e \$HOME/.config/systemd/user/$LABEL.service"
check "after uninstall nothing answers" nothing_answers

remote "rm -rf \$HOME/$HOME_DIR/runtime"
remote "cat > /tmp/pocketd-e2e.tar.gz" < "$archive"
sum=$(cut -d' ' -f1 "$archive.sha256")
out=$(bootstrap "$HOME_DIR" ANYWHERE_ARCHIVE=/tmp/pocketd-e2e.tar.gz ANYWHERE_SHA256="$sum") || true
echo "$out"
check "an uploaded archive installs" has "$out" "ANYWHERE done"
check "an uploaded archive skips the download" lacks "$out" "step download"

out=$(bootstrap "$HOME_DIR-bad" ANYWHERE_ARCHIVE=/tmp/pocketd-e2e.tar.gz ANYWHERE_SHA256=0000) && failed=1
check "a wrong checksum is an error" has "$out" "ANYWHERE error checksum mismatch: got $sum, want 0000"
check "a wrong checksum stops before unpacking" lacks "$out" "step unpack"

exit $failed
