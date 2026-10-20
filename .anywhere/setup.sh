#!/usr/bin/env bash
# A new worktree starts with no third_party/ (gitignored, built by scripts/build-ghostty.sh)
# and an empty target/, so its first build recompiles every dependency.
# cp -c clones on APFS: instant and takes no space; cargo then rebuilds only the workspace crates.
set -euo pipefail
shopt -s nullglob
root="$ANYWHERE_ROOT_PATH"

mkdir -p third_party
for dep in "$root"/third_party/*; do
  [ -e "third_party/${dep##*/}" ] || ln -s "$dep" "third_party/${dep##*/}"
done

mkdir -p packages/desktop/target
for profile in debug release; do
  src="$root/packages/desktop/target/$profile"
  if [ -d "$src" ] && [ ! -e "packages/desktop/target/$profile" ]; then
    cp -cR "$src" packages/desktop/target/
  fi
done
