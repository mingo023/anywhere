---
name: release
description: Release the latest main as the next patch version — write its CHANGELOG.md section, push it to main and run the Release workflow, without asking questions. Only runs when invoked as /release.
disable-model-invocation: true
---

# Release

Invoking /release is the approval: write the changelog, push it to `main` and start the release without asking. Stop only if there is nothing to release, or a git/gh command fails in a way you can't fix.

`.github/workflows/release.yml` tags `main` as `v<version>` and dispatches the macOS, Linux/Windows and iOS builds. It fails unless `CHANGELOG.md` on `main` has a `## <version>` section, so the changelog commit lands first.

## Steps

1. **Latest:** `git fetch --tags origin`. Everything below reads `origin/main`, never the local branch.
2. **Version:** the version given after /release, else the latest stable tag (`git tag --sort=-v:refname`, skipping `-` pre-releases) with its patch bumped: `v0.1.25` → `0.1.25` → `0.1.26`.
3. **Work tree:** don't touch the current checkout; it may hold someone's work. `git worktree add --detach /tmp/release-<version> origin/main` and do steps 4–6 there.
4. **Draft:** `scripts/changelog-draft.sh <version>`. It adds one `- <subject>: (<hash>)` line per first-parent commit since the last stable tag.
5. **Prune:** drop lines that don't reach a user, matching `.github/release.yml`:
   - a PR (`(#N)` in the subject) labelled `type: internal`: check with `gh pr view N --json labels`;
   - a commit with no `(#N)`: direct pushes such as `Changelog for …` or skill/tooling tweaks.

   Keep the other lines as they are, in the script's order. If no line is left, remove the work tree and report there's nothing to release.
6. **Push:** `git commit -am "Changelog for <version>"`, then `git push origin HEAD:main`. If the push is rejected because `main` moved, `git pull --rebase origin main` and push again.
7. **Release:** `gh workflow run release.yml --ref main -f version=<version>`. Find its run with `gh run list --workflow release.yml -L 1` and `gh run watch <id> --exit-status`; it only tags, so it takes seconds. If it fails, show the failing log (`gh run view <id> --log-failed`).
8. **Clean up:** `git worktree remove /tmp/release-<version>`.
9. **Report:** the version and tag, the changelog lines, any lines dropped and why, and the URLs of the dispatched `Release macOS`, `Release Linux and Windows` and `Release iOS` runs (`gh run list -L 5`, those on `v<version>`). Don't wait for those builds; they take a while. If recent macOS runs failed because no runner picked the job up, say the macOS/iOS release may need a rerun.
