# 19 — Install, first run and distribution for Pocket (Mac app + pocketd + phone)

Date: 2026-09-30.

Sources:
- Zeron `zeronsh/comet` @ ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (tag v0.2.99, MIT). GitHub release v0.2.99 via `gh api` (published 2026-09-30).
- MonoCode `hardbeat920/monocode` @ cdc1441dc51e3709cd843e5c316608a123f323c6 (v0.5.0, MIT). GitHub release v0.5.0 via `gh api` (2026-09-29).
- Pocket worktree @ b9d14a19 (the task said 86deb13; HEAD is b9d14a1).
- Earlier reports r01, r02, r08, r09, r14, r15 in this folder.
- Apple:
  - https://developer.apple.com/documentation/servicemanagement/smappservice (plus `/agent(plistname:)`, `/register()`, `/status-swift.enum/requiresapproval`, read via the docs JSON)
  - https://developer.apple.com/documentation/servicemanagement/updating-your-app-package-installer-to-use-the-new-service-management-api
  - https://developer.apple.com/help/account/reference/supported-capabilities-ios/
  - https://developer.apple.com/support/compare-memberships/
  - https://developer.apple.com/programs/enroll/
  - https://developer.apple.com/testflight/
  - https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/
  - https://support.apple.com/en-us/102445
- Expo:
  - https://docs.expo.dev/push-notifications/sending-notifications/
  - https://docs.expo.dev/push-notifications/faq/
  - https://docs.expo.dev/push-notifications/push-notifications-setup/
  - https://expo.dev/pricing
- Crates: https://crates.io/crates/objc2-service-management

Citation legend:
- `Z path:L` = Zeron clone, path relative to the clone root.
- `M path:L` = MonoCode clone.
- `P path:L` = Pocket worktree root.
- `rNN:L` = report NN in `docs/orchestrators/research/`, at line L.
- Bare URLs are official vendor pages.
- "Inference" marks a claim no source states directly.
- "code≠doc" marks a disagreement between code and docs; code wins.

## TL;DR

- **A non-author can't run Pocket today.** The path is 12 manual steps (F7):
  1. Build Go, Rust, Zig 0.16.0 and Ghostty from source.
  2. Keep `pocketd serve` open in a terminal.
  3. Re-sign the phone app away from the author's Team ID.
  4. Type the host and token by hand. A wrong token shows "Disconnected — retrying" forever.
- **Ship one signed, notarized, stapled `Anywhere.app`.** `Contents/MacOS/pocket` and `Contents/MacOS/pocketd` both link libghostty-vt as a static `.a` (P packages/desktop/crates/term/build.rs:2-14; P packages/pocketd/internal/vt/vt.go:4-5), so there is no dylib or framework to sign. Port Zeron's `package-macos.sh` and `release.yml` nearly line for line.
- **The app registers pocketd as a LaunchAgent** through `SMAppService.agent(plistName:)` (macOS 13+). The plist ships in `Contents/Library/LaunchAgents`. It must never boot out a running pocketd, because that kills every Terminal. MonoCode gets this right (M host/service.ts:178,216-222); Zeron's `daemon install` boots out first (Z apps/zeron/src/daemon.rs:46).
- **Updater: port Zeron's Rust `crates/update`.** Zeron's desktop is also GPUI. Keep its manifest + sha256, staged swap, relaunch-after-exit and install-on-quit. Add a codesign designated-requirement check, which Zeron lacks.
- **pocketd restart policy:**
  - An app update never restarts pocketd. The running process keeps the old binary.
  - pocketd restarts itself into the new binary only when 0 Terminals are open, at most once per version (Zeron's quiescence gate, Z crates/engine/src/lib.rs:817-824).
  - Otherwise the user presses "Restart — closes N terminals".
  - This needs protocol range negotiation. Today the hello check requires exactly `== 3` (P packages/pocketd/internal/wsserver/wsserver.go:122).
- **Push is settled: Expo Push (02-2 stands; 08-1 is dropped).**
  - The APNs key stays with the publisher in EAS. pocketd stores only `ExponentPushToken`s and POSTs to `exp.host`.
  - Expo push is free and keeps no content after delivery.
  - The same API covers Android, which answers r08's FCM question.
  - Direct APNs would need the publisher's `.p8` on every user's Mac.
- **Phone distribution:** TestFlight external testing with a public link (≤10,000 testers, builds expire after 90 days, the first build needs beta review), built with EAS (15 free iOS builds/mo). Free Apple accounts can't use the Push capability, so personal signing means a paid team plus the builder's own EAS project.
- **First run: 8 Mac screens and 4 phone screens (F1-F2).**
  - No Pocket account.
  - No hook consent screen, because the Claude plugin goes in through env and no user file changes (P packages/pocketd/internal/daemon/plugin.go:65-82).
  - No login UI, because Claude and Codex sign in inside their own Terminal.
- **Licence: add MIT now.** That matches Zeron, MonoCode and Ghostty, and lets Pocket port their code if the notices are kept. Price: free, no account. The fixed cost is Apple's $99/yr.

## Findings

### F1. Mac first-run storyboard (screens, copy, state)

**M1. Download.**
- Artifacts:
  - Zeron v0.2.99: `zeron-0.2.99-macos-arm64.dmg` (40,489,782 B), plus `…-macos-arm64-app.tar.gz` (34,780,985 B) for the updater, plus `manifest.json` `{version, files:{<name>:{sha256}}}` (gh api release v0.2.99).
  - MonoCode: `MonoCode_0.5.0_aarch64.dmg` 13,963,539 B and `MonoCode.app.tar.gz.sig` 408 B (gh api release v0.5.0).
- DMG window spec in UI/UX.

**M2. Wrong-location gate.**
- Trigger: the path contains `AppTranslocation`, is under `/Volumes`, or the parent isn't writable (Z crates/update/src/lib.rs:474-512).
- Copy (adapted from Z crates/update/src/lib.rs:375-377): "Pocket is running from a temporary, read-only location. Move Pocket to your Applications folder and reopen it." Button: "Quit".
- Zeron uses this only to block updates. Pocket must also block service registration, because the LaunchAgent is tied to the bundle location (Apple `agent(plistName:)`: the plist must sit in the calling app's `Contents/Library/LaunchAgents`).

**M3. Terminal service (pocketd).**
- Action:
  1. Call `SMAppService.agent(plistName: "dev.mingo.anywhere.pocketd.plist").register()`.
  2. Poll `~/.coding-pocket/pocketd.sock` 50×200 ms, as MonoCode does (M host/service.ts:262-265).
- Apple: a registered LaunchAgent "is immediately bootstrapped and may begin running", and again at each login (`register()` doc). Status `requiresApproval` means the user must act in System Settings, including after revoking consent there.
- States and copy:
  - Starting: "Starting Pocket's terminal service…"
  - `requiresApproval`: title "Allow Pocket in the background". Body: "Pocket's terminal service keeps your terminals and agents running when this window is closed, so your phone can reach them." Button: "Open Login Items".
  - Did not start: "Pocket's terminal service didn't start. Check ~/.coding-pocket/logs/pocketd.log." Button: "Retry". Adapted from M host/service.ts:273 "The host service was installed but did not start. Check {log}."; Retry style from Z crates/ui/src/shell.rs:11395-11430.
  - Already running: reuse it and send no kill. MonoCode: "Never replace a running host: connecting must not interrupt agent turns." (M host/service.ts:178). For a loaded service it runs `kickstart` without `-k` (M host/service.ts:216-222).
- Contrast with Zeron:
  - The Mac app never installs the LaunchAgent; `zeron daemon install` is CLI-only (Z apps/zeron/src/main.rs:272).
  - The headed app embeds the engine when no daemon listens (Z apps/zeron/src/main.rs:275-293).
  - Pocket can't do either, because the phone needs pocketd while the window is closed.
- Today Pocket fails hard:
  - It exits with `pocket-desktop: cannot reach pocketd at {path}` (P packages/desktop/crates/pocket/src/main.rs:1050-1052).
  - It never reconnects the ops socket after `pocketd disconnected` (P packages/desktop/crates/daemon/src/daemon.rs:172-185).
  - The agents WS does retry every 2 s (P packages/desktop/crates/agents/src/agents.rs:226-238).

**M4. Agents: Claude Code and Codex.**
- Probe:
  - Run `run_login(["command","-v",cli])`, then `<cli> --version`, under the account's login shell. This reuses P packages/desktop/crates/daemon/src/daemon.rs:126-128, which already strips the launcher's env (:76-92).
  - Cache the result 30 s (M src/integrations/harness/core/availability.ts:64,77).
  - Check existence only. MonoCode never probes auth (r09).
- Row states and copy:
  - Found: "Claude Code · v{x}", with the path muted.
  - Missing: "Install the claude CLI to enable. Install with `curl -fsSL https://claude.ai/install.sh | bash`" plus an "Install" button (Z crates/ui/src/settings/harnesses.rs:71-93,1020-1031).
    - Codex: `curl -fsSL https://chatgpt.com/codex/install.sh | sh`; the manual fallback is `npm install -g @openai/codex`.
    - Zeron verified these commands on 2026-09-21 (Z crates/harness/src/install.rs:41-68,150-160).
  - Installing: "Installing Claude Code…" (Z crates/ui/src/settings/harnesses.rs:95-97).
    - Pocket runs the installer in a visible Terminal with `setup_op`, and the agent starts only if the install succeeds (P packages/desktop/crates/daemon/src/daemon.rs:104-107).
    - Zeron's installer is hidden, with a 15 min deadline and `CI=1 NONINTERACTIVE=1 TERM=dumb` (Z crates/harness/src/install.rs:1-12,240-242). Pocket's visible terminal lets interactive installer prompts work.
  - Not on PATH (adapted from Z crates/harness/src/install.rs:206-213): "The installer finished, but `claude` isn't on your PATH. Open a new terminal or add ~/.local/bin to PATH." Directories come from :165-177.
  - Login: no Pocket UI. The CLI's own sign-in runs in its Terminal.
    - Copy: "Claude Code and Codex sign in inside their terminal the first time you run them."
    - MonoCode needs "Authentication required" / "Sign in to continue using {X}." (M src/features/sessions/ui/ProviderSignInDialog.tsx:41-47) because it runs CLIs headless and detects auth errors by regex (M src/integrations/harness/core/authSupport.ts:28-36).
- Continue is always enabled. The secondary action is "Skip — use terminals only".

**M5. How Pocket sees agents.** Informational, a footnote under M4; no consent is needed.
- pocketd writes `~/.coding-pocket/plugin/{.claude-plugin/plugin.json,hooks/hooks.json}` at mode 0600 on every `serve` (P packages/pocketd/internal/daemon/plugin.go:24-54; P packages/pocketd/cmd/pocketd/serve.go:41-43).
- It appends the plugin to `CLAUDE_CODE_PLUGIN_DIRS` for Pocket Terminals only (P packages/pocketd/internal/daemon/plugin.go:65-82).
- Design intent: "changes no user file" (P docs/plans/2026-09-28-agent-sessions.md:60).
- Codex: pocketd watches `$CODEX_HOME/app-server-control/app-server-control.sock` (P packages/pocketd/internal/codex/rpc.go:163-165).
- Copy: "Pocket reads status from Claude Code and Codex running in its terminals. It changes nothing in ~/.claude or ~/.codex. Claude only runs hooks in folders you trust."
- Hooks call `'<abs path to pocketd>' hook` (P packages/pocketd/internal/daemon/plugin.go:32). The plugin is rewritten at each start, so a moved or updated bundle self-heals.

**M6. Add a project.**
- Copy: "Add a project to get started" / "A project is a folder on one of your devices." Button: "Add a project" (Z crates/ui/src/shell.rs:9958-9976).
- code≠screenshot: Z docs/media/registry-sync/01-empty.png still says "space".
- TCC: the first access to Desktop, Documents or Downloads shows a system prompt with the Info.plist purpose string. MonoCode's: "MonoCode opens project folders you choose, including Desktop." (likewise Documents, Downloads; M src-tauri/Info.plist:7-12). Pocket's desktop has no Info.plist yet (no bundle); the new one needs the same three keys.

**M7. First attached agent.**
- `cmd-n` (P packages/desktop/crates/pocket/src/main.rs:1064) runs `$SHELL -l -c '"$@"; exec $SHELL -l' claude` (P packages/desktop/crates/daemon/src/daemon.rs:109-122).
- Once attached, the status goes Working, then Needs you or Idle.
- Not-attached banners (P packages/desktop/crates/pocket/src/status.rs:106-107):
  - "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."
  - "This codex runs without the app-server, so Pocket can't see its status."
- MonoCode plays a decorative 7000–7600 ms first-use welcome, skipped under reduced motion (M src/features/sessions/ui/AstraWelcome.tsx:4,32-37; M src/features/sessions/ui/OpusWelcome.tsx:12,117-122). Not worth copying.

**M8. Pair your phone.** Shown after M7 and in Settings → Phone.
- A QR code of `anywhere://pair?host=<ip>:<port>&token=<token>` (r08:603-606), the host text, and "Copy token".
- Tailscale:
  - Today pocketd shells out to `tailscale ip -4` and falls back to `localhost` (P packages/pocketd/cmd/pocketd/serve.go:63-69). A phone can't use `localhost`.
  - Missing copy: "Install Tailscale on this Mac and your phone, signed in to the same account."
  - LAN fallback warning: "Without Tailscale, the token crosses Wi-Fi unencrypted." pocketd listens on all interfaces (P packages/pocketd/cmd/pocketd/serve.go:46) and accepts any Origin (P packages/pocketd/internal/wsserver/wsserver.go:53).

### F2. Phone first-run storyboard

**P1. Install.**
- TestFlight public link: up to 10,000 external testers (developer.apple.com/testflight). Builds expire after 90 days. The first build sent to an external group goes to App Review (TestFlight overview).
- Internal testers: ≤100 App Store Connect users, no review.

**P2. Pair.**
- Today (P packages/app/src/screens/ConnectScreen.tsx:30-59):
  - "Anywhere" heading, "Host" field (placeholder is the author's IP `100.77.122.82:4517`), "Token" field (placeholder "daemon token", secure entry), "Connect" button.
  - Offline shows "Disconnected — retrying".
  - Nothing connects at launch. `Root` shows ConnectScreen whenever the state isn't `online` (P packages/app/src/App.tsx:22-26), and `session.tsx` has no mount effect.
  - A wrong token closes with 1008 after an `error` "Rejected" (P packages/pocketd/internal/wsserver/wsserver.go:121-126). The session stores the `error` "Rejected" (P packages/app/src/session.tsx:67-69) but ConnectScreen never renders it; `onclose` sets `offline` and retries with backoff 1 s→30 s (P packages/app/src/client.ts:7,44-55), so the user reads "Disconnected — retrying" forever.
- Proposed flow:
  - Empty state: "Open Pocket on your Mac → Settings → Phone, then scan the code with the Camera app." Secondary: "Enter code manually".
  - The iOS Camera opens `anywhere://pair?…`. The scheme is already registered (P packages/app/app.json:7; P packages/app/ios/Anywhere/Info.plist:25-34), so the app needs no camera permission.
  - Connecting: a spinner in the button (current behaviour).
  - Rejected: "Rejected — token or version mismatch" (r08:605).
  - Offline: "Can't reach {host}. Is your Mac awake and on Tailscale?"
  - Relaunch: auto-connect from SecureStore `pocket.host`/`pocket.token` (P packages/app/src/screens/ConnectScreen.tsx:7-8).

**P3. Agents list** once `hello.ok` arrives (P packages/app/src/client.ts:38; P packages/app/src/App.tsx:26).

**P4. Notification pre-prompt.**
- Timing:
  - Zeron asks right after the first session started from the phone, "the moment the ask makes sense" (Z apps/ios/Zeron/App/PushNotifications.swift:76-82).
  - Pocket's phone starts no sessions yet, so ask the first time the list shows ≥1 agent after pairing.
- Inline card:
  - Title: "Get notified".
  - Body: "When a session finishes, needs you or fails" (Z apps/ios/Zeron/Shell/MoreViewController.swift:132).
  - Buttons: "Turn on" / "Not now".
- "Turn on" calls `requestAuthorization([.alert,.sound,.badge])`, only while the status is `notDetermined` (Z apps/ios/Zeron/App/PushNotifications.swift:86-97).
- Denied: alert "Notifications are off" / "Allow notifications for Anywhere in iOS Settings." with "Not Now" and "Open Settings" (Z apps/ios/Zeron/Shell/MoreViewController.swift:155-160).
- Granted: send `push.register {token}` to pocketd.
- Today there is no push code: no `expo-notifications` (P packages/app/package.json:14-31) and an empty entitlements dict (P packages/app/ios/Anywhere/Anywhere.entitlements:4).

### F3. Packaging: one signed, notarized .app

- **Layout (proposed):**
  ```
  Anywhere.app/Contents/
    Info.plist            dev.mingo.anywhere · LSMinimumSystemVersion 13.0 · NS{Desktop,Documents,Downloads}FolderUsageDescription
    MacOS/pocket          GPUI desktop; libghostty-vt.a static
    MacOS/pocketd         Go (cgo) daemon; libghostty-vt.a static; identifier dev.mingo.anywhere.pocketd
    Library/LaunchAgents/dev.mingo.anywhere.pocketd.plist
    Resources/AppIcon.icns · OFL.txt · THIRD_PARTY_NOTICES
  ```
  - Zeron's bundle is one exe with Info.plist `__VERSION__` substituted by sed (Z scripts/package-macos.sh:28-30).
  - Zeron copies font licences into Resources (:31-32).
  - The icon set is 16/32/128/256/512 @1x/@2x, built from `icon-1024.png` with sips and iconutil (:37-45).
- **libghostty:** static in both binaries. build.rs copies only `libghostty-vt.a` because "next to the .dylib the linker would pick the dylib" (P packages/desktop/crates/term/build.rs:4-5,12). cgo also links the `.a` (P packages/pocketd/internal/vt/vt.go:5). Pinned at 4ae9f1a2 and built with `zig build -Demit-lib-vt -Doptimize=ReleaseFast` (P scripts/build-ghostty.sh:4,11). There is nothing extra to sign; no Frameworks/.
- **Entitlements: none planned.** Inference: GPUI (Metal) and Go need no JIT. MonoCode needs `allow-jit` and `allow-unsigned-executable-memory` only for its WebView and JS runtime (M src-tauri/Entitlements.plist:5-8).
- **Signing:**
  - Sign `MacOS/pocketd` first, then the app, with `codesign --force --options runtime --timestamp --sign "$IDENTITY"`. No `--deep`: "Apple deprecated it" (Z scripts/package-macos.sh:47-55).
  - Verify with `codesign --verify --deep --strict --verbose=2` (M .github/workflows/release.yml:137).
- **Notarize:** `xcrun notarytool submit --key --key-id --issuer --wait` (Z scripts/package-macos.sh:60-64).
- **Staple:**
  - Staple the .app before tarring, because the updater swaps bundles without a DMG (:68-77).
  - Then build the DMG with dmgbuild UDZO (:87-124), notarize it and staple it (:126-129).
- **CI (port Z .github/workflows/release.yml):**
  - Tags `v*` plus dispatch (:16-19).
  - Secrets `MACOS_CERT_P12`/`_PASSWORD` and `AC_API_KEY_P8`/`_ID`/`_ISSUER_ID` (:59-73), imported into a temp keychain with `set-key-partition-list` (:79-98).
  - Guard that the tag matches the version (:159-169).
  - Write `manifest.json` with sha256 (:170-178), create the GitHub Release (:179-184), upload with `manifest.json` before `latest.txt` (:185-199).
  - Zeron silently falls back to ad-hoc signing when secrets are missing (:59-73). MonoCode refuses: "APPLE_SIGNING_IDENTITY is required and must not be ad-hoc '-'" (M .github/workflows/release.yml:76-78). Copy MonoCode.
  - MonoCode caching: immutable `public,max-age=31536000,immutable` for versioned keys, `no-store,max-age=0,must-revalidate` for latest (M .github/workflows/release.yml:269-282,390-392).
  - Extra build deps: Zig 0.16.0, the Ghostty checkout (~1.5 min, r14), Go, Rust. Cache `third_party/ghostty/zig-out`.
- **Architecture:** arm64 first, as Zeron's CI does (Z .github/workflows/release.yml:67). MonoCode ships x64 as well and lipo-checks it (M .github/workflows/release.yml:132).
- **Apple prerequisites:**
  - "Mac software notarization" is a Program-only benefit (compare-memberships).
  - The Program costs 99 USD per membership year (programs/enroll).
  - Unnotarized downloads need System Settings → Privacy & Security → "Open Anyway" (support.apple.com/102445).

### F4. pocketd LaunchAgent

- **Plist (proposed, shipped in the bundle):**
  ```xml
  <key>Label</key><string>dev.mingo.anywhere.pocketd</string>
  <key>BundleProgram</key><string>Contents/MacOS/pocketd</string>
  <key>ProgramArguments</key><array><string>pocketd</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>30</integer>
  ```
  - Zeron's template has Label = bundle id, ProgramArguments `[exe,"headless"]`, captured EnvironmentVariables, RunAtLoad, `KeepAlive{SuccessfulExit=false}`, ThrottleInterval 30, and StandardOut/ErrPath `{data_dir}/daemon.log` (Z apps/zeron/src/daemon.rs:262-301).
  - MonoCode uses `KeepAlive true`, ThrottleInterval 10, a PATH-only env and a 0600 file (M host/service.ts:61-84,220).
  - `SuccessfulExit=false` lets a clean stop stay stopped. ThrottleInterval 30 damps crash loops.
- **No PATH in the plist.** Zeron captures PATH because a service's PATH can't find `claude` (Z apps/zeron/src/daemon.rs:20-36). Pocket's desktop already sends each spawn a minimal Terminal.app-style env (PATH `/usr/bin:/bin:/usr/sbin:/sbin`) and runs it under `$SHELL -l`, so the login shell builds the real PATH (P packages/desktop/crates/daemon/src/daemon.rs:76-92,98-122), and pocketd resolves commands against the caller's PATH (P packages/pocketd/internal/terminal/terminal.go:87-107).
  - Gap: a spawn with nil Env falls back to pocketd's `os.Environ()` (P packages/pocketd/internal/daemon/daemon.go:34-40), which under launchd has a minimal PATH. Future phone-started sessions would hit this.
- **Logs:** pocketd should write `~/.coding-pocket/logs/pocketd.log` itself, rotating to `.old` under flock, the way Zeron's `zeron-{mode}.log` works (Z apps/zeron/src/main.rs:458-613). Inference: a bundle plist can't carry per-user absolute log paths.
- **Service detection:** Zeron's process knows it is the service when `XPC_SERVICE_NAME == label` (Z crates/update/src/lib.rs:766-772). It restarts via `launchctl kickstart -k gui/<uid>/<label>` (:791-808) and stops via `bootout`, "not `kill`: with KeepAlive the job would otherwise respawn" (Z apps/zeron/src/daemon.rs:140-141).
- **Session scope:** the `gui/<uid>` domain runs only while the user is logged in. MonoCode's copy: "keep the Mac signed in and awake" (M host/service.ts:202). Logout closes every Terminal.
- **Visibility:** helpers registered this way appear in System Settings → General → Login Items under the app's name (Apple sample article).
- **Rust binding:** `objc2-service-management` 0.3.2 (crates.io), the same objc2 0.3 family as `objc2-app-kit` (P packages/desktop/Cargo.toml:46).
- **Single instance:** pocketd has no lock today. Zeron flocks `{data_dir}/engine.lock` with a pid stamp and 40×25 ms retries, and fails with "another zeron engine is already running on {dir} (pid {pid}); stop it or use a different data dir (ZERON_DATA_DIR)" (Z crates/engine/src/instance_lock.rs:1-120). Pocket needs this so a dev `go run … serve` and the service can't fight over `pocketd.sock` and :4517.
- **Dev isolation:**
  - Zeron Dev uses bundle `sh.zeron.app.dev` with its own data dir and IPC port via LSEnvironment (Z dist/macos/Info-dev.plist).
  - It signs with Apple Development when available; otherwise it warns "macOS may ask for permissions again after a rebuild" (Z scripts/run-macos-dev.sh:43-53).
  - It launches through `open -W` so TCC attributes prompts to the app (:55-68).
  - Pocket already has `POCKET_HOME`/`POCKETD_SOCK` (P packages/pocketd/internal/config/config.go:14-27), but `create` hardcodes port 4517 (:55).
- **Uninstall copy** (M host/cli.ts:115-116): "The host is stopped and will not start automatically." / "Sessions, logs and device credentials are kept in {dir}. Delete that directory only if you want to erase them."

### F5. Updater and when pocketd may restart

- **Choice: port Zeron `crates/update` (Rust, in-process, GPUI).**
  - Sparkle would add an ObjC framework, XPC services and FFI to a Rust app (inference).
  - The Tauri updater is tied to Tauri (M src-tauri/Cargo.toml:61).
- **Zeron mechanics to keep:**
  - Timing: checks every 1 h, retries at 1/5/15/30 min, desktop first check after 2 s, idle recheck every 5 min, 30 s timeout on the staged `--version` check, metadata capped at 1 MiB / 60 s (Z crates/update/src/lib.rs:53-80).
  - Download to `.partial`, then sha256 (:594-639).
  - Stage to `{data_dir}/updates/<ver>/…app` (:816-869).
  - Swap: ditto to `.<App>.new-<pid>`, rename the old bundle aside, rename the new one in, roll back on failure (:886-920).
  - Relaunch: an `sh` loop runs `kill -0` every 0.2 s until the old process exits, then runs `/usr/bin/open` on the bundle (:923-952).
  - Desktop auto-update is on by default; `=0` switches to report-only (:984-999).
  - "Check for Updates…" sits in the app menu (Z crates/ui/src/app_update.rs:431-437).
  - "restart to apply", or install on quit (:1-14).
- **Add authenticity.**
  - Zeron checks only a same-origin sha256, warns when a checksum is missing (Z crates/update/src/lib.rs:594-639), and its `latest.txt` fallback skips verification (r02).
  - MonoCode ships minisign `.sig` files (M .github/workflows/release.yml:116-117,273).
  - Proposal: after staging, require `codesign --verify --strict -R 'anchor apple generic and certificate leaf[subject.OU] = "<TEAM>"'` and a stapled ticket. This reuses the Developer ID, so there is no second key to guard (inference).
- **MonoCode contrast:** it relaunches right after `downloadAndInstall` (M src/app/model/updater.ts:135-158). Its quit confirmation while chats run is described in r09.
- **pocketd restart policy.** A restart kills every PTY, because Terminals live inside pocketd (P packages/pocketd/cmd/pocketd/serve.go:34; r02).
  1. The app swap renames bundles, so the running pocketd keeps executing its old inode, and the LaunchAgent's `BundleProgram` resolves to the new binary on its next start (inference from Z crates/update/src/lib.rs:886-920).
  2. pocketd reads its bundle's `CFBundleShortVersionString` (Z crates/update/src/lib.rs:576-585) every 5 min (:72). If newer and 0 Terminals are open, it runs `kickstart -k` (:791-808), at most once per version (`SupersededRestart`, :1360-1405).
     - Zeron's gate: `!sessions.any_active() && !terminals.any_open()` "so a restart never lands under a live run or open PTY" (Z crates/engine/src/lib.rs:817-824).
  3. Pocket Terminals live for days, so step 2 rarely fires. The common path is a desktop chip: "Terminal service v{x} ready · Restart closes {n} terminals". Pressing it opens a confirmation that lists the Working and Needs-you agents.
  4. A forced restart happens only if the new desktop falls outside pocketd's protocol range.
     - Today the WS hello demands exact equality (P packages/pocketd/internal/wsserver/wsserver.go:122).
     - Clients hardcode 3 (P packages/desktop/crates/agents/src/agents.rs:248; P packages/protocol/src/constants.ts:1).
     - The ops socket has no version at all (P packages/desktop/crates/daemon/src/daemon.rs:172-186).
  5. Later: the SCM_RIGHTS fd handoff (15-14: batches of 64 fds, 30 s timeout) makes restarts free, and resume argv (15-1) restores agents.
- **The desktop must survive the restart.** Replace `exit(1)` and the one-shot reader with a reconnect loop (F1 M3).

### F6. Phone distribution and the APNs vs Expo decision

| Route | Who can install | Push | Cost | Limits | Source |
|---|---|---|---|---|---|
| TestFlight external, public link | anyone with the link | yes | $99/yr + EAS (free: 15 iOS builds/mo; Starter $19/mo) | ≤10,000 testers; build lives 90 days; first build gets beta review | developer.apple.com/testflight; TestFlight overview; expo.dev/pricing |
| TestFlight internal | ≤100 ASC users | yes | same | 90 days | TestFlight overview |
| Personal signing, paid team | the builder | yes, with their own EAS project + APNs key | $99/yr | must change team + bundle id (P packages/app/ios/Anywhere.xcodeproj/project.pbxproj:375,382-384) | Apple capabilities table |
| Personal signing, free Apple account | the builder | **no**: Push notifications is ticked only for ADP/ADEP | 0 | profile lifetime not verified here | supported-capabilities-ios |
| App Store | anyone | yes | $99/yr | App Review needs a way in without the reviewer's Mac. Zeron keeps a demo mode (`app?.isDemo`, Z apps/ios/Zeron/App/PushNotifications.swift:79) | — |

- **CI pattern (Z .github/workflows/testflight.yml):**
  - Manual dispatch with an internal/external audience (:5-14), main branch only (:25).
  - ASC JWT, ES256, `exp` = now+1199 s (:53-69).
  - Build number = max+1 from the ASC builds API (:88-91).
  - Turns on the PUSH_NOTIFICATIONS capability via the API (:93-120).
  - Archives with `CODE_SIGNING_ALLOWED=NO`, because runners were minting Apple Development certs until the limit (:143-155). Ad-hoc signs, greps `aps-environment` (:157-165), then exports with `app-store-connect` + `upload` (:167-186).
  - Polls processing 40×15 s (:188-234) and creates an "External Testers" group for beta review (:236-292).
  - Pocket alternative: EAS Build + Submit. An Expo project is needed for Expo Push anyway (projectId), and EAS stores the push key.
- **Decision: Expo Push. Direct APNs is dropped.**
  - The contradiction: r08 idea 08-1 puts a `.p8` in pocketd `config.Config.apns{…}` (r08:472), while r02 idea 02-2 sends through the Expo Push API from pocketd (r02:328). r08 left the key owner open (r08:610-613).
  - Answer: the key belongs to the app's publisher. For distributed builds only Expo, which holds the publisher's key via `eas credentials` (push-notifications-setup), lets any user's pocketd push without holding a secret.
  - Direct APNs would ship the publisher's `.p8` to every Mac. It is also iOS-only; r08 raised FCM as a second sender (r08:621).
- **Expo facts:**
  - A paid Apple account is required, and `getExpoPushTokenAsync({ projectId })` attributes the token to the EAS project (push-notifications-setup).
  - Send endpoint `https://exp.host/--/api/v2/push/send`: ≤100 messages per request, 600/s per project. Receipts at `/getReceipts`, checked about 15 min later, kept 24 h (sending-notifications).
  - "There is no cost". Expo doesn't store contents "any longer than it takes to deliver them". The iOS token survives a reinstall. A dead app returns `DeviceNotRegistered` (FAQ).
- **Enhanced push security stays off.** It would require an Expo access token in every pocketd (sending-notifications). A leaked `ExponentPushToken` lets anyone push to that phone, so store it like the pairing token: 0600 `config.json` (P packages/pocketd/internal/config/config.go:60).
- **App work:**
  - Add `expo-notifications`.
  - Add `aps-environment` to the committed `ios/` entitlements; `ios/` is checked in, so the config plugin alone won't patch it.
  - Add `extra.eas.projectId` to `app.json`.
  - Add a `push.register` message (r02:328).
- **Payload:** fixed copy by default (r08:614).
- **Limit:** a sleeping Mac sends no push (r08:615; keep-awake is 15-7).

### F7. Manual steps that stop a non-author today

| # | Step | Evidence |
|---|---|---|
| 1 | No build to download: no `.github/`, no bundle, no release | r14:50,396 |
| 2 | Install Zig 0.16.0 and run `scripts/build-ghostty.sh` (clones Ghostty, ~1.5 min); otherwise the build panics with "libghostty-vt.a missing: run scripts/build-ghostty.sh" | P scripts/build-ghostty.sh:1-11; P packages/desktop/crates/term/build.rs:5-6; r14 |
| 3 | Go and Rust toolchains; pocketd's cgo paths are repo-relative | P packages/pocketd/internal/vt/vt.go:4-5 |
| 4 | Start `pocketd serve` by hand and keep that terminal open; closing it kills every Terminal | P packages/pocketd/cmd/pocketd/main.go:21-22; r14 |
| 5 | Start pocketd before the desktop, or the desktop exits; a later pocketd death is never recovered | P packages/desktop/crates/pocket/src/main.rs:1050-1052; P packages/desktop/crates/daemon/src/daemon.rs:183 |
| 6 | No `pair`, `status`, `--version` or service command | P packages/pocketd/cmd/pocketd/main.go:10 |
| 7 | Phone: build with Xcode after replacing `DEVELOPMENT_TEAM = "KK9V7PRPF2"` and the bundle id `dev.mingo.anywhere` | P packages/app/ios/Anywhere.xcodeproj/project.pbxproj:375,382,410,416; P packages/app/app.json:12; P packages/app/package.json:8 (`expo run:ios`) |
| 8 | Install Tailscale on both devices; otherwise pocketd prints `ws://localhost:4517` | P packages/pocketd/cmd/pocketd/serve.go:58,63-69 |
| 9 | Copy the token from pocketd's stdout and type `host:port` over the author's placeholder IP; tap Connect on every launch | P packages/pocketd/cmd/pocketd/serve.go:59; P packages/app/src/screens/ConnectScreen.tsx:36,52; P packages/app/src/App.tsx:26 |
| 10 | A wrong token or protocol looks like "Disconnected — retrying" | P packages/app/src/client.ts:44-55; P packages/app/src/screens/ConnectScreen.tsx:59 |
| 11 | Trust each folder in Claude so hooks run; use a codex that has the app-server | P docs/spike-remote-sessions.md:104; P packages/desktop/crates/pocket/src/status.rs:106-107 |
| 12 | No LICENSE, so nobody else may legally redistribute or modify the code | r15:326 |

### F8. Licence and pricing stance

- **Peers** (r15 matrix):
  - MIT: Zeron, MonoCode, Happy, Nimbalyst.
  - Apache-2.0: herdr, Emdash.
  - ELv2: Superset. GPL-3.0+ app with BSL 1.1 server: cmux. AGPL: Claude Squad. Proprietary: Conductor.
  - Clone headers: "MIT License / Copyright (c) 2026 Wing" (Z LICENSE:1-3) and "Copyright (c) 2026 Nick" (M LICENSE:1-3).
- **Notices Pocket must carry:**
  - Ghostty is MIT, "Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors" (github.com/ghostty-org/ghostty LICENSE). It is vendored at `third_party/ghostty` in the author's main checkout, not in this worktree.
  - Geist is OFL, already shipped (P packages/desktop/crates/theme/assets/fonts/OFL.txt).
  - Any ported Zeron or MonoCode code must keep their MIT notice.
- **Stance: MIT** for pocketd, protocol, desktop and app.
  - There is no hosted service to protect. ELv2 and BSL exist to protect one.
  - MIT allows the ports proposed across r01-r15.
- **Pricing: free, no account.**
  - Fixed costs: Apple $99/yr and EAS (the free tier suffices).
  - Per-user cost is zero: no relay, and Expo push is free.
  - Market: paid tiers gate the phone (Superset Pro $20/user/mo, r15:22,69; Conductor Pro $50/mo, r15:238), and "free-only orchestrators struggle" (r15:429). Pocket's edge is having no vendor relay (r15:414).
  - Revisit only for something with a running cost, such as a hosted relay for users without Tailscale.
  - An optional, dismissible "Star on GitHub" banner is Zeron's only ask (Z crates/ui/src/shell.rs:1009,8059-8078; Z crates/ui/src/settings.rs:756-758).

### F9. Code vs doc disagreements

- Z dist/README.md:63-68 signs with `codesign --deep`. The script avoids `--deep` (Z scripts/package-macos.sh:47-55). Code wins.
- Z dist/README.md:40-47 documents a universal lipo build. CI builds arm64 only (Z .github/workflows/release.yml:67).
- Z apps/ios/README.md:128-131 says the archive uses "automatic signing". The workflow archives unsigned and signs at export (Z .github/workflows/testflight.yml:143-186).
- M src-tauri/tauri.conf.json:58,74-75 ship `signingIdentity "-"` and an empty updater `pubkey`/`endpoints`. CI injects the real values and rejects `-` (M .github/workflows/release.yml:69-107).
- Zeron's empty-state screenshot says "space" (Z docs/media/registry-sync/01-empty.png). The code says "project" (Z crates/ui/src/shell.rs:9963).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 19-1 | Signed, notarized, stapled `Anywhere.app` embedding `pocket` and `pocketd`, with DMG + app.tar.gz + manifest.json; CI on tag | Download and run; no toolchains | Z scripts/package-macos.sh:18-129; Z .github/workflows/release.yml:16-199; M .github/workflows/release.yml:76-78,137 | **new** `scripts/package-macos.sh`, `.github/workflows/release.yml`, `packages/desktop/dist/macos/Info.plist` | L | Paid ADP; Developer ID cert; ASC API key; 19-16 |
| 19-2 | pocketd LaunchAgent via SMAppService, plist in the bundle; states Starting / Needs approval / Failed | Terminals survive closing the window; the phone always reaches the Mac | Apple SMAppService docs; Z apps/zeron/src/daemon.rs:262-301; M host/service.ts:178,216-222,262-273 | **new** in `packages/desktop/crates/daemon` (objc2-service-management) + onboarding view in `crates/pocket` | M | 19-1 (bundle), 19-5 |
| 19-3 | Desktop waits and reconnects instead of `exit(1)`: "Starting Pocket's terminal service…" | No crash on first launch or after a pocketd restart | P packages/desktop/crates/pocket/src/main.rs:1050-1052; P packages/desktop/crates/daemon/src/daemon.rs:172-185 | **adapt** `crates/daemon` (reconnect loop), `crates/pocket` (state) | M | — |
| 19-4 | pocketd `--version`, `status`, `pair` (QR), single-instance flock, own rotating log | Diagnosable service; staged-version check for the updater | Z crates/engine/src/instance_lock.rs:1-120; Z apps/zeron/src/main.rs:458-613; Z crates/update/src/lib.rs:660-685; r08:603-606 | **adapt** `packages/pocketd/cmd/pocketd/main.go`, **new** `internal/lock`, `internal/logfile` | M | overlaps 02-7/02-8 |
| 19-5 | Gate for translocated / DMG / unwritable locations | Service and updates never bind to a temporary path | Z crates/update/src/lib.rs:360-380,474-512 | **port** into `crates/pocket` onboarding | S | — |
| 19-6 | Agent check: probe claude/codex via the login shell; Install runs the official installer in a visible Terminal; PATH hint | New users get a working agent in one click | Z crates/harness/src/install.rs:41-68,150-177,206-213; Z crates/ui/src/settings/harnesses.rs:71-97; P packages/desktop/crates/daemon/src/daemon.rs:104-128 | **adapt** `crates/daemon` (`run_login`, `setup_op`), **new** onboarding rows | M | — |
| 19-7 | One-line attach explainer with a trust hint instead of a consent screen | Users know why status appears and what was (not) changed | P packages/pocketd/internal/daemon/plugin.go:24-82; P docs/plans/2026-09-28-agent-sessions.md:60; P packages/desktop/crates/pocket/src/status.rs:106-107 | **new** copy in onboarding | S | 19-6 |
| 19-8 | In-app updater ported from Zeron plus a codesign requirement check | Stay current without re-downloading | Z crates/update/src/lib.rs:53-80,594-952; Z crates/ui/src/app_update.rs:1-39,382-437 | **port** as new crate `packages/desktop/crates/update` | L | 19-1, 19-4 (`--version`) |
| 19-9 | pocketd restart policy: auto only at 0 Terminals, once per version; otherwise "Restart closes N terminals" | Updates never silently kill running agents | Z crates/engine/src/lib.rs:817-824; Z crates/update/src/lib.rs:72,576-585,791-808,1360-1405 | **new** in `pocketd` (version watch + kickstart) and desktop chip | M | 19-2, 19-8, 19-10 |
| 19-10 | Protocol range negotiation (`min`/`max` in hello; version on the ops socket) | Phone (TestFlight), desktop and pocketd can differ by one version | P packages/pocketd/internal/wsserver/wsserver.go:122; P packages/protocol/src/constants.ts:1; P packages/desktop/crates/agents/src/agents.rs:248 | **adapt** `packages/protocol`, `pocketd/internal/proto`, `wsserver`, `ops` | M | before the first TestFlight build |
| 19-11 | QR pairing through the system Camera + `anywhere://pair` deep link; auto-connect; distinct Rejected/offline copy | Pairing in seconds; no typing a 32-char token | r08:603-606; P packages/app/app.json:7; P packages/app/src/client.ts:44-55 | **adapt** `packages/app` (Linking handler, ConnectScreen, session), desktop Settings → Phone QR | M | 19-4 `pair` |
| 19-12 | Expo Push from pocketd (settles 02-2 vs 08-1) | The phone buzzes on Needs you / Done / Failed | Expo send/FAQ/setup docs; r02:328; r08:472,610-613 | **new** `pocketd/internal/push`; `packages/app` expo-notifications + entitlement; `push.register` in protocol | M | Paid ADP; EAS project + publisher APNs key; 19-14 |
| 19-13 | Notification pre-prompt card + denied alert | Asked at the right moment; recoverable when denied | Z apps/ios/Zeron/App/PushNotifications.swift:76-97; Z apps/ios/Zeron/Shell/MoreViewController.swift:132,155-160 | **new** in `packages/app/src/screens/AgentsScreen.tsx` | S | 19-12 |
| 19-14 | TestFlight via EAS Build + Submit (fallback: Zeron's Actions workflow) | Phone install without Xcode | Z .github/workflows/testflight.yml:5-292; developer.apple.com/testflight; expo.dev/pricing | **new** `packages/app/eas.json`, workflow | M | Paid ADP; ASC app record |
| 19-15 | Self-build: team and bundle id from env/app config, not the pbxproj; BUILD.md with prerequisites | Contributors can build without editing Xcode files | P packages/app/ios/Anywhere.xcodeproj/project.pbxproj:375,382; P scripts/build-ghostty.sh | **adapt** `packages/app` config; **new** doc | S | 19-16 |
| 19-16 | MIT LICENSE + THIRD_PARTY_NOTICES (Ghostty, Geist OFL, ported Zeron/MonoCode code) | Legal to use, fork and contribute | r15:326; Z LICENSE:1-3; M LICENSE:1-3; P packages/desktop/crates/theme/assets/fonts/OFL.txt | **new** repo-root files; copy into `Resources/` | S | author decision |
| 19-17 | Pairing screen detects Tailscale and warns on LAN fallback | Users don't pair to `localhost` or leak the token on Wi-Fi | P packages/pocketd/cmd/pocketd/serve.go:46,63-69; r08 open q | **adapt** pocketd `pair` + desktop Settings → Phone | S | 19-11; pairs with 15-13 |
| 19-18 | TCC purpose strings for Desktop/Documents/Downloads | Clear system prompts; fewer denials | M src-tauri/Info.plist:7-12 | **new** keys in the desktop Info.plist | S | 19-1 |
| 19-19 | Dev isolation: "Pocket Dev" bundle id, `POCKET_HOME`, port and label | Dev builds don't fight the installed service | Z dist/macos/Info-dev.plist; Z scripts/run-macos-dev.sh:43-68; P packages/pocketd/internal/config/config.go:14-27,55 | **adapt** `config.create` (port), dev plist | S | 19-2 |
| 19-20 | "Stop terminal service" / uninstall with MonoCode's copy | A clean exit path; trust | M host/cli.ts:103-118; M host/service.ts:129-167 | **new** desktop menu item (SMAppService `unregister`) | S | 19-2 |
| 19-21 | Demo mode for App Review / external beta review | Makes an App Store listing possible | Z apps/ios/Zeron/App/PushNotifications.swift:79 (`isDemo`) | **new** in `packages/app` | M | only if going to the App Store |
| 19-22 | Direct APNs from pocketd (08-1) | — | r08:472 | — | L | superseded by 19-12 |

## UI/UX spec to copy

- **DMG:**
  - dmgbuild UDZO; `window_rect ((200,120),(660,400))`; `icon_size 104`; `text_size 12`.
  - App at (165,195), Applications at (495,195); background @1x/@2x TIFF (Z scripts/package-macos.sh:87-124).
  - Zeron's look: dark background with ASCII art, five chevrons between the icons (Z docs/media/dmg-window-preview.png).
- **Onboarding card** (Zeron login card, Z crates/ui/src/shell.rs:11432-11490):
  - Card: width 360, px 32, py 40, radius 12, 1 px border, `surface_card`, `shadow_lg`, over a grid backdrop.
  - Logo 31.4×36.
  - Title: mt 24, 18 semibold.
  - Body: mt 6, mb 24, 13/19 muted.
  - Primary button: height 36, radius 6, background = text colour, 14 medium, hover opacity 0.9.
  - Retry button: px 12, py 6, radius 8, 13 text (Z crates/ui/src/shell.rs:11395-11430).
  - One card per step (M2, M3, M4 / M5, M8). Return triggers the primary button, Esc the secondary (proposed).
- **Empty state (M6)** (Z crates/ui/src/shell.rs:9938-9978):
  - Logo 41.9×48 at text opacity 0.09.
  - Title: mt 24, 16 medium, text colour.
  - Body: mt 6, 13, `text_muted` at 0.7.
  - Button: mt 20, `btn_primary`.
  - The whole block is wrapped in `motion::fade_in`.
- **Agent row (M4)**, adapted from M src/features/sessions/ui/ProviderSignInPanel.tsx:29-69:
  - Icon tile: 64×64, `rounded-2xl`, background content/6 %, 1 px inset ring content/8 %; icon 36.
  - Title: 15 px medium, line height 20.
  - Body: 11 px, line height 16, content/45, max width 224.
  - Button: height 32, `rounded-lg`, px 14, 12 px medium, hover content/85, active scale 0.98, 150 ms ease-out.
  - Busy: 14 px spinning icon plus the label ("Installing Claude Code…").
  - Error: 10 px red-500, mt 10, max width 240.
- **Update strip copy** (Z crates/ui/src/app_update.rs:382-428):
  - "Update available — v{latest}"
  - "Downloading v{version}…"
  - "Update ready — restart to apply"
  - "Update failed: {message}"
  - "Restarting…"
  - CLI: "zeron {current} is up to date"; `--check` exits 1 when an update exists (Z apps/zeron/src/update_cli.rs).
  - Pocket additions: "Terminal service v{x} ready · Restart closes {n} terminals" and "Waiting for terminals to close" (cf. "Waiting for agent to be idle", Z crates/ui/src/settings/harnesses.rs:118-160).
- **Motion** (Z docs/research/feature-inventory.md:27,115-116,120):
  - fade-in 0.5 s `cubic-bezier(0.16,1,0.3,1)`, translateY 4→0.
  - dialog-in 0.18 s, scale 0.96→1.
  - splash-out 0.5 s, opacity plus translateY −6 px, 0.15 s delay.
  - Boot splash capped at 15 s.
  - Honour reduced motion (P packages/desktop/crates/pocket/src/main.rs:1041-1045; M src/features/sessions/ui/AstraWelcome.tsx:32-37).
- **Shortcuts:**
  - Existing: `cmd-n` start session, `cmd-,` project settings (P packages/desktop/crates/pocket/src/main.rs:1064,1070).
  - Add "Check for Updates…" to the app menu (Z crates/ui/src/app_update.rs:431-437).
- **Phone pair screen:**
  - Zeron's sign-in layout (Z apps/ios/Zeron/Shell/SignInViewController.swift:36-131):
    - Symbol 44 pt light.
    - Title semibold 34.
    - Tagline 17 secondary.
    - Stack spacing 12; centre Y −80.
    - Capsule filled button: background = text colour, insets 15/20, semibold 17, 24 from the safe-area edges.
    - Status line 14 danger: "Sign-in didn't complete. Try again."
  - Current Pocket tokens (P packages/app/src/screens/ConnectScreen.tsx:64-86):
    - Root padding 24, gap 8.
    - Heading 28/700, mb 24.
    - Label 12.
    - Input padding 12, 15 text.
    - Button paddingVertical 14, mt 24.
    - Error 12, centred, mt 12.
- **Phone copy:** F2 P2 and P4.
- **Mac copy:** F1 M2–M8.
- **Settings → Phone QR:** size not specified by either clone (open).

## Open questions / risks

- **Apple account and identity.**
  - Whose paid ADP account publishes? The Team ID in the pbxproj is KK9V7PRPF2 (P packages/app/ios/Anywhere.xcodeproj/project.pbxproj:382); Zeron's is 5XY3M483YQ (Z .github/workflows/testflight.yml:167-186).
  - Should the Mac bundle id stay under `dev.mingo.*`?
- **Unverified SMAppService details.**
  - The `BundleProgram` plist key and the `openSystemSettingsLoginItems()` call were not in the fetched Apple pages.
  - Whether TCC attributes pocketd's child-shell file access to "Pocket" (responsible process) or to `pocketd`.
- **Claude and the plugin dir.** Does Claude Code prompt about plugins loaded from `CLAUDE_CODE_PLUGIN_DIRS`? Known: hooks run only in trusted folders (P docs/spike-remote-sessions.md:104).
- **Codex floor.** Which codex version first uses the app-server by default? pocketd treats `--no-daemon`, `exec` and others as not attachable (P packages/pocketd/internal/daemon/codex.go:37-52). The codex daemon also self-updated during the spike (P docs/spike-remote-sessions.md:105-107).
- **Restarts until the fd handoff lands (15-14).** Every pocketd update that needs a restart closes all Terminals. Is the "Restart closes N terminals" UX acceptable, or should pocketd updates be rare and batched?
- **Protocol skew.** A TestFlight phone updates on its own schedule. What window is supported (N-1)? Without 19-10, the first protocol bump strands every phone.
- **Architecture and OS floor.** arm64-only first? Is macOS 13 (SMAppService) acceptable, given Zeron supports 12.0 (Z dist/macos/Info.plist:38-39)?
- **Mac asleep or logged out.**
  - No push and no phone access while the Mac sleeps.
  - The gui domain stops at logout (M host/service.ts:202).
  - Keep-awake while Working (15-7)?
- **LAN security.**
  - Plain `ws://` with the token on every interface (P packages/pocketd/cmd/pocketd/serve.go:46).
  - `NSLocalNetworkUsageDescription` is absent (P packages/app/ios/Anywhere/Info.plist). The iOS local-network prompt behaviour for LAN pairing is untested.
  - Tailscale-only plus loopback (15-13)?
- **Expo dependency.**
  - The third party sees push metadata. Enhanced security can't be enabled.
  - Android needs FCM credentials in EAS.
  - EAS free tier: 15 iOS builds/mo.
- **Firewall prompt.** A signed pocketd accepting connections on :4517 is allowed automatically only if "Automatically allow downloaded signed software" is on (unverified: no Apple firewall page is in Sources). The default state was not verified.
- **App Store later.** Review needs a demo path (19-21). The external TestFlight beta review of the first build may need one too.
- **Pricing.** Is free-forever the intent, or should the phone app or a hosted relay be held back for a paid tier? Peers gate the phone (r15:22,238).

## Verification

Date: 2026-09-30. Claims checked: 15 (static libghostty link, SMAppService/LaunchAgent contrast with Zeron bootout and MonoCode kickstart, desktop `exit(1)` and one-shot ops reader, hello `== 3` check, plugin via `CLAUDE_CODE_PLUGIN_DIRS`, Zeron translocation gate, quiescence gate + `SupersededRestart`, updater timings and bundle swap, installer commands and env, package-macos.sh sign/notarize/staple/DMG geometry, release.yml secrets/manifest/order, MonoCode ad-hoc refusal, phone ConnectScreen/client/entitlements/package.json, pocketd config/port/lock, release asset sizes via `gh api`). Ideas-table mappings all point at existing Pocket paths or are marked **new**. Corrected: 6.

- P2: the client does receive the `error` "Rejected" (session stores it, P packages/app/src/session.tsx:67-69); ConnectScreen just never renders it. Reworded.
- App.tsx citations moved to :22-26 / :26 (the `online` check).
- F7 row 7: bundle id cited at P packages/app/app.json:12 and both pbxproj configs (:375,382,410,416); package.json:8 is only `expo run:ios`.
- M6: Pocket's desktop has no Info.plist today; reworded as a key for the new one.
- F4: the desktop sends a minimal env with PATH `/usr/bin:/bin:/usr/sbin:/sbin`, not a login-shell env; the `$SHELL -l` wrapper builds PATH.
- Z scripts/run-macos-dev.sh `open -W` is at :68 (range fixed); firewall claim marked unverified (no source).
