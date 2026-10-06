# Anywhere ships as a notarized universal DMG on GitHub Releases and updates through Sparkle

Users download `Anywhere.dmg` from a landing page whose button points at `github.com/mingo023/anywhere/releases/latest/download/Anywhere.dmg`. One script, `scripts/release-mac.sh`, builds a universal (arm64 + x86_64) `Anywhere.app`, signs it with a Developer ID under the hardened runtime (no entitlements needed), notarizes and staples the DMG, signs the update with Sparkle's EdDSA key, writes `appcast.xml` and uploads both to the release. It runs on the maintainer's Mac first and later unchanged in a tag-triggered GitHub Actions workflow, free for a public repo. Updates use Sparkle 2 (MIT) rather than a Zed-style updater of our own, because strangers' machines hit the cases Sparkle already handles: an app run from Downloads, /Applications without write access, a bad signature. The cost is a bundled framework and an `objc2` bridge.

`pocketd` ships inside the app as `Contents/Library/Helpers/AnywhereDaemon.app`, a bundle named "Anywhere", so privacy prompts from its terminals say "Anywhere" rather than "pocketd". The app registers it with `SMAppService.agent` on first launch, without a welcome screen; if the user turns it off in Login Items, the "Not starting?" state offers to open Login Items. This needs macOS 13. A moved or deleted app takes its daemon along, which a LaunchAgent plist with an absolute path would not.

## Rules

- The git tag `vX.Y.Z` is the only version. The script injects it into `Info.plist` (`CFBundleShortVersionString` and `CFBundleVersion`), the app (`ANYWHERE_VERSION`) and `pocketd` (`-ldflags -X main.version`). Untagged builds say `dev`.
- One stable channel. A pre-release tag publishes a DMG but stays out of the appcast.
- Release notes are a `CHANGELOG.md` section, drafted from commit subjects since the last tag and edited before release. The script refuses a tag without one.
- The Sparkle private key lives outside the repo (1Password and a CI secret). Losing it strands every installed copy.
- `SUFeedURL` is `…/releases/latest/download/appcast.xml` and never changes, since every shipped build has it baked in.
- Release and Dev are separate apps that run side by side:

| | Release | Dev |
|---|---|---|
| Bundle ID | `dev.mingo.anywhere`, never renamed | `dev.mingo.anywhere.dev` |
| Name | Anywhere | Anywhere Dev |
| Daemon label | `dev.mingo.anywhere.pocketd` | `dev.mingo.anywhere.dev.pocketd` |
| Home | `~/.coding-pocket` | `~/.coding-pocket-dev` |
| Updates | Sparkle feed | none |

## Update flow

Sparkle checks at launch and every 6h, then downloads and verifies in the background without UI. Once the update is ready, the sidebar shows "Restart to update"; clicking it installs and relaunches, and ignoring it installs at the next quit. After a relaunch on a new version, the sidebar shows a dismissible "What's new in X" card. "Check for Updates…" in the app menu and in Settings uses Sparkle's standard dialogs. Failed automatic checks stay silent. The updated daemon takes over without interrupting anything (ADR 0005), so unlike MonoCode there is no "chats still running, quit anyway?" prompt.
