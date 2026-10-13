#!/bin/sh
# Starts CHANGELOG.md's section for <version> from the PRs merged since the last stable tag, grouped by their
# `type:` labels (.github/release.yml); edit it before releasing. HEAD must be pushed: GitHub reads the range itself.
# Usage: scripts/changelog-draft.sh <version>
set -eu
VERSION="${1:?usage: scripts/changelog-draft.sh <version>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$ROOT/CHANGELOG.md"
REPO=mingo023/anywhere
if grep -qx "## $VERSION" "$LOG" 2>/dev/null; then
  echo "changelog-draft: $LOG already has ## $VERSION" >&2
  exit 1
fi
LAST="$(git -C "$ROOT" describe --tags --abbrev=0 --match 'v*' --exclude 'v*-*' 2>/dev/null || true)"
NOTES="$(gh api "repos/$REPO/releases/generate-notes" -f tag_name="v$VERSION" \
  -f target_commitish="$(git -C "$ROOT" rev-parse HEAD)" ${LAST:+-f previous_tag_name="$LAST"} --jq .body)"
TMP="$(mktemp)"
{
  printf '# Changelog\n\n## %s\n\n' "$VERSION"
  # Drop GitHub's comment and top heading, and nest its other ## headings, so release-mac.sh's section ends at the next version.
  printf '%s\n' "$NOTES" | sed -e '/^<!--.*-->$/d' -e "/^## What's Changed$/d" -e 's/^## /### /' | sed '/./,$!d'
  if [ -f "$LOG" ]; then
    printf '\n'
    tail -n +3 "$LOG"
  fi
} > "$TMP"
mv "$TMP" "$LOG"
echo "changelog-draft: edit ## $VERSION in $LOG"
