#!/bin/sh
# Checks that the flags the desktop spawns agents with still parse. Every probe stops at argument
# parsing with stdin closed, so none starts a turn. Exits 1 on any drift; a missing CLI is skipped.
set -eu
DRIFT=0

drift() {
  echo "probe: DRIFT $1: $2"
  DRIFT=1
}

# claude checks --permission-mode before it rejects the unknown sentinel, so reaching the sentinel
# means the mode parsed. `--version` can't probe: it exits 0 whatever the mode.
claude_parses() {
  err=$(claude -p --permission-mode "$1" --pocket-probe </dev/null 2>&1 >/dev/null) || true
  case $err in *"unknown option '--pocket-probe'"*) return 0 ;; esac
  printf '%s\n' "$err" | head -n 1
  return 1
}
claude_mode() { why=$(claude_parses "$1") || drift "claude --permission-mode $1" "$why"; }
claude_bad_mode() {
  if claude_parses "$1" >/dev/null; then drift "claude --permission-mode $1" "accepted; the probe can't see a bad mode"; fi
}

# clap validates every value before `--version` answers.
codex_parses() {
  err=$(codex "$@" --version </dev/null 2>&1 >/dev/null) && return 0
  printf '%s\n' "$err" | head -n 1
  return 1
}
codex_ok() { why=$(codex_parses "$@") || drift "codex $*" "$why"; }
codex_bad() {
  if codex_parses "$@" >/dev/null; then drift "codex $*" "accepted; the probe can't see a bad value"; fi
}
# claude only warns about an unknown --effort and runs at its default, so a warning is drift too.
claude_flags() {
  err=$(claude -p "$@" --pocket-probe </dev/null 2>&1 >/dev/null) || true
  case $err in
    *Warning:*) ;;
    *"unknown option '--pocket-probe'"*) return 0 ;;
  esac
  drift "claude $*" "$(printf '%s\n' "$err" | head -n 1)"
}

versions=""
if command -v claude >/dev/null 2>&1; then
  versions="claude $(claude --version </dev/null | cut -d' ' -f1)"
  claude_mode default
  claude_mode acceptEdits
  claude_mode plan
  claude_mode auto
  claude_flags --permission-mode bypassPermissions --allow-dangerously-skip-permissions
  claude_flags -n calm-otter --model opus --effort high
  claude_flags --effort xhigh
  claude_flags --effort max
  claude_bad_mode pocket-bogus
else
  echo "probe: skip claude (not on PATH)"
fi
if command -v codex >/dev/null 2>&1; then
  versions="${versions:+$versions · }$(codex --version </dev/null)"
  codex_ok -s read-only -a on-request
  codex_ok -s workspace-write -a on-request
  codex_ok -s danger-full-access -a never
  codex_ok --approve-for-me
  codex_ok -m gpt-5
  codex_ok -c model_reasoning_effort=high
  codex_bad -a untrusted
  codex_bad --full-auto
  help=$(codex resume --help </dev/null 2>&1) || true
  for f in --sandbox --ask-for-approval; do
    case $help in *"$f"*) ;; *) drift "codex resume $f" "not in codex resume --help" ;; esac
  done
  # -c values aren't validated at parse time, so the efforts are shown, never gated.
  efforts=$(codex debug models --bundled </dev/null 2>/dev/null | grep -o '"effort":"[a-z]*"' | cut -d'"' -f4 | awk '!seen[$0]++' | paste -sd ' ' -)
  echo "probe: codex efforts: ${efforts:-unknown}"
else
  echo "probe: skip codex (not on PATH)"
fi
[ -z "$versions" ] || echo "probe: $versions"
exit "$DRIFT"
