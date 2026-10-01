# 08 · Zeron: iOS app and push

Date: 2026-09-30.
Sources:
- `zeronsh/comet` (now `zeronsh/zeron`) @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5` (v0.2.99, shallow), cloned at `/Users/mingo/tmp/orchestrators/zeron`. MIT.
- https://zeron.sh.
- Expo docs: https://docs.expo.dev/versions/latest/sdk/notifications/ and https://docs.expo.dev/versions/latest/sdk/application/.
- Pocket worktree `/Users/mingo/.worktrees/anywhere/orchestrator-research` @ `86deb13`.

Citation legend: `Z path:L` is a file and line in the Zeron clone, relative to its root. Unless stated otherwise, Swift paths are under `apps/ios/Zeron/`, so `Z Shell/MainTabController.swift:24` means `apps/ios/Zeron/Shell/MainTabController.swift`. `P path:L` is a file and line in the Pocket worktree, relative to its root. `M` (monocode) is not used here. A URL is a vendor doc. When code, docs and screenshots disagree, code wins; §F1 flags every disagreement. "Screenshot" means a PNG under `Z docs/screenshots/`, which I viewed.

## TL;DR

- **Zeron iOS is a UIKit shell over a Rust core, linked through UniFFI.** "Rust decides what to paint and where; Swift paints, scrolls and handles gestures" (Z apps/ios/README.md:8-10).
  - The phone is a peer device: it mirrors the cloud registry and session rooms and runs no agent (Z apps/ios/README.md:3-6).
  - It never talks to the Mac directly. All traffic goes through Cloudflare Durable Objects (Z edge/src/device-room.ts:1-16).
- **Docs and screenshots are partly stale.** The scope label "SwiftUI", `docs/mobile-polish.md` and most screenshots describe the older SwiftUI/UITableView app. Values below come from code (§F1).
- **Navigation.**
  - iPhone: a tab bar (Sessions / Settings / Search) plus a bottom "New session" accessory with a live summary, "2 working · 1 needs you".
  - iPad: a split view with a 360pt sidebar and ⌘N / ⇧⌘F / ⌘B (Z Shell/MainTabController.swift:24-35,190-264; Z Shell/SplitRootController.swift:36-59).
- **Session screen.**
  - A glass header plus a glass status pill for connectivity/send banners. Working state renders as a transcript tail row ("Working…" / "Writing…" + elapsed), not in the pill (Z Session/CoreSessionSource.swift:86; Z Transcript/RowView.swift:403-433).
  - A virtualized transcript with a "send runway" and strict follow rules.
  - A composer that is a capsule at rest and a card when active, with queue / steer / stop-and-send while the agent works (Z Session/SessionViewController.swift:565-643; Z Composer/ComposerBar.swift:32-40,579-633).
- **No approvals from the phone.** Runs are `auto_approve: true` (Z crates/client/src/session/mod.rs:1054).
  - The phone only answers agent questions, through a paged glass panel that replaces the composer (Z Composer/QuestionPanel.swift:3-5).
  - Pocket already has real permission approvals. Copy the panel UX, not the policy.
- **Push is computed at the edge from registry row diffs.** It covers three kinds (done / input / failed) and sends APNs directly: ES256 JWT cached 50 min, priority 10, 24h expiry, collapse-id and thread-id = chatId (Z edge/src/push-notify.ts:37-99; Z edge/src/apns.ts:26-80).
  - No actions, no badge, no seen-suppression.
  - Silent in the foreground; a tap opens the chat (Z App/PushNotifications.swift:161-193).
- **Permission prompt timing.** The app asks for notification permission after the first session started from the phone. Per-kind toggles live in Settings; the token re-registers on sign-in and unregisters on sign-out (Z App/PushNotifications.swift:67-112; Z Shell/MoreViewController.swift:132-161).
- **Offline is "graced", not modal.**
  - Degradation shows after 4s; "Not delivered" after 120s.
  - Sends are durable (doc ledger plus host nudges).
  - Lifecycle hooks fire on network change, foreground and background (Z crates/client/src/connectivity.rs:1-56; Z crates/client/src/client.rs:1410-1451).
- **Pocket gaps.**
  - No push.
  - Any socket drop ejects the user from the chat to ConnectScreen (P packages/app/src/App.tsx:22-26).
  - Sends silently no-op while offline (P packages/app/src/client.ts:57-61).
  - Drafts are lost when the chat unmounts; only Stop is available while the agent works.
  - One permission slot; portrait only.
- **Recommendation.**
  1. pocketd sends APNs itself, driven by its existing seen-aware status machine, which is better than Zeron's edge rule. The app registers its token over the existing WebSocket (08-1..08-5).
  2. Then graced connectivity with an outbox (08-6..08-8).
  3. Then an inline approval panel (08-13).

## Findings

### F1. Doc, code and screenshot disagreements (code wins)

- **UIKit, not SwiftUI.** SwiftUI was dropped for per-cell hosting cost and whole-doc re-decode on every token (Z docs/mobile-rewrite.md:3-13; Z apps/ios/README.md:3).
- **`docs/mobile-polish.md` describes the old app.** It covers the `UITableView` + `UIHostingConfiguration` era (Z docs/mobile-polish.md:3-8). Each value it gives is superseded:

  | What | Doc says | Code says |
  |---|---|---|
  | User bubble corners | 22pt (Z docs/mobile-polish.md:28-29) | `BUBBLE_RADIUS 20.0` (Z crates/mobile/src/layout/rows.rs:131) |
  | Long-message preview | 5 lines / 400 chars (Z docs/mobile-polish.md:35-36) | fold at 8 lines, show 6 (Z crates/mobile/src/layout/rows.rs:134-135) |
  | Chat body | 17pt (Z docs/mobile-polish.md:45) | 16.5/25 (Z crates/mobile/src/layout/style.rs:104-110) |
  | Tool-group disclosure | 200ms (Z docs/mobile-polish.md:107) | 140ms fold, 360ms reveal (Z Transcript/ToolViews.swift:4-14) |

- **Screenshots show older chrome.**
  - `mobile-polish/home-glass.png` and `project-menu.png` show an "All ▾" project pill, glass [+] and [account] buttons, and project@host *above* the title.
  - Code has a tab bar with a bottom accessory and puts meta *below* the title (Z Threads/SessionCell.swift:157-191).
- **Queue screenshot differs from the code.**
  - `appshots/appshots-ios-queue-actions.png` shows a "2 QUEUED" header with numbered rows and pencil / → / … buttons, and a resting composer with no "+".
  - Code: QueuePanel shows at most 3 rows plus "+N more queued", with a clock glyph (Z Composer/QuestionPanel.swift:217-328). The capsule is `[+] Message… [↑]` (Z Composer/ComposerBar.swift:32-40).
- **Push-notify comment overstates "input".** It says input covers "a question / permission prompt" (Z edge/src/push-notify.ts:40). Every run auto-approves (Z crates/client/src/session/mod.rs:1054; Z crates/harness/src/codex/mod.rs:968-970), so on Zeron "input" can only mean a question.
- **Push docs live in the iOS README.** The root README has no mobile section; the push docs are `apps/ios/README.md:110-124`.

### F2. Architecture of the phone app

- **Layout and paint.**
  - Rust measures every row at viewport width and publishes a `LayoutFrame` (heights plus prefix-sum offsets). Swift positions reusable `RowView`s and paints with CoreText at the Rust coordinates (Z apps/ios/README.md:53-67).
  - Reference numbers: cold layout of 3,300 rows in 30 ms, streamed token 0.19 ms, 0 hitches in 671 idle and 536 streaming frames (Z docs/mobile-rewrite.md:75-78).
- **AppModel.**
  - Owns `CoreClient` (Z App/AppModel.swift:139-146).
  - Device id is `"ios-"` + 8 hex (Z App/AppModel.swift:175-180).
  - Runs a 30s refresh timer (Z App/AppModel.swift:157).
  - Uses `NWPathMonitor` to feed network state to Rust (Z App/AppModel.swift:75-80).
- **Transport** is WorkOS-authenticated WebSockets to the edge.
  - A per-device `DeviceRoom` relays frames: uleb128 header length ‖ JSON header `{s,k,to?,from?}` ‖ payload (Z edge/src/device-room.ts:8-11,22-47).
  - A host counts as live only if it pinged within 75s; hosts ping every 15s (Z edge/src/device-room.ts:62-78).
  - A dead host gets a `host_offline` bounce ("device is asleep" instead of hanging) (Z edge/src/device-room.ts:266-270).
  - A new host socket supersedes the old one with close code 4409 (Z edge/src/device-room.ts:167).
- **Durable command nudges.**
  - Any owner device may `POST /nudge {chatId}`. The DeviceRoom queues it (cap 4096; returns a retryable 503 when full) and replays it to the host on join, with a 5s alarm re-delivering unacked nudges (Z edge/src/device-room.ts:209-253; Z edge/src/device-nudges.ts:1-39).
  - This is how a phone send reaches a sleeping Mac later.
- **Pocket** is the opposite design.
  - The phone opens `ws://<host>` (user-typed `ip:port`, default port 4517) straight to pocketd (P packages/app/src/session.tsx:76; P packages/pocketd/internal/config/config.go:55). pocketd listens on all interfaces and prints a Tailscale IP (P packages/pocketd/cmd/pocketd/serve.go:46-69).
  - No cloud, no account, and no durability outside pocketd.

### F3. Sign-in and pairing

- **Zeron sign-in screen** (Z Shell/SignInViewController.swift:38-62):
  - sparkle symbol, 44pt light
  - "Zeron", SemiBold 34
  - tagline "Your coding agents, from anywhere.", 17
  - a capsule "Sign In" button with insets 15/20, SemiBold 17
- **Flow.**
  - `ASWebAuthenticationSession` to `https://edge.zeron.sh` AuthKit, with callback scheme `zeron` (Z Shell/SignInViewController.swift:4-19,91-117; Z Info.plist:25-35).
  - Failure copy: "Sign-in didn't complete. Try again."
  - Multi-org users get "Choose an organization" (Z Shell/SignInViewController.swift:121-131).
- **Credentials.**
  - Keychain service `sh.zeron.ios`, account `credentials`, accessibility `AfterFirstUnlock` (Z App/AppModel.swift:572-622).
  - A per-account owner marker keeps one user's local data away from the next (Z App/AppModel.swift:117-131).
  - Sign-out and expiry are distinct flows (Z App/AppModel.swift:204-216).
  - Sign-out confirm: "Sign out?" / "Local drafts stay on this device." (Z Shell/MoreViewController.swift:237).
- **No device pairing.** The account is the trust boundary.
- **Pocket pairing.**
  - The user types host and token. They are stored in SecureStore as `pocket.host` / `pocket.token` (P packages/app/src/screens/ConnectScreen.tsx:7-26). Placeholder: `100.77.122.82:4517` (P packages/app/src/screens/ConnectScreen.tsx:36).
  - pocketd creates a 24-byte base64url token in `~/.coding-pocket/config.json` with mode 0600 (P packages/pocketd/internal/config/config.go:14-20,52-60) and prints `phone: ws://<tailscale-ip>:<port>` plus the token (P packages/pocketd/cmd/pocketd/serve.go:58-59).
  - `hello {token, clientId, protocolVersion}` is checked; a mismatch gets "Rejected" and a policy-violation close (P packages/pocketd/internal/wsserver/wsserver.go:122-127).
  - A QR code holding a deep link would remove the typing (08-18).

### F4. Screens and navigation

- **Root.** Shows sign-in, the tab shell (compact width) or the split shell (regular width) (Z App/SceneDelegate.swift:77-81), switched with a 0.35s crossDissolve (Z App/SceneDelegate.swift:91). Tint is `Palette.text` (Z App/SceneDelegate.swift:14-15).
- **iPhone tabs** (Z Shell/MainTabController.swift:21-35,105-128):
  - Tabs: Sessions / Settings / `UISearchTab`. The tab bar minimizes on scroll down.
  - Large title SemiBold 30, inline title 17.
  - "New session" opens a `.pageSheet` at the `.large` detent with a grabber.
- **Bottom accessory** (Z Shell/MainTabController.swift:190-264):
  - Left: a plus glyph (15 semibold) in a 34×34 `accentSoft` circle, then "New session" in Medium 16.
  - Right, 16pt from the trailing edge: a summary "N working · N need(s) you" in Medium 13 secondary, with a 12×12 glyph (spinner if anything is working, otherwise the input dot).
  - Fades in over 0.25s after a 0.12s delay (Z Shell/MainTabController.swift:58).
- **Draft handoff.** Text typed in the new-session sheet moves continuously into the opened chat (Z Shell/MainTabController.swift:141-169; Z Session/DraftHandoff.swift:3-5).
- **The session screen hides the tab bar** (Z Session/SessionViewController.swift:28).
- **Pocket** has no navigation stack. `Root` swaps ConnectScreen, AgentsScreen and ChatScreen on connection state and the selected id (P packages/app/src/App.tsx:15-31). The global `PermissionSheet` renders over everything (P packages/app/src/App.tsx:29).

### F5. Session list

- **List.**
  - Plain `UICollectionView` list (Z Threads/SessionListController.swift:49-53).
  - Pull-to-refresh holds the spinner for 0.6–3s (Z Threads/SessionListController.swift:106-112; Z App/AppModel.swift:241-263).
- **Sections** (sidebar folders plus Pinned) collapse and reorder (Z Threads/SessionListController.swift:193-198,419-495).
  - Header: 40pt, SemiBold 13.5.
  - The chevron rotates -90° with a spring (0.3s, damping 0.85) (Z Threads/SessionCell.swift:234-306).
  - Folder cell: 46pt (Z Threads/SessionCell.swift:198-229).
- **Swipes and menus.**
  - Swipes: Pin (accent), Archive (secondary), Move (#5E6AD2) (Z Threads/SessionListController.swift:224-259). A context menu mirrors them (Z Threads/SessionListController.swift:261-292).
  - Archive shows the toast "Archived" + "Undo", auto-dismissed after 4s (Z App/AppModel.swift:417-435; Z Design/Toast.swift:102).
- **Search.** Placeholder "Sessions, projects, branches", limit 60 (Z Threads/SessionListController.swift:498-521; Z App/AppModel.swift:404-408).
- **Empty state.** Wallpaper image at 72% of the height, max 760 (Z Threads/SessionListController.swift:331-416).
- **Row** (Z Threads/SessionCell.swift):
  - Height 62 × TypeScale, clamped 0.85–1.6 (Z Threads/SessionCell.swift:49; Z Design/Palette.swift:114).
  - Pressed highlight: radius 16, inset 1pt vertically and 8pt horizontally (Z Threads/SessionCell.swift:79-85).
  - Corner word, colour and glyph (Z Threads/SessionCell.swift:95-104):

    | Condition | Word | Glyph |
    |---|---|---|
    | `sendFailed` (overrides status) | "Failed" | danger dot |
    | Working | "Working" | spinner |
    | Awaiting | "Input" | input dot |
    | Errored | "Failed" | failed dot |
    | Completed and unseen | "Done" | check |
    | Completed and seen, or idle | none; relative time shows | none |

  - Fonts (Z Threads/SessionCell.swift:106-155):
    - Title Medium 16.5; SemiBold when unseen.
    - Idle and seen rows use alpha 0.88.
    - Meta 13.5, branch 12.5, corner word Medium 13.
  - Geometry (Z Threads/SessionCell.swift:157-191):
    - Side margins 20.
    - Title y 10, height 22. Meta y 35, height 18.
    - Project mark 20. The 12pt glyph sits 5pt before the time.
- **StatusGlyph** is a cell grid (Z Design/StatusGlyph.swift:4-58):
  - Spinner: 2×3 violet cells, 750ms cycle.
  - Trailer: 3×3 pastel cells, used by the working pill.
  - Dot: 7pt. Check: stroke 1.6.
- **Pocket list** (P packages/app/src/screens/AgentsScreen.tsx:7-80):
  - Header "Agents" (22/700) with a "Disconnect" link.
  - Cards: radius 12, padding 14, 1pt border, 8pt tone dot. Title 15; `provider · cwd` 11.
  - Right-hand label: "Needs you" / "Failed" / "Done" / "Working".
  - Detached agents are disabled at opacity 0.5 ("Not attached · open on your Mac").
  - Empty state: "Start one on your Mac: pocketd run claude".
  - Sorted by urgency, then recency (P packages/app/src/status.ts:9-25).
  - Keep the urgency sort. Zeron sorts by recency and relies on the corner word; Pocket has fewer, hotter rows.
- **Pocket token sets.** AgentsScreen and ConnectScreen use `theme.ts` (accent #7aa2f7). ChatScreen uses `design.ts` (bg #0B0C0E, green #5BE38A) (P packages/app/src/theme.ts:1-14; P packages/app/src/design.ts:1-26). Unify these before copying visuals.

### F6. Session screen chrome

- **Header.**
  - Glass back circle.
  - Title SemiBold 16 over a subtitle 12 reading "project @ host", max width 240 (Z Session/SessionViewController.swift:528-562; Z Session/CoreSessionSource.swift:46). Screenshot `appshots-ios-portrait.png` shows "zeron @ MacBook Pro".
  - Menu: Pin / Copy Transcript / Archive (Z Session/SessionViewController.swift:503-524).
- **Status pill** (Z Session/SessionViewController.swift:565-643): glass capsule, 30pt tall, text 13 Medium. States:
  - **Working:** the pill supports trailer glyph + "\(word) · 12s" (1s tick; elapsed `Ns` / `Nm Ns` / `Nh Nm`, Z Session/SessionViewController.swift:609-637), but only the debug `FixtureSessionSource` sets it, with the fixed word "Thinking" (Z Session/SessionSource.swift:144). The real source never does: "Working state is shown at the transcript tail" (Z Session/CoreSessionSource.swift:86). That tail row is `WorkingIndicatorView`: trailer glyph + "Working…" or "Writing…" (streaming), elapsed in tertiary, Medium 13.5 (Z Transcript/RowView.swift:403-433). The 21 rotating flavour words ("Percolating", every 7s, seeded per chat) are desktop-only (Z crates/ui/src/transcript.rs:2186-2215); iOS does not use them.
  - **Offline:** "Offline — sends are saved" (idle dot).
  - **Reconnecting:** "Reconnecting in Ns".
  - **Not delivered:** "Not delivered · Tap to retry" (danger dot).
  - **Host offline** (send queued): "<host> is offline — will send when it's back" (Z Session/CoreSessionSource.swift:80).
  - **Editing:** "Editing queued message · Tap to cancel" (input dot).
- **Banner precedence:** explicit send failure > not delivered > queued for an offline host > app offline > room reconnecting; otherwise none (Z Session/CoreSessionSource.swift:75-85). Editing overrides from the view controller (Z Session/SessionViewController.swift:481).
- **Seen.** Opening or re-appearing marks the chat seen (Z Session/SessionViewController.swift:153,293).
- **Open animation.** A loader spins at 1.75× speed; content reveals over 0.18s; an 8s fallback applies (Z Session/SessionViewController.swift:221-254).
- **Pocket ChatScreen header** (P packages/app/src/screens/ChatScreen.tsx:126-166):
  - 44pt glass back circle.
  - Title pill 15 SemiBold with a mono 11 subtitle; provider in teal.
  - The diff pill and terminal button have no handlers.
  - Red error banner.
  - The Working row lives in the timeline, not the header: "Waiting for approval", "Compacting context 12s", or "Working 12s · model", with an asterisk pulse (800ms) and shimmer (P packages/app/src/components/TimelineView.tsx:67-97).

### F7. Transcript

- **Virtualization.** Rust owns geometry; overscan is 700pt (Z Transcript/TranscriptListView.swift:4-7,26).
- **Row kinds:** Markdown / User / Tools / Chip / Image / Working (Z crates/mobile/src/layout/rows.rs:27-34).
  - A pending user row carries a client-minted id equal to the id of the server echo, so the swap causes no flicker (Z crates/mobile/src/layout/rows.rs:36-41).
- **Geometry** (Z crates/mobile/src/layout/rows.rs:121-139):
  - Margin 18; reading width 768.
  - Gaps: first 14, turn 30, reply 18, block 12, heading 22.
  - User bubble: padding 15×10, radius 20, max width `max(0.86·cw, cw−56)` (Z crates/mobile/src/layout/rows.rs:684).
  - Fade 28. Fold at 8 lines, showing 6.
  - Thumb 76; chip line 32; image 260; working row 36.
- **Type** (Z crates/mobile/src/layout/style.rs:104-110):
  - Body 16.5/25.
  - Headings 22/29, 19.5/27, 17.5/25, 16.5/25.
  - Code 13.5/20; inline code 14.5; small 13.5/19.
- **Markdown:** paragraph gap 10, item gap 5; code block header 32, horizontal padding 14, radius 12 (Z crates/mobile/src/layout/markdown.rs:408-418).
- **Tools.**
  - Rail geometry (Z crates/mobile/src/layout/tools.rs:22-40): header 26, row 32, trunk x 12.5, bend 6, icon 16, text x 56, output line height 18. Output caps at 24 lines; diffs at 600.
  - Motion (Z Transcript/ToolViews.swift:4-14,231-233):
    - fold 0.14s
    - row reveal 0.36s on expo `(0.16,1,0.3,1)`
    - connector 0.48s quint
    - stagger 0.065s, lead-in 0.09s
    - shimmer sweep 3.4s
  - Screenshot `tool-activity.png` shows the group summary "Ran 1 command · edited 1 file · 1 search · 1 failed".
- **Send runway** (Z Transcript/TranscriptListView.swift:42-62; Z Session/SessionViewController.swift:67-82):
  - After a send, the user's prompt glides to the top of the viewport with `glideRetain 0.85` (about 90% of the way in about 230ms).
  - One viewport of space is reserved below so the reply streams into view.
  - A drag releases the reserve.
- **Follow rules** (Z Transcript/TranscriptListView.swift:224-257):
  - A finger-down drag always releases follow.
  - On release, follow re-latches only if the tail is within 70pt and velocity is ≤50.
  - Momentum re-engages follow only when moving toward the tail within 70pt.
  - A status-bar tap scrolls to the top (Z Transcript/TranscriptListView.swift:259-263).
- **Jump button** (Z Session/SessionViewController.swift:17,49-51,102-148,453):
  - 40pt glass circle with `arrow.down` 15.
  - Visible when more than 140pt from the tail and not following.
  - Placed 16pt from the trailing edge, 12pt above the composer.
  - Spring 0.35s, damping 0.8.
- **Motion:** fold 140ms ease-out; a grown group reveals over 360ms expo; new rows fade in over 0.28s (Z Transcript/TranscriptListView.swift:332-346,387).
- **Long-press:** Copy / Copy Message / Select Text; preview radius 16, inset 8 (Z Transcript/TranscriptListView.swift:593-618).
- **Pocket TimelineView** (P packages/app/src/components/TimelineView.tsx:174-241):
  - `BOTTOM_SLACK 32`; pinned state tracked `onScroll`; auto scroll to bottom when pinned.
  - Glass ChevronDown "Scroll to latest".
  - No runway and no velocity rule.

### F8. Composer

- **Two shapes** (Z Composer/ComposerBar.swift:32-40,308-333):
  - Resting capsule `[+] Message… [↑]`, radius 25.
  - Card with toolbar `[+] [model] [effort] [branch] … [Send]`, radius 26, toolbar 50, insets 14/6.
  - It becomes the card when chips are always visible, the field is focused, or images are attached.
  - Spring 0.42s, damping 0.86.
- **Metrics** (Z Composer/ComposerBar.swift:112-119,146-151,169,240-267):
  - Text: Sans 16.5, scaled with Dynamic Type.
  - Max lines: 2 at rest, 3 in compact vertical size, 8 in the card.
  - Control 34; resting height 50.
  - Attach button: plus glyph (15 semibold) on a `controlFill` capsule.
  - Text insets 14/14.
- **Chips** (Z Composer/ComposerBar.swift:425-444):
  - Symbol 11.5 semibold; tint fill at 0.1 opacity, ink at 0.85; capsule with insets 7/11.
  - PR chip: Mono Medium 12.5. Others: Sans Medium 13.5.
  - Screenshot `streaming-keyboard.png` shows chips `#77` (red tint), `main` and "GPT-5.6-Te…".
- **Context chip** (Z Session/CoreSessionSource.swift:51-73): appears at ≥50% context use as "NN% context"; warning tint at ≥85%.
- **Placeholder:** "Message <harness>" (Z Session/CoreSessionSource.swift:49).
- **Attachments.**
  - Up to 8 images (Z Composer/ComposerBar.swift:473-513): thumbs 56 with radius 14 and an 18pt remove "x"; thumb row 68.
  - Sources: Photo Library / Take Photo / Choose File / Paste Image. HEIC converts to JPEG; long side ≤2560px; ≤24 MB (Z Composer/AttachmentPicker.swift:5-6).
- **Mentions.** `@name` becomes `[name](zeron-file:path)`, disambiguated by parent folder, with a 120ms debounce (Z Composer/Mentions.swift:3-17; Z Composer/ComposerBar.swift:346).
- **Action button** (Z Composer/ComposerBar.swift:579-621):
  - `running ? (hasContent ? queue : stop) : send`.
  - Send and queue: accent background with a white `arrow.up` (15, bold). Disabled: `controlFill` background with a tertiary glyph.
  - Stop: text-colour background with `stop.fill` (11) in the background colour.
  - Changes bounce with a spring (0.32s, damping 0.8). Each state has an accessibility label.
- **Long-press "While the agent works"** (Z Composer/ComposerBar.swift:624-633):
  - "Queue for next turn".
  - "Steer now", subtitled "Deliver into the running turn"; only offered when the harness can steer.
  - "Stop and send" (destructive).
  - Backed by the delivery modes queue / steer / interrupt (Z Composer/ComposerBar.swift:12-20; Z Session/CoreSessionSource.swift:110-122).
- **Keyboard:** ⌘↩ sends (Z Composer/ComposerBar.swift:668-670).
- **Queue panel** (Z Composer/QuestionPanel.swift:217-328):
  - Glass card, radius 20, insets 10.
  - Up to 3 rows of 40pt, each with a clock (12, tertiary), a 26pt thumb (radius 6) and a label (Sans 15) formatted "gate · text".
  - Send button `arrow.up.circle.fill` 20.
  - Ellipsis menu: Edit / Move Up / Move Down / Remove (destructive).
  - Overflow line "+N more queued" (Medium 12.5, tertiary, 24pt).
- **Edit lease** on queued messages, renewed every 20s (Z Session/CoreSessionSource.swift:93-100,152-167).
  - Gate words: "Editing" / "Being edited" / "Needs review" / "Updating".
  - Conflict alert: "Can't edit right now" / "Another device is editing this message, or it was just sent." (Z Session/SessionViewController.swift:377-432).
- **Drafts** are saved per chat in UserDefaults under "drafts" when leaving the chat and when the app backgrounds; loaded on open (Z Session/SessionViewController.swift:37-39,84,279,646-658).
- **Pocket Composer** (P packages/app/src/components/Composer.tsx:15-91; P packages/app/src/screens/ChatScreen.tsx:51-53,173-179):
  - Text lives in local `useState`, so it is lost on unmount.
  - Attach and Dictate buttons have no handlers.
  - While busy, the only action is Stop.
  - Text 15; green send button 44.
  - Placeholder "Message <Provider>…"; `/compact` routes to `agent.compact`.

### F9. Questions and approvals from the phone

- **Zeron never asks permission.** Session create sends `auto_approve: true` (Z crates/client/src/session/mod.rs:1054). The Codex adapter notes "zeron sessions run unattended … never surface wire approvals" (Z crates/harness/src/codex/mod.rs:968-972).
- **What the phone answers.** The newest unresolved `MessagePart::Input` with questions (Z crates/client/src/session/mod.rs:678-697), via `respond_input(request_id, answers)` (Z crates/client/src/session/mod.rs:1090-1102; Z Session/CoreSessionSource.swift:130-132).
- **Question panel** (Z Composer/QuestionPanel.swift:3-213). It replaces the composer with a spring (0.38s, damping 0.86) (Z Session/SessionViewController.swift:459-501).
  - Design intent: "one glass card per question, large option rows, 'Other…' free text, single-select auto-advance (220ms) so a 3-question form is three taps."
  - Card: glass, radius 26, insets 16/16/16/12; stack spacing 10, with 14 after the question and after "Other".
  - Header "\(header or "Question") · \(page+1) of \(count)" when there are several questions, otherwise just the header; Medium 12.5 secondary (Z Composer/QuestionPanel.swift:113).
  - Single-select auto-advance fires after 0.22s (Z Composer/QuestionPanel.swift:166).
  - Question: SemiBold 17.
  - Options (spacing 6):
    - Symbol `N.circle`, becoming `checkmark.circle.fill` (single-select) or `checkmark.square.fill` (multi-select) when chosen; image padding 10.
    - Insets 11/12; Medium 16; radius 14.
    - Chosen: accent foreground on accent@0.12. Otherwise chip@0.55.
  - "Other…" field: 16pt, chip@0.6, radius 12, 44pt tall. Multiline editor: 140pt.
  - Navigation: "Back" (plain, secondary) and "Next", which becomes "Submit" on the last page (prominent glass capsule, text-colour background with background-colour foreground).
  - Feedback: selection haptic; pages crossfade over 0.2s; submit fires once with a medium impact haptic.
- **Pocket PermissionSheet** (P packages/app/src/components/PermissionSheet.tsx:32-100):
  - Modal slide-up sheet, radius 20.
  - "Do you want to proceed?", then "1. Yes", then provider options, then "No".
  - "No, and tell Claude what to do differently" with free text.
- **Pocket protocol support.**
  - `PermissionRequest {requestId, agentId, toolName, detail, options?, feedback?}` (P packages/protocol/src/timeline.ts:105-112).
  - `permission.resolve {decision, option, message}`.
  - pocketd replays open requests on hello (P packages/pocketd/internal/wsserver/wsserver.go:137-139) and rejects stale resolves: "Permission request is no longer open" (P packages/pocketd/internal/wsserver/wsserver.go:156-158).
- **Pocket bug: one permission slot.** A second concurrent request overwrites the first in the UI (P packages/app/src/session.tsx:61-63). The first can only be answered after a reconnect replays it.
- **Takeaway.** Keep Pocket's approval semantics and present them in Zeron's inline paged panel: one page per open request, numbered rows, "Other…" as feedback.

### F10. New session from the phone (Zeron only)

- **Canvas, composer first** (Z Session/NewSessionViewController.swift:41-43,108-165):
  - Title "New Session".
  - Hero "What are we building?" (SemiBold 22) with a 34pt harness mark, spacing 14, centred in the free space.
  - Placeholder "Describe the task"; chips always visible.
  - Composer ≤768 wide, 12 insets, 8pt above the keyboard.
- **Chips** (Z Session/NewSessionViewController.swift:239-359):
  - Project menu: No Project… / New Project…
  - "Run on" host, with Online / Offline subtitles.
  - Checkout: New worktree / Branch.
  - Model; "Reasoning effort".
  - Error: "Couldn't start the session" / "Choose a project or a host that can run it."
- **Created session defaults:** sandbox `.workspaceWrite`, worktree base `HEAD`, busy policy `.queue` (Z App/AppModel.swift:531-554).
- **Pocket:** the phone cannot spawn; the empty state points to the Mac (P packages/app/src/screens/AgentsScreen.tsx:24). pocketd spawns only over its local ops socket (P packages/pocketd/cmd/pocketd/serve.go:53-60).

### F11. Push notifications

**Client** (Z App/PushNotifications.swift):
- **Kinds and titles** (Z App/PushNotifications.swift:13-22):
  - `done` "Run finished"
  - `input` "Waiting on your input"
  - `failed` "Run failed"
- **Preferences.** A master toggle plus one per kind, all on by default (Z App/PushNotifications.swift:33-53).
- **When to ask.** After the first session started from the phone, or from Settings → Notifications (Z App/PushNotifications.swift:78-97; Z apps/ios/README.md:115-118).
- **Registration.**
  - Re-registers on every sign-in; unregisters on sign-out (Z App/PushNotifications.swift:67-74,108-112).
  - Syncs token, environment and prefs to the edge (Z App/PushNotifications.swift:126-137).
  - Detects the APNs environment from the build (Z App/PushNotifications.swift:142-153).
- **Foreground.** `completionHandler([])`, so nothing shows while the app is open (Z App/PushNotifications.swift:161-167).
- **Tap.** Reads `chatId` (Z App/PushNotifications.swift:170-182). It is held as `pendingChat` until the shell is shown, then opened (Z App/PushNotifications.swift:184-193; Z App/SceneDelegate.swift:19-23,69-73).
- **Not used:** notification categories and actions, `UIBackgroundModes` (Z Info.plist), badges. Entitlement `aps-environment` is `development` (Z apps/ios/Zeron.entitlements:5-6).
- **Settings rows** (Z Shell/MoreViewController.swift:132-161):
  - "Notifications" (bell). Subtitle "When a session finishes, needs you or fails", or "Turned off in iOS Settings" when denied.
  - When on, per-kind rows with `checkmark.circle` / `questionmark.bubble` / `exclamationmark.triangle`.
  - Denied alert: "Notifications are off" / "Allow notifications for Zeron in iOS Settings."

**Edge:**
- **Storage.** `POST /registry/:org/push-target` stores `{device, token, environment, prefs}` in the `push_targets` table, validated with `TOKEN_RE` (Z edge/src/registry-room.ts:59-84,105,233-262).
- **Stats.** `/stats` lists phones and the last 30 deliveries (`PUSH_LOG_MAX 30`) (Z edge/src/registry-room.ts:208-227).
- **Rule** (`notificationFor`), which diffs a `sessions` row before and after each batch (Z edge/src/push-notify.ts:37-56):
  - becomes errored → `failed`
  - becomes awaitingInput → `input`
  - a new `lastCompletedTurn` while fresh → `done`
  - a row seen for the first time only sets the baseline
  - Working → Idle without a completion is silent
  - rows older than 45s count as dead (Z edge/src/push-notify.ts:10-35)
- **Filtering.** Side chats and archived chats never notify. The title falls back to "New session" (Z edge/src/push-notify.ts:62-69).
- **Payload** (Z edge/src/push-notify.ts:91-99): `{aps:{alert:{title, body}, sound:"default", "thread-id":chatId}, chatId, category}`.
  - `category` is top level, not `aps.category`, so it is data only; no actions.
- **Delivery** (`notifySessions`, Z edge/src/registry-room.ts:503-555):
  - Each note × target pair is filtered by the target's prefs.
  - Log results: "not configured" / "no targets" / HTTP status + reason.
  - Tokens APNs reports dead are deleted.
  - Best effort; never retried; runs under `ctx.waitUntil`.
- **APNs** (Z edge/src/apns.ts:26-80):
  - Hosts: `api.push.apple.com` / `api.sandbox.push.apple.com`.
  - ES256 provider JWT (`kid`, `iss` = team, `iat`), cached 50 min.
  - Headers: `apns-topic`, `apns-push-type: alert`, `apns-priority: 10`, `apns-expiration: now+24h`, `apns-collapse-id: chatId[:64]`.
  - Dead token: 410, `BadDeviceToken`, `Unregistered` or `DeviceTokenNotForTopic`.
  - Secrets: `APNS_KEY_P8`, `APNS_KEY_ID`, team `5XY3M483YQ`, topic `sh.zeron.ios` (Z edge/src/env.ts:27-41; Z apps/ios/README.md:120-124).
- **Gap: no seen check.** Pushes fire even while the desktop shows the chat; only the phone's own foreground mutes them.

**Pocket already has the right trigger data:**
- `TurnEnded` sets `unseenEnd = !seen`, where seen means any connection (phone or desktop) shows the agent (P packages/pocketd/internal/agent/agent.go:117-143,215-225).
- `status()` yields needsYou / done / working / idle / closed (P packages/pocketd/internal/agent/agent.go:174-184).
- `update` publishes `agent.update` only on change (P packages/pocketd/internal/agent/agent.go:186-205).
- A push notifier can diff summaries off the same Hub a phone connection subscribes to (P packages/pocketd/internal/wsserver/wsserver.go:130-133).
- Missing today: no `expo-notifications` dependency (P packages/app/package.json:14-31) and no push field in `Config` (P packages/pocketd/internal/config/config.go:29-32).

**Expo facts** (https://docs.expo.dev/versions/latest/sdk/notifications/, https://docs.expo.dev/versions/latest/sdk/application/):
- `getDevicePushTokenAsync()` returns the native APNs token `{type:'ios', data}`.
- "The iOS APNs entitlement is always set to 'development'. Xcode automatically changes this to 'production' in the archive."
- `getIosPushNotificationServiceEnvironmentAsync()` returns `'development' | 'production' | null` (null on the simulator).
- `setNotificationHandler` decides foreground presentation, and must answer within 3s.
- Tap handling: `addNotificationResponseReceivedListener` / `useLastNotificationResponse`.
- Category actions support `opensAppToForeground`, `isAuthenticationRequired`, `isDestructive` and `textInput`.

### F12. Offline and reconnect

- **Posture: "hide-fast, show-slow"** (Z crates/client/src/connectivity.rs:1-56):
  - `DEGRADE_GRACE_MS 4_000`: blips under 4s never show.
  - `UNDELIVERED_GRACE_MS 120_000`: a send is marked Failed only after 2 min.
  - `PRESENCE_FRESH_MS 45_000`.
  - States: Disabled / Offline / Reconnecting / Connected.
  - `SendState`: Sending / Queued / Failed.
  - The FFI exposes `{state, retry_at_ms, last_failure, degraded_chats}` (Z crates/mobile/src/client_ffi/types.rs:287-305).
- **Lifecycle** (Z crates/client/src/client.rs:1410-1451):
  - `set_network_online` parks or unparks every backoff and kicks rooms when back online.
  - `on_foreground` kicks rooms, probes health and restarts PR watches.
  - `on_background` flushes the registry and docs and stops watches.
  - Wiring: `NWPathMonitor` (Z App/AppModel.swift:75-80); scene background / foreground (Z App/SceneDelegate.swift:96-102).
- **Timings:**
  - Backoff 250ms → 16s cap, reset after 30s stable; offline park re-check 30s; connect timeout 20s (Z crates/sync/src/registry.rs:30-49; same in Z crates/sync/src/chat_client.rs:27-43).
  - Socket ping 15s; 45s silence lease (Z crates/sync/src/socket.rs:18-19).
- **Durability.** A send is written to the chat doc (so it survives app kill) and nudged to the host (F2). The phone shows the "host is offline — will send when it's back" banner.
- **Pocket** (P packages/app/src/client.ts:5-61; P packages/app/src/App.tsx:22-26):
  - Backoff 1s doubling to a 30s cap.
  - Any close → `offline` → the chat unmounts and ConnectScreen shows "Disconnected — retrying" (P packages/app/src/screens/ConnectScreen.tsx:59).
  - `send` is `this.ws?.send`, a silent no-op when disconnected.
  - Acks and errors are not correlated to sends (P packages/app/src/session.tsx:47-71).
  - pocketd pings every 20s and drops a connection after a 10s timeout, so a sleeping phone doesn't keep agents "seen" (P packages/pocketd/internal/wsserver/wsserver.go:22-27,84-99).
  - Resume exists in the protocol (`agent.timeline sinceSeq`, `maxSeq`, `epoch`; P packages/protocol/src/messages.ts:4-61), but the app refetches the page on mount (P packages/app/src/screens/ChatScreen.tsx:97-100).
  - The app goes non-viewing on AppState change (P packages/app/src/screens/ChatScreen.tsx:39-48,101-105).

### F13. iPad

- **Split view** (Z Shell/SplitRootController.swift):
  - Primary column 360pt (range 320–400) (Z Shell/SplitRootController.swift:36-44).
  - Keys: ⌘N new session, ⇧⌘F search, ⌘B toggle sidebar (0.3s) (Z Shell/SplitRootController.swift:53-59).
  - New session is embedded in the secondary column (Z Shell/SplitRootController.swift:87-109). Settings opens as a `.formSheet` (Z Shell/SplitRootController.swift:111-119).
  - The chat carries over on collapse and expand (Z Shell/SplitRootController.swift:146-174).
  - Sidebar toolbar: gearshape, ellipsis, prominent plus (Z Shell/SplitRootController.swift:181-257).
- **Width caps.** The reading column caps at 768 (Z crates/mobile/src/layout/rows.rs:123), as does the composer (Z Session/SessionViewController.swift:102-148).
- **Orientation.** iPhone: portrait plus both landscapes. iPad: all four (Z Info.plist). Screenshot `landscape-keyboard.png`.
- **Pocket.** `supportsTablet: true` but `orientation: portrait` and no split layout (P packages/app/app.json:6,11).

### F14. Settings (Zeron)

- **Sections:** Account / Devices / Notifications / Appearance / Wallpaper / Sessions / Sign Out (Z Shell/MoreViewController.swift:181-206,237).
- **Placement.** Settings is a tab on iPhone and a form sheet on iPad (F4, F13).
- **Pocket** has no settings screen; "Disconnect" is the only control (P packages/app/src/screens/AgentsScreen.tsx:14-17).

### F15. Side-by-side

| Concern | Zeron iOS | Pocket app |
|---|---|---|
| Stack | UIKit + Rust/UniFFI (Z apps/ios/README.md:3-10) | Expo RN, `newArchEnabled` (P packages/app/app.json:9) |
| Link to Mac | Cloud DO relay (Z edge/src/device-room.ts:1-16) | Direct `ws://` over Tailscale or LAN (P packages/pocketd/cmd/pocketd/serve.go:46-69) |
| Identity | WorkOS account (Z Shell/SignInViewController.swift:4-19) | Shared daemon token (P packages/pocketd/internal/config/config.go:52-60) |
| Approvals | None, auto-approve (Z crates/client/src/session/mod.rs:1054) | Yes, PermissionSheet with a single slot (P packages/app/src/session.tsx:61-63) |
| Questions | Paged glass panel (Z Composer/QuestionPanel.swift:3-5) | None: `AskUserQuestion` only flags "Needs you" (P packages/pocketd/internal/daemon/plugin.go:13; P packages/pocketd/internal/daemon/daemon.go:108-109) |
| Push | Edge APNs, 3 kinds (Z edge/src/push-notify.ts:46-99) | None |
| Seen model | `unseen` → "Done" check; no push suppression (Z Threads/SessionCell.swift:95-104) | `unseenEnd` from live views (P packages/pocketd/internal/agent/agent.go:215-225) |
| Offline | Graced pill; durable sends (Z crates/client/src/connectivity.rs:1-56) | Eject to ConnectScreen; drops sends (P packages/app/src/App.tsx:22-26) |
| Send while working | Queue / steer / stop-and-send (Z Composer/ComposerBar.swift:624-633) | Stop only (P packages/app/src/components/Composer.tsx:49-55) |
| Drafts | Per chat, persisted (Z Session/SessionViewController.swift:646-658) | Lost on unmount (P packages/app/src/components/Composer.tsx:15-23) |
| New session | Phone canvas (Z Session/NewSessionViewController.swift:41-43) | Mac only (P packages/app/src/screens/AgentsScreen.tsx:24) |
| iPad | Split, 360 sidebar, shortcuts (Z Shell/SplitRootController.swift:36-59) | Portrait phone layout (P packages/app/app.json:6) |

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 08-1 | pocketd sends APNs on attention transitions: needsYou → input; done → done; done+failed → failed; first sight = baseline; closed/detached skipped | Leave the desk; the phone buzzes when an agent needs you or finishes | Z edge/src/push-notify.ts:37-99; Z edge/src/apns.ts:26-80; Z edge/src/registry-room.ts:503-555 | pocketd: **new** `internal/push` (port apns.ts: ES256 JWT cached 50 min, HTTP/2, same headers; port notificationFor over `AgentSummary` diffs from Hub); `config.Config` gains `apns{keyPath,keyId,teamId,topic}` | L | Paid Apple Developer account; Push capability on `dev.mingo.anywhere`; dev-client build (not Expo Go); 08-2 |
| 08-2 | Phone registers its token and prefs over the existing socket: `push.register {token, environment, prefs{done,input,failed}}` / `push.unregister`; re-send on every `hello.ok`; ask for permission after the first prompt sent from the phone | Opt-in at the moment it makes sense; per-kind control | Z App/PushNotifications.swift:33-153 | protocol: **new** messages, `PROTOCOL_VERSION` 3→4 (P packages/protocol/src/constants.ts:1); app: **new** `push.ts` using expo-notifications and expo-application; pocketd: `~/.coding-pocket/push.json` (0600) | M | 08-1 |
| 08-3 | Tap opens that agent (held as pending until online); muted in foreground; collapse-id and thread-id = agentId | One tap from lock screen to the right chat; no double alerts | Z App/PushNotifications.swift:161-193; Z App/SceneDelegate.swift:19-23,69-73 | app: **port**; `setNotificationHandler` → no banner; `useLastNotificationResponse` → `setAgentId` in P packages/app/src/App.tsx:17 | S | 08-1, 08-2 |
| 08-4 | Seen-aware rule: "done"/"failed" only if `unseenEnd`; "input" suppressed while any view shows the agent | No buzz for a run you just watched finish on the Mac (Zeron lacks this) | P packages/pocketd/internal/agent/agent.go:117-143,215-225; Zeron gap: Z edge/src/registry-room.ts:503-555 | pocketd: **new** (inside 08-1) | S | 08-1 |
| 08-5 | Push diagnostics: `pocketd push status` lists devices and the last 30 deliveries; prune dead tokens on 410/BadDeviceToken/Unregistered/DeviceTokenNotForTopic | Debuggable "why didn't it buzz" | Z edge/src/registry-room.ts:208-227,558-566; Z edge/src/apns.ts:48-50 | pocketd ops CLI: **port** | S | 08-1 |
| 08-6 | Graced connectivity: stay in ChatScreen when the socket drops; show a status pill only after 4s: "Offline — sends are saved", "Reconnecting in Ns" | No ejection to ConnectScreen on every blip | Z crates/client/src/connectivity.rs:1-32; Z Session/SessionViewController.swift:600-632 | app: **adapt** P packages/app/src/App.tsx:22-26, P packages/app/src/client.ts (expose `retryAt`), ChatScreen pill | M | none |
| 08-7 | Redial immediately on AppState active and on NetInfo online (reset backoff); resync with `agent.timeline sinceSeq=maxSeq`; pull-to-refresh on the list = redial + `agent.list` | Fresh on unlock instead of waiting up to 30s of backoff | Z crates/client/src/client.rs:1410-1451; Z App/AppModel.swift:75-80,241-263 | app: **adapt** client.ts; add `@react-native-community/netinfo`; protocol already has `sinceSeq` (P packages/protocol/src/messages.ts) | S | 08-6 |
| 08-8 | Outbox: prompts get a client id; kept until `ack {id}`; resent on reconnect; "Not delivered · Tap to retry" after 120s; pocketd dedupes by client id | Prompts typed on a train are not silently lost | Z crates/client/src/connectivity.rs:17,46-56; Z crates/mobile/src/layout/rows.rs:36-41 | app **new** outbox in session.tsx; pocketd **new** recent-id set in wsserver `dispatch` (P packages/pocketd/internal/wsserver/wsserver.go:191-208) | M | 08-6 |
| 08-9 | Per-agent drafts persisted on blur and background; restored on open | Switching agents or backgrounding keeps half-typed text | Z Session/SessionViewController.swift:37-39,646-658 | app: **port** into P packages/app/src/components/Composer.tsx:15-23 (lift state; AsyncStorage or SecureStore) | S | none |
| 08-10 | Agent row redesign: title Medium 16.5 (SemiBold when Done-unseen), meta line `provider · cwd`, trailing glyph + word (Working spinner / Needs you dot / Failed dot / Done check, else relative time), 62pt, no card borders | Scannable at a glance; matches desktop | Z Threads/SessionCell.swift:49,95-191; Z Design/StatusGlyph.swift:4-58 | app: **adapt** P packages/app/src/screens/AgentsScreen.tsx:25-79; keep `byUrgency` | M | Unify theme.ts and design.ts |
| 08-11 | Live summary line "2 working · 1 needs you" in the list header or bottom bar | Know the fleet state without scanning | Z Shell/MainTabController.swift:190,252-258 | app: **new**, derived from `agents` | S | none |
| 08-12 | Send while working: primary becomes Queue when there is text, Stop when empty; long-press menu "Queue for next turn" / "Steer now" / "Stop and send" | Add a follow-up without killing the run | Z Composer/ComposerBar.swift:579-633; Z Session/CoreSessionSource.swift:110-122 | app: **adapt** Composer.tsx:49-55; pocketd: define `agent.prompt` semantics while working per driver (queue vs inject) | M | Driver behaviour check (claude/codex) |
| 08-13 | Inline approval panel replaces the composer: one glass page per open request ("Bash · 1 of 2"), numbered option rows, "Other…" feedback, submit once + haptic; hold all open requests, not one | Faster, thumb-reachable approvals; no lost second request | Z Composer/QuestionPanel.swift:3-213; Z Session/SessionViewController.swift:459-501; bug P packages/app/src/session.tsx:61-63 | app: **adapt** PermissionSheet.tsx into an inline panel; session.tsx `permission` → `Record<requestId, PermissionRequest>` | M | none |
| 08-14 | Follow rules: drag releases follow; re-latch within 70pt at ≤50 velocity; jump button (40pt glass) visible >140pt from the tail | Reading back while streaming without being yanked | Z Transcript/TranscriptListView.swift:224-257; Z Session/SessionViewController.swift:17,49-51,453 | app: **adapt** P packages/app/src/components/TimelineView.tsx:174-241 | S | none |
| 08-15 | Send runway: after send, scroll the prompt to the top and reserve one viewport below; release on drag | The reply streams in where your eyes are | Z Transcript/TranscriptListView.swift:42-62; screenshot `send-runway.png` | app: **new** in TimelineView (FlatList footer spacer = viewport height) | M | 08-14 |
| 08-16 | Working pill in the header: trailer glyph + "Percolating · 1m 4s" (flavour word every 7s, seeded per agent). Not what Zeron iOS ships: its pill supports a working state but the real source leaves working to the transcript tail; flavour words are Zeron desktop | Alive feeling; elapsed time without scrolling to the tail | Pill capability Z Session/SessionViewController.swift:600-637 (unused, Z Session/CoreSessionSource.swift:86); words Z crates/ui/src/transcript.rs:2186-2215 (desktop) | app: **adapt** the TimelineView Working row text (P packages/app/src/components/TimelineView.tsx:67-97) into a header pill | S | 08-6 (same pill) |
| 08-17 | Context chip "NN% context" at ≥50%, warning at ≥85% | Know when to /compact from the phone | Z Session/CoreSessionSource.swift:51-73 | protocol: add context usage to `AgentSummary` (P packages/protocol/src/timeline.ts:83-99); pocketd drivers report it | M | Drivers expose token usage |
| 08-18 | QR pairing: `pocketd pair` prints a terminal QR of `anywhere://pair?host=<ts-ip>:4517&token=…`; the iOS Camera app opens it; ConnectScreen fills and saves | Zero typing; no mistyped tokens | Zeron has no pairing (account-based, Z Shell/SignInViewController.swift:91-117); Pocket scheme P packages/app/app.json:7 | pocketd: **new** subcommand; app: **new** `Linking` handler in ConnectScreen | S | none |
| 08-19 | Lock-screen actions on permission pushes: category `permission` with "Allow" / "Deny" (`isAuthenticationRequired`, Deny `isDestructive`); `aps.category` set | Approve from the lock screen | Zeron has none (category is top level, Z edge/src/push-notify.ts:91-99); Expo action options (URL) | app: **new** (background handler must reconnect and send `permission.resolve` within about 30s); pocketd: **new** payload with `requestId` | L | 08-1..08-3; background socket spike |
| 08-20 | iPad split: 360pt (320–400) sidebar of agents + chat; ⌘B toggles the sidebar, ⇧⌘F search; unlock orientation on iPad | Real tablet use | Z Shell/SplitRootController.swift:36-59,146-174 | app: **new** layout on width ≥ 768; P packages/app/app.json:6 orientation | L | 08-10 |
| 08-21 | New session from the phone (project, provider, worktree chips; "What are we building?") | Start work away from the Mac | Z Session/NewSessionViewController.swift:41-359; Z App/AppModel.swift:531-554 | protocol **new** `agent.spawn`; pocketd exposes Spawn over ws (today ops socket only, P packages/pocketd/cmd/pocketd/serve.go:60) | L | Security review of remote spawn |
| 08-22 | Hosted relay (DeviceRoom-style) for access off the tailnet, with durable nudges | Works without Tailscale | Z edge/src/device-room.ts:1-16,209-253 | **new** service | XL | Contradicts Pocket's no-cloud stance |

## UI/UX spec to copy

Values are Zeron's code values. Map colours to Pocket's dark `design.ts` tokens: bg `#0B0C0E`, card `#15171A`, text `#EDEBE6`, muted `#A1A4AA`, faint `#7E828A`, rule `#1D2024`, green `#5BE38A`, red `#F07167` (P packages/app/src/design.ts:1-26). Zeron dark equivalents are background 060606, elevated 111113, text E8E8EA, secondary A9A9AE, tertiary 6B6B72, hairline 1E1E22, accent 8B7CF6, danger F87171, success 34D399, warning FACC15 (Z Design/Palette.swift:13-32).

### S1. Agent row (replaces the AgentsScreen card)
- Full-bleed row, no border. Height 62; side margins 20. Pressed: fill `card`, radius 16, inset 1pt vertically and 8pt horizontally.
- Line 1 (y 10, h 22): title.
  - Geist Medium 16.5, colour `text`.
  - SemiBold when status is Done (unseen) or Needs you.
  - Idle and seen rows: alpha 0.88.
- Line 2 (y 35, h 18): `provider · cwd-basename`, 13.5, `muted`. Detached rows: "Not attached · open on your Mac" at 0.5 opacity.
- Trailing corner (Medium 13, with a 12pt glyph 5pt before the word):
  - Needs you: warn dot, "Needs you".
  - Failed: red dot, "Failed".
  - Done: green check (stroke 1.6), "Done".
  - Working: 2×3 cell spinner (750ms), "Working".
  - Otherwise: relative time in `faint`.
- Sort: keep `byUrgency` (P packages/app/src/status.ts:23-25).
- Header: large title "Agents" SemiBold 30. Summary "N working · N need(s) you" in Medium 13 `muted` with the 12pt glyph. Overflow menu: Notifications…, Disconnect.

### S2. Status glyph (cell grid)
- Dot: 7pt circle.
- Spinner: 2×3 cells (3.5pt, gap 1.5), violet ramp, 750ms loop.
- Trailer: 3×3 cells (3.5pt, gap 1.75), pastel B6D3EF / EDB185 / F888A0.
- Check: stroke 1.6.
- Tone alphas: working 0.55, input 0.6, failed 0.65, done 0.9, idle 0.14.
- Source: Z Design/StatusGlyph.swift:4-58.

### S3. Chat header and status pill
- Header:
  - 44pt glass back circle (Pocket already has it, P packages/app/src/screens/ChatScreen.tsx:126-160).
  - Title SemiBold 16 over a 12pt subtitle `provider @ hostname`, max width 240. The hostname is already in `hello.ok` (P packages/protocol/src/messages.ts:37-61).
- Pill: centred under the header; glass capsule, 30pt tall, 13 Medium, 12pt glyph leading. Precedence:
  1. "Not delivered · Tap to retry" (red dot; tap resends the outbox)
  2. "Offline — sends are saved" (idle dot; shown only after 4s of disconnect)
  3. "Reconnecting in Ns" (idle dot)
  4. "Waiting for approval" (warn dot)
  5. "<Word> · 1m 4s" (trailer glyph, 1s tick; elapsed `Ns` / `Nm Ns` / `Nh Nm`). Pocket-specific addition (08-16); Zeron iOS shows working at the transcript tail instead.
  6. Hidden
- Remove the header diff and terminal buttons until they have handlers (P packages/app/src/screens/ChatScreen.tsx:147-159).

### S4. Composer
- Resting capsule:
  - Radius 25, height 50, 8 insets.
  - `[+]` 34pt control on a `chip` capsule; text Geist 16.5 at 10 after the attach button, max 2 lines.
  - `[↑]` 34pt circle.
- Focused card:
  - Radius 26, insets 14/6, text 16 from the edge, max 8 lines.
  - 50pt toolbar row for chips (provider/model; "NN% context" at ≥50%, red at ≥85%).
  - Spring 0.42s, damping 0.86.
- Primary button:
  - Idle with text: Send, green bg with a dark `arrow.up` (bold 15). Empty: disabled `chip` bg with a `faint` glyph.
  - Working with text: Queue (same look).
  - Working and empty: Stop, `text` bg with a `bg`-coloured `stop.fill` (11).
  - Change animation: bounce spring 0.32s, damping 0.8.
- Long-press menu titled "While the agent works": "Queue for next turn", "Steer now" (subtitle "Deliver into the running turn"; only if the driver supports it), "Stop and send" (destructive).
- Placement: max width 768, 12 side insets, 8pt above the keyboard.
- Draft: persisted per agent.

### S5. Inline approval panel (Pocket permission → Zeron question look)
- Replaces the composer (spring 0.38s, damping 0.86). Glass card, radius 26, insets 16/16/16/12, stack spacing 10.
- Header: "`toolName` · 1 of N", Medium 12.5 `muted`. N is the number of open requests for this agent.
- Body: `detail` in Geist Mono 13.5 inside a `code` block (radius 12, horizontal padding 14), max 8 lines with a fade, then "Show more".
- Question: "Do you want to proceed?", SemiBold 17, 14pt below.
- Option rows, spacing 6:
  - Medium 16, radius 14, insets 11/12.
  - Leading symbol `1.circle`, `2.circle`… with image padding 10.
  - Rows: "Yes", then the provider options, then "No".
  - Chosen: green text on green@0.12. Others: chip@0.55.
- "Other…" row: a 44pt field (16pt, chip@0.6, radius 12) whose placeholder is "Tell Claude what to do differently". It maps to `decision:"deny", message`.
- Single-choice tap submits after 220ms. The Submit button is shown only for "Other…".
- Submit fires once (disable, medium haptic). Pages crossfade over 0.2s when more requests remain. A stale resolve error ("Permission request is no longer open") silently drops that page.
- Source: Z Composer/QuestionPanel.swift:10-213; P packages/app/src/components/PermissionSheet.tsx:32-100.

### S6. Transcript scroll
- Follow:
  - Drag start releases follow.
  - On release, re-latch if the distance to the tail is <70pt and signed pan velocity y ≤50 (any fling toward the tail, or a slow drift away).
  - Momentum toward the tail re-latches within 70pt.
- Jump button: 40pt glass circle with a chevron-down (15); 16pt from the trailing edge, 12pt above the composer. Visible when more than 140pt from the tail and not following. Spring 0.35s, damping 0.8.
- Runway: on send, animate the new user row to the top (about 230ms) and reserve a footer equal to the viewport height minus the composer; the first drag removes it.
- Bubble: radius 20, padding 15×10, max width `max(0.86·w, w−56)`. Fold above 8 lines to 6 with "Show more".
- Gaps: turn 30, reply 18, block 12.

### S7. Push copy and payload (pocketd)
- Title: agent title, falling back to the cwd basename.
- Body:
  - done "Run finished"
  - failed "Run failed"
  - needsYou "Waiting on your input"
- Payload: `{aps:{alert:{title,body}, sound:"default", "thread-id":agentId}, agentId, kind}`.
- Headers: `apns-push-type: alert`, `apns-priority: 10`, `apns-expiration: now+86400`, `apns-collapse-id: agentId[:64]`, `apns-topic: dev.mingo.anywhere`.
- Host: `api.sandbox.push.apple.com` for `development` tokens, `api.push.apple.com` for `production`.
- Source: Z edge/src/push-notify.ts:58-99; Z edge/src/apns.ts:26-71.

### S8. Notification settings (sheet from the Agents overflow menu)
- "Notifications" row (bell) with a toggle. Subtitle "When an agent finishes, needs you or fails", or "Turned off in iOS Settings" when denied.
- When on: "Run finished" (`checkmark.circle`), "Waiting on your input" (`questionmark.bubble`), "Run failed" (`exclamationmark.triangle`), each with a toggle.
- Denied alert: "Notifications are off" / "Allow notifications for Anywhere in iOS Settings." with an Open Settings button.
- Source: Z Shell/MoreViewController.swift:132-161.

### S9. iPad
- Width ≥ 768 shows a split: sidebar 360 (320–400) with S1 rows, chat on the right.
- Reading column and composer cap at 768, centred.
- ⌘B toggles the sidebar (0.3s). ⇧⌘F focuses search. ⌘↩ sends.
- All four orientations on iPad.
- Source: Z Shell/SplitRootController.swift:36-59.

### S10. Pairing
- `pocketd pair` prints a QR of `anywhere://pair?host=<tailscale-ip>:<port>&token=<token>` plus the plain text.
- The app handles the link by pre-filling ConnectScreen, saving to SecureStore, then connecting.
- On failure: "Rejected — token or version mismatch".

## Open questions / risks

- **APNs credentials.**
  - Push needs a paid Apple Developer account, the Push capability on `dev.mingo.anywhere`, and a `.p8` key stored on the Mac running pocketd.
  - Whose key is it if Pocket is ever distributed? Each user self-signing means each user needs their own key.
  - The alternative is Expo's push service, which still needs the key uploaded to Expo and adds a third party.
- **Mac asleep means no push.** pocketd runs on the Mac. Zeron has the same limit for host-side state, but its edge still pushes on registry diffs it receives (Z edge/src/registry-room.ts:503-555). Is "Mac awake" acceptable?
- **Lock-screen privacy.** Should the push body include `toolName`/`detail` (for example a Bash command)? Zeron sends only fixed copy (Z edge/src/push-notify.ts:59-60). Proposal: fixed copy by default, details opt-in.
- **Seen window.** A phone that backgrounds without its `agent.view([])` reaching pocketd keeps the agent "seen" for up to about 30s (20s ping + 10s timeout; P packages/pocketd/internal/wsserver/wsserver.go:22-27). A done push could be wrongly suppressed. Should push ignore phone-only views?
- **Duplicate prompts.** The retry outbox (08-8) needs server-side idempotency. pocketd acks after `Driver().Prompt` returns (P packages/pocketd/internal/wsserver/wsserver.go:191-207), so an ack lost to a drop causes a re-send. Needs a client message id.
- **Prompt while working.** What do the Claude and Codex drivers do with `agent.prompt` mid-turn? This is unverified in this report and gates 08-12.
- **Background actions (08-19).** A notification action gets only a short background window to open `ws://`, send hello and resolve. It may be unreliable; it may need a small HTTP endpoint on pocketd.
- **Plain ws on LAN.** pocketd listens on all interfaces (P packages/pocketd/cmd/pocketd/serve.go:46). Over Tailscale the link is WireGuard-encrypted; on LAN the token travels in clear text. A QR code (08-18) makes LAN pairing easier and so raises this risk.
- **Android.** Pocket builds for Android (P packages/app/app.json:14-17). A direct APNs port covers iOS only; FCM would be a second sender.
- **Stale sources.** Zeron screenshots and `mobile-polish.md` predate the current UIKit/Rust code (F1). Spec values here come from code, but no current-build screenshot was available to confirm visuals.

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 8.

Confirmed against source: UIKit/Rust peer-device architecture and DO relay (frame format, 75s host liveness, 4409 supersede, `host_offline`, nudge cap 4096/503/5s alarm); `auto_approve: true`; push rule, payload, APNs headers, 50-min JWT, dead-token pruning, `/stats` log of 30; client push kinds, ask-after-first-session, foreground mute, tap → `pendingChat`; iPhone tabs and accessory summary; iPad 360 (320–400) split with ⌘N / ⇧⌘F / ⌘B; composer action/long-press menu and ⌘↩; connectivity graces 4s/120s/45s; Pocket eject-on-drop, silent send no-op, local-state drafts, Stop-only while busy, single permission slot, portrait-only, no `expo-notifications`, `Config{Token,Port}`, seen/unseenEnd machine, 20s+10s keepalive, protocol v3. Ideas-table Pocket mappings point at existing files and lines.

Corrections:
- Working status: Zeron iOS does not show working in the header pill; the real `CoreSessionSource` leaves it to a transcript tail row ("Working…"/"Writing…" + elapsed). Only the debug fixture sets a pill working state. Fixed in TL;DR, F6, 08-16, S3.
- Flavour words (21, every 7s) are Zeron desktop (`crates/ui`), not iOS. Fixed in F6 and 08-16.
- Banner precedence ended with "> working"; the source has no working banner. Fixed.
- Drafts save on leaving the chat as well as on backgrounding. Fixed in F8.
- Question panel header shows "· N of M" only with several questions; added the 0.22s auto-advance citation. Fixed in F9.
- Follow re-latch uses signed velocity y ≤ 50 and distance < 70, not |velocity|. Fixed in S6.
- Pocket "Questions: via permission options only" was unsupported; `AskUserQuestion` only flags Needs you. Fixed in F15.
- Pocket URL is `ws://<host>` with a user-typed port, not a hardcoded `:4517`. Fixed in F2.
