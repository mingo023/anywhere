# Rebrand Anywhere → Zoes

## Why

Anywhere becomes Zoes (domains zoes.dev, canonical, and zoes.app, redirect). This plan amends the original handoff (`~/Downloads/HANDOFF.md`, scanned at `415de3da`) after comparing it with how Paradev renamed from Superset (`ParadevAI/Paradev`, 26 Oct – 22 Nov 2026). Where this file and the handoff disagree, this file wins.

Paradev's rule, kept here: the old name never stops working; the new one is added beside it. Bundle ids, app group, keychain ids, launchd labels, MCP server name `anywhere`, Swift/Rust/Go module and target names, and `push.getanywhere.dev` stay as the handoff lists them.

## Decisions

1. **macOS bundle.** The DMG ships `Zoes.app`; installs already at `/Applications/Anywhere.app` stay there (Sparkle updates in place). Executable `pocket-desktop`, helper `AnywhereDaemon.app`, `anywhere-notch` and bundle id stay. Finder, Dock and Cmd-Tab read Zoes through `en.lproj/InfoPlist.strings` + `LSHasLocalizedDisplayName`; Info.plist's own `CFBundleName`/`CFBundleDisplayName` stay `Anywhere`, because Finder uses the localized name only while the plist name matches the folder (verified on macOS, see Evidence). The CLI symlink (`daemon/src/cli.rs`) and the launchd plist must work from either folder. Paradev #10099.
2. **Linux/Windows.** The archive's folder becomes `Zoes/`; binaries stay `pocket-desktop` and `pocketd`. There's no auto-update on these, so Paradev's broken relaunch (#10123) doesn't apply.
3. **Release assets.** Only `Zoes.*` (`Zoes.dmg`, `.sha256`, `Zoes.md`, `Zoes-linux-amd64.tar.gz`, `Zoes-windows-amd64.zip`). Old `latest/download/Anywhere.*` links outside the repo will 404; accepted. `packages/pocketd/internal/hosts/bootstrap.sh` (and its tests) fetch `Zoes.dmg` in the same PR as the release scripts; it downloads its own version's release, so older builds keep finding `Anywhere.dmg` in theirs.
4. **App Store name.** `Zoes – Agent Remote`, by removing 1.0 from review (handoff §3 option B). See "Dashboard changes".
5. **Data dir.** `~/.anywhere` → `~/.zoes` in this rebrand, chained after the existing `~/.coding-pocket` move (`config.MoveLegacyHome`, `channel.rs` `legacy_home_dir_name`). See "Data dir move".
6. **Repo config.** `.zoes/` is read first, `.anywhere/` when only it holds config; new config is written to `.zoes/`. A repo configured in `.anywhere/` keeps being edited there. Paradev #9248.
7. **Env vars.** Terminal and worktree variables (`ANYWHERE_ROOT_PATH`, `ANYWHERE_WORKSPACE_PATH`, `ANYWHERE_WORKSPACE_NAME`, and any other a user script or agent sees) are also set as `ZOES_*`. Readers take `ANYWHERE_*` when set and non-empty, else `ZOES_*`, so existing overrides keep winning. Build, CI and host-install variables stay `ANYWHERE_*`. Docs lead with `ZOES_*`. Paradev #10136.
8. **URL scheme.** The next iOS build registers and parses `zoes://` beside `anywhere://`. pocketd keeps emitting `anywhere://` until an iOS build that knows `zoes://` is live on the store, then switches. iOS 1.0 (In Review) and desktop 0.1.63/0.1.64 know only `anywhere://`. Paradev #10161, #10160.
9. **Agent skill.** `pocketd skill install` writes `skills/zoes/` and removes the `skills/anywhere/` it installed before (both `~/.claude/skills` and `~/.agents/skills`). The skill teaches `zoes` and says `anywhere` still works. MCP server stays `anywhere` so `mcp__anywhere__*` allowlists keep working.
10. **CLI.** `/usr/local/bin/zoes` and `/usr/local/bin/anywhere` both link to the bundled pocketd; `cmd/pocketd/main.go` treats `zoes` and `anywhere` as CLI mode. Paradev #10137.

## Data dir move

Reuse `MoveLegacyHome` (rename under the home lock, leave a symlink at the old path; Windows keeps its old home). Order: `~/.coding-pocket` → `~/.zoes` directly when it still exists, else `~/.anywhere` → `~/.zoes`.

- Dev home is nested: `~/.anywhere/dev` becomes `~/.zoes/dev`, moving with the release home.
- Rewrite absolute paths stored in `state/pocketd.db` and `desktop.json`.
- Worktrees under `~/.anywhere/worktrees`: run `git worktree repair` per repo, skipping paths whose parent no longer exists (Paradev #9265: one missing parent aborts the repair for every later path).
- Claude/Codex session folders are keyed by cwd: link the old keys to the new paths so `/resume` keeps history (Paradev #9158).
- Launchd plist, agent hook configs and MCP configs that name `~/.anywhere` paths: rewrite or rely on the symlink; list each.
- Host homes over SSH (`hosts/src/remote.rs` `pocket_dir`) follow the same rule on the host's next update.
- If any step fails, that run keeps the old home and the next start retries.

## PRs

One `type:` label each; Minh merges.

1. **Site, docs, blog, legal** (`type: docs`). Handoff §1. GitHub links stay `mingo023/anywhere` until the repo rename. Download links move to `Zoes.*` only once a release carries them (land with or after PR 3).
2. **Display name + CLI** (`type: feature`). Decisions 1, 9, 10; handoff §4: `InfoPlist.strings` (already drafted in `scripts/release-mac.sh`), `Zoes.app` in the DMG, menu and window title in `pocket/src/main.rs`, helper display name, push titles in `push/notice.go` / `activity.go`, iOS `CFBundleDisplayName` and permission strings, README.
3. **Release assets** (`type: internal`). Decisions 2, 3: release scripts, workflows, `bootstrap.sh` + tests, ADR 0004.
4. **Data dir** (`type: feature`). Decision 5, its own PR.
5. **Repo config + env vars** (`type: feature`). Decisions 6, 7.
6. **iOS scheme** (`type: feature`). Decision 8, ships with the first iOS build after 1.0; emitting `zoes://` is a later follow-up.
7. **After Minh's `gh repo rename zoes`** (`type: internal`). `REPO=mingo023/zoes`, remaining GitHub URLs. Keep `SUFeedURL` on the old repo URL (it redirects).

## State on 2026-11-26 (read in the dashboards, nothing changed yet)

Cloudflare:

- Zones `zoes.dev` and `zoes.app` exist and are active (Free plan), with no DNS records, so neither resolves.
- Worker `anywhere` (production) has one custom domain, `getanywhere.dev`. No worker routes, no page rules.
- `getanywhere.dev` zone: `www` CNAME to the apex, apex `AAAA 100::` (proxied), a Google site-verification TXT, and one redirect rule: `https://www.*` → `https://${1}`, 301.
- **`push.getanywhere.dev` has no DNS record and doesn't resolve: the push relay was never deployed.** `DefaultPushRelay` and iOS `RelayClient.defaultBase` point at a host that doesn't exist. Wherever the relay is deployed later, it must answer on `push.getanywhere.dev` (baked into iOS 1.0 and desktop ≤ 0.1.64); `push.zoes.dev` can be a second name.

App Store Connect, app 6826877200:

- 1.0 In Review, set to release automatically after approval, no earlier than 26 Nov 2026 21:00.
- Name `Anywhere – Agent Remote` and subtitle are locked while in review; promotional text is editable.
- Support URL `https://getanywhere.dev/support`, marketing URL `https://getanywhere.dev`. Promotional text, description and review notes say "Anywhere" and link getanywhere.dev and `mingo023/anywhere`.

## Dashboard changes (decided, not done)

Do these from the dashboards; the dashboard API refuses scripted writes (403).

Cloudflare, all at once (Minh chose not to wait for PR 1, accepting that zoes.dev serves Anywhere copy and a getanywhere.dev canonical until it deploys):

1. Worker `anywhere` → Settings → Domains & Routes → add custom domain `zoes.dev`.
2. `zoes.dev` zone: proxied `www` record and a redirect rule `www.zoes.dev/*` → `https://zoes.dev/${1}`, 301, query kept.
3. `zoes.app` zone: proxied apex and `www` records (`AAAA 100::`), redirect rule for both hosts → `https://zoes.dev/<same path>`, 301, query kept.
4. `getanywhere.dev` zone: replace the `www` rule with one rule for `getanywhere.dev` and `www.getanywhere.dev` → `https://zoes.dev/<same path>`, 301, query kept. Redirect rules run before the worker, so the existing custom domain can stay. Keep the site-verification TXT.
5. Check: `curl -sI` on `/`, `/docs/install`, `/blog/…`, `/privacy`, `/support` for each old host shows a single 301 to the same zoes.dev path; zoes.dev returns 200.

App Store Connect (Minh chose option B, remove 1.0 from review):

1. Remove 1.0 from review (loses the queue place; cancels tonight's scheduled release).
2. App Information: name `Zoes – Agent Remote`; subtitle unchanged unless it names Anywhere. Privacy URL → `https://zoes.dev/privacy`.
3. Version 1.0: support `https://zoes.dev/support`, marketing `https://zoes.dev`; promotional text, description and review notes say Zoes; screenshots that show "Anywhere" redone.
4. Resubmit.

Risk to settle before step 1: the build in review still shows "Anywhere" under its icon. Guideline 2.3.8 asks the store name and the on-device name to be similar, and the description walks the reviewer through "Anywhere → Settings → Phone" on a Mac app still named Anywhere. Resubmitting the same build may be rejected. Safer: submit a new build with `CFBundleDisplayName` Zoes (from PR 2) and a desktop release whose UI says Zoes, or keep "Anywhere" in the description where it names the Mac app.

## Minh

- Search Console / Bing for zoes.dev, submit the sitemap.
- `gh repo rename zoes`, homepage `https://zoes.dev`.
- Closing tagline on the landing page ("Take your agents anywhere") needs a new line.

## Evidence

Finder display name (`osascript` `displayed name`), macOS, system language en-VN, unsigned test bundles with the same Info.plist keys; the drafted `assemble` step was then run and showed Zoes for both folders:

| Folder | Info.plist display name | InfoPlist.strings | Finder shows |
|---|---|---|---|
| Anywhere.app | Zoes | no | Anywhere |
| Anywhere.app | Anywhere | Zoes | **Zoes** |
| Anywhere.app | Zoes | Zoes | Anywhere |
| Zoes.app | Anywhere | Zoes | **Zoes** |

Not yet checked: other system languages (they should fall back to `en` through `CFBundleDevelopmentRegion`), a real signed release build, and the helper's name in Login Items.

## Out of scope

New icon or mark, renaming `AnywhereKit` / Xcode targets / crates / the Cloudflare worker, bundle id or launchd label changes, `push.zoes.dev`, Android, Homebrew.
