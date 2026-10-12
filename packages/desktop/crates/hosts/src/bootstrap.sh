#!/bin/sh
# Installs pocketd as a user service on a host, or finds the one already running there.
# The desktop fills the @@…@@ placeholders and pipes this into `sh -s` over ssh.
# @@URL@@ is the release's download directory; it holds pocketd-<os>-<arch>.tar.gz and its .sha256.
# ANYWHERE_ARCHIVE names a tarball already on the host to use instead (checked against ANYWHERE_SHA256 if set).
# ANYWHERE_FORCE=1 updates: installs the archive even over a running pocketd, which hands off to it in place.
# Lines for the desktop: ANYWHERE step <name> | warn <msg> | error <msg> | running | done. Others are log.
set -u
umask 077

VERSION='@@VERSION@@'
URL='@@URL@@'
DIR="$HOME/@@HOME_DIR@@"
LABEL='@@LABEL@@'

say() { echo "ANYWHERE $*"; }
fail() {
  say error "$*"
  exit 1
}

# running: whether a pocketd answers on DIR's socket, asked through the pocketd at $1.
running() {
  [ -S "$DIR/pocketd.sock" ] && [ -x "$1" ] && POCKET_HOME="$DIR" POCKETD_SOCK="$DIR/pocketd.sock" "$1" status >/dev/null 2>&1
}

# version_of: the "version" field of what the pocketd at $1 prints for the rest of the arguments.
version_of() {
  exe=$1
  shift
  POCKET_HOME="$DIR" POCKETD_SOCK="$DIR/pocketd.sock" "$exe" "$@" 2>/dev/null </dev/null | sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' | head -n 1
}

fetch() {
  if command -v curl >/dev/null; then
    curl -fsSL -o "$2" "$1"
  elif command -v wget >/dev/null; then
    wget -qO "$2" "$1"
  else
    fail "need curl or wget"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    fail "need sha256sum or shasum"
  fi
}

unpack() {
  tmp=$(mktemp -d) || fail "mktemp failed"
  trap 'rm -rf "$tmp"' EXIT
  archive=${ANYWHERE_ARCHIVE:-}
  want=${ANYWHERE_SHA256:-}
  if [ -z "$archive" ]; then
    say step download
    archive="$tmp/pocketd.tar.gz"
    fetch "$URL/pocketd-$os-$arch.tar.gz" "$archive" || fail "download $URL/pocketd-$os-$arch.tar.gz failed"
    fetch "$URL/pocketd-$os-$arch.tar.gz.sha256" "$tmp/sha256" || fail "download $URL/pocketd-$os-$arch.tar.gz.sha256 failed"
    want=$(cut -d' ' -f1 "$tmp/sha256")
  fi
  if [ -n "$want" ]; then
    say step verify
    got=$(sha256 "$archive")
    [ "$got" = "$want" ] || fail "checksum mismatch: got $got, want $want"
  fi
  say step unpack
  mkdir -p "$tmp/x" "$DIR/runtime/$VERSION" || fail "can't create $DIR/runtime/$VERSION"
  tar -xzf "$archive" -C "$tmp/x" || fail "can't unpack $archive"
  set -- "$tmp"/x/*/pocketd
  [ -f "$1" ] || fail "no pocketd in $archive"
  "$1" version --json >/dev/null 2>&1 </dev/null || fail "the pocketd in $archive doesn't run on $os-$arch"
  mv "$1" "$bin.new" && mv "$bin.new" "$bin" || fail "can't install $bin"
}

# A function so sh reads the whole script before running it: a command reading stdin can't eat the rest.
main() {
  say step detect
  case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) fail "unsupported os $(uname -s)" ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | amd64) arch=amd64 ;;
    *) fail "unsupported arch $(uname -m)" ;;
  esac
  # Under Rosetta, uname says x86_64 on Apple silicon.
  if [ "$os" = darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then arch=arm64; fi

  force=${ANYWHERE_FORCE:-}
  if [ -z "$force" ] && running "$DIR/bin/pocketd"; then
    say running
    exit 0
  fi
  bin="$DIR/runtime/$VERSION/pocketd"
  # A dev build's version never changes, so an update unpacks again.
  if [ -n "$force" ] || [ ! -x "$bin" ]; then unpack; fi
  # Another pocketd (an Anywhere app on this Mac) may hold the home; it's reused, never replaced.
  if [ -z "$force" ] && running "$bin"; then
    say running
    exit 0
  fi

  # A copy, not a symlink: the service runs this path, and a running pocketd hands off in place
  # when the file it runs from changes, so replacing it is the update.
  say step link
  mkdir -p "$DIR/bin" && cp "$bin" "$DIR/bin/pocketd.new" && mv -f "$DIR/bin/pocketd.new" "$DIR/bin/pocketd" || fail "can't install $DIR/bin/pocketd"

  say step service
  out=$(POCKET_HOME="$DIR" "$DIR/bin/pocketd" service install --label "$LABEL" --json 2>&1 </dev/null) || fail "service install: $out"
  echo "$out"
  case "$out" in
    *'"linger":false'*) say warn "linger is off, so pocketd stops when $(id -un) logs out; run: sudo loginctl enable-linger $(id -un)" ;;
  esac

  say step start
  want=$(version_of "$bin" version --json)
  i=0
  until running "$bin" && [ "$(version_of "$bin" status --json)" = "$want" ]; do
    i=$((i + 1))
    [ "$i" -le 30 ] || fail "pocketd $want didn't start in 30s; see $DIR/logs"
    sleep 1
  done
  say "done"
}

main
