#!/bin/sh
# Starts CHANGELOG.md's section for <version> from commit subjects since the last stable tag; edit it before releasing.
# Usage: scripts/changelog-draft.sh <version>
set -eu
VERSION="${1:?usage: scripts/changelog-draft.sh <version>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$ROOT/CHANGELOG.md"
if grep -qx "## $VERSION" "$LOG" 2>/dev/null; then
  echo "changelog-draft: $LOG already has ## $VERSION" >&2
  exit 1
fi
LAST="$(git -C "$ROOT" describe --tags --abbrev=0 --match 'v*' --exclude 'v*-*' 2>/dev/null || true)"
TMP="$(mktemp)"
{
  printf '# Changelog\n\n## %s\n\n' "$VERSION"
  git -C "$ROOT" log --no-merges --format='- %s' "${LAST:+$LAST..}HEAD"
  if [ -f "$LOG" ]; then
    printf '\n'
    tail -n +3 "$LOG"
  fi
} > "$TMP"
mv "$TMP" "$LOG"
echo "changelog-draft: edit ## $VERSION in $LOG"
