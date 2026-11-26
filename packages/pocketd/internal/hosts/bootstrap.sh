#!/bin/sh
# Installs pocketd as a user service on a host, or finds the one already running there.
# The desktop fills the @@…@@ placeholders and pipes this into `sh -s` over ssh.
# @@URL@@ is the release's download directory; it holds pocketd-<os>-<arch>.tar.gz and Anywhere.dmg, each with its .sha256.
# ANYWHERE_ARCHIVE names a tarball already on the host to use instead, checked against ANYWHERE_SHA256.
# Downloads are https only; ANYWHERE_ALLOW_HTTP=1 lets scripts/test-bootstrap.sh serve them from this Mac.
# ANYWHERE_FORCE=1 updates: installs the archive even over a running pocketd, which hands off to it in place.
# When the Anywhere app on a Mac host runs it, the update replaces that app with the release's Anywhere.dmg instead.
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

# app_owned: whether the pocketd answering on DIR's socket, asked through the pocketd at $1, runs from somewhere other
# than DIR/bin/pocketd, as an Anywhere app's does; replacing bin/pocketd never reaches it. It sets from to that path.
# Only a Mac runs the app. Its ps prints argv[0], which launchd and a handoff set to the full path, with HOME's
# symlinks resolved, so -ef compares files rather than paths.
app_owned() {
  [ "$os" = darwin ] || return 1
  pid=$(POCKET_HOME="$DIR" POCKETD_SOCK="$DIR/pocketd.sock" "$1" status --json 2>/dev/null </dev/null | sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' | head -n 1)
  [ -n "$pid" ] && from=$(ps -o comm= -p "$pid") && [ -n "$from" ] && ! [ "$from" -ef "$DIR/bin/pocketd" ]
}

# version_of: the "version" field of what the pocketd at $1 prints for the rest of the arguments.
version_of() {
  exe=$1
  shift
  POCKET_HOME="$DIR" POCKETD_SOCK="$DIR/pocketd.sock" "$exe" "$@" 2>/dev/null </dev/null | sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' | head -n 1
}

# update_app swaps in the release's Zoes.app for the one whose pocketd is $from. That pocketd sees its file change
# and hands off to the new one in place, as after a Sparkle update.
update_app() {
  app=${from%/Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd}
  if [ "$app" = "$from" ] || [ -z "$URL" ] || [ "$(plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist" 2>/dev/null)" != dev.mingo.anywhere ]; then
    fail "pocketd here runs from $from, which an update can't replace; update the Anywhere app on this Mac instead"
  fi
  fetch "$URL/Anywhere.dmg" "$tmp/Anywhere.dmg" || fail "download $URL/Anywhere.dmg failed"
  fetch "$URL/Anywhere.dmg.sha256" "$tmp/dmg.sha256" || fail "download $URL/Anywhere.dmg.sha256 failed"
  want=$(cut -d' ' -f1 "$tmp/dmg.sha256")
  got=$(sha256 "$tmp/Anywhere.dmg")
  [ "$got" = "$want" ] || fail "checksum mismatch for Anywhere.dmg: got $got, want $want"
  mkdir "$tmp/mnt" && hdiutil attach -quiet -nobrowse -readonly -noautoopen -mountpoint "$tmp/mnt" "$tmp/Anywhere.dmg" || fail "can't open Anywhere.dmg"
  rm -rf "$app.new" "$app.old"
  out=$(ditto "$tmp/mnt/Zoes.app" "$app.new" 2>&1)
  copied=$?
  hdiutil detach -quiet "$tmp/mnt"
  [ "$copied" = 0 ] || fail "can't copy Zoes.app beside $app: $out"
  if [ "$(version_of "$app.new/Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd" version --json)" != "$VERSION" ]; then
    rm -rf "$app.new"
    fail "Anywhere.dmg doesn't hold pocketd $VERSION"
  fi
  out=$(mv "$app" "$app.old" 2>&1) || {
    rm -rf "$app.new"
    fail "can't replace $app: $out"
  }
  out=$(mv "$app.new" "$app" 2>&1) || {
    mv "$app.old" "$app"
    rm -rf "$app.new"
    fail "can't replace $app: $out"
  }
  rm -rf "$app.old"
}

wait_for_version() {
  say step start
  i=0
  until running "$bin" && [ "$(version_of "$bin" status --json)" = "$1" ]; do
    i=$((i + 1))
    [ "$i" -le 30 ] || fail "pocketd $1 didn't start in 30s; see $DIR/logs"
    sleep 1
  done
  say "done"
}

fetch() {
  proto='=https'
  case "$1" in
    https://*) ;;
    *) [ -n "${ANYWHERE_ALLOW_HTTP:-}" ] && proto='=http,https' || fail "won't download $1: not https" ;;
  esac
  if command -v curl >/dev/null; then
    curl --proto "$proto" --proto-redir "$proto" --tlsv1.2 -fsSL -o "$2" "$1"
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
  say step verify
  [ -n "$want" ] || fail "no checksum to verify $archive against"
  got=$(sha256 "$archive")
  [ "$got" = "$want" ] || fail "checksum mismatch: got $got, want $want"
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

  say step link
  if [ -n "$force" ] && running "$bin" && app_owned "$bin"; then
    update_app
    wait_for_version "$VERSION"
    exit 0
  fi
  # A copy, not a symlink: the service runs this path, and a running pocketd hands off in place
  # when the file it runs from changes, so replacing it is the update.
  mkdir -p "$DIR/bin" && cp "$bin" "$DIR/bin/pocketd.new" && mv -f "$DIR/bin/pocketd.new" "$DIR/bin/pocketd" || fail "can't install $DIR/bin/pocketd"

  say step service
  out=$(POCKET_HOME="$DIR" "$DIR/bin/pocketd" service install --label "$LABEL" --json 2>&1 </dev/null) || fail "service install: $out"
  echo "$out"
  case "$out" in
    *'"linger":false'*) say warn "linger is off, so pocketd stops when $(id -un) logs out; run: sudo loginctl enable-linger $(id -un)" ;;
  esac

  wait_for_version "$(version_of "$bin" version --json)"
}

main
