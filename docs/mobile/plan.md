# iOS app build plan

Read with `docs/adr/0007-native-ios-app.md` (decisions), `docs/mobile/protocol.md` (the wire contract) and the research in `docs/mobile/research-*.md` (what the desktop does, with file:line sources).

## Toolchain on this machine

- No Xcode. Swift 6.2.4 from swiftly: `export PATH=$HOME/.swiftly/bin:$PATH`. macOS SDK 14.2 (CLT). swift-testing works (`import Testing`); XCTest does not exist.
- Build one module with your own scratch dir so parallel builds don't fight over `.build`:
  `cd packages/ios/Packages/AnywhereKit && swift build --scratch-path /tmp/ak-<you> --target <Module>` and `swift test --scratch-path /tmp/ak-<you> --filter <Module>Tests`.
- Speed: `swift build -c release` for any measurement; C deps are `-O0` in debug.
- iOS-only code: `make -C packages/ios typecheck-ios` typechecks it and the three Xcode targets' sources as Mac Catalyst; `docs/mobile/ios-build-checklist.md` has the first Xcode build and what that can't verify.
- pocketd: `cd packages/pocketd && go build ./... && go test ./internal/<pkg>/...`. libghostty is prebuilt in `third_party/ghostty/zig-out`. Use a scratch home for anything that starts pocketd: `POCKET_HOME=$(mktemp -d)`; never touch the user's live pocketd. `TestProvidersShowWhatIsInstalledAndTheCeiling` in `internal/launch` already fails on this machine before any change (it depends on the installed claude); ignore it.
- node for `packages/protocol`: `mise exec node@22 -- npx --yes pnpm@10 <args>`.

## Package `packages/ios/Packages/AnywhereKit`

Platforms `.iOS(.v18), .macOS(.v14)`, `swiftLanguageModes: [.v6]`. Dependencies: `swiftlang/swift-cmark` branch `gfm` (cmark-gfm + extensions) and `migueldeicaza/SwiftTerm` pinned to 1.20.0 (1.99.x doesn't compile against the macOS 14.2 SDK under Swift 6).

| Module | Imports | Owns |
|---|---|---|
| `Wire` | Foundation | Codable mirrors of every client and server message (existing + protocol.md), `ServerMessage`/`ClientMessage` enums switching on `type`, binary terminal frames, caps, JSON coding helpers |
| `Client` | Wire, Network, Security | `DaemonConnection` actor (NWConnection + NWProtocolWebSocket, `ws://` on tailnet/loopback, else `wss://`), id-correlated `request`, `send`, event `AsyncStream`, backoff 0.25 s→8 s with jitter, close-code outcomes (4401 unpaired, 4426 version), pairing link parse, `Keychain`, `Credentials` |
| `Domain` | Foundation | pure rules: status order and Up next, elapsed formatting (`12s`, `3m 4s`, `1h 2m`, `1d 3h`), 24 h times, `Route`, `DeepLink.parse`, project/worktree grouping, path display |
| `ChatModel` | Wire | ports of desktop `crates/chat`: `timeline.rs` fold → `Transcript`, `rows.rs` → `Row`s with stable ids, `session.rs` splicing, `outgoing.rs` send state machine, `marks.rs`, `commands.rs`, `mcp.rs`, `spawned.rs`; reveal pacing (`pocket/chat/reveal.rs`) and pin maths (`pin.rs`) as pure functions |
| `MarkdownModel` | cmark-gfm | cmark-gfm (table, strikethrough, tasklist, autolink, footnotes) → `MDBlock`/`MDInline` values; desktop block split (`rows.rs markdown_blocks`), fence parse, GitHub alerts, footnote numbering, path-chip detection (`markdown.rs path_in`), math left literal / `$$` as code |
| `Highlight` | Foundation | fast lexer highlighter for the desktop's languages (bash, css, go, html, java, javascript, kotlin, markdown, python, rust, swift, toml, tsx, typescript, yaml, json) emitting token ranges; the Xcode light/dark token palette from `research-chat.md` §3.4 as data |
| `DiffModel` | Wire | git.diff lines → rows, hunk renumbering for chat edit previews (`pocket/chat/diff.rs`), side-by-side pairing, word ranges, commit-graph lanes (port of `crates/git/src/graph.rs Lanes::push`), change tree folding (`git_ui/changes.rs`) |
| `ActivityModel` | Foundation | `MacActivity` attributes + `ContentState` (protocol.md), ActivityKit conformance under `#if canImport(ActivityKit) && os(iOS)` |
| `PushSupport` | Wire, CryptoKit | `Notice`, AES-GCM open/seal (Go `gcm.Seal` layout), category ids, relay register client, badge sum across Macs |
| `Stores` | Wire, Client, Domain, ChatModel, DiffModel, Observation | `@MainActor @Observable` stores: `AppModel` (every Mac), `HostModel` per Mac (its `DaemonConnection`, link state, stores, events coalesced per 16 ms), `SessionsStore` (one `@Observable SessionModel` per agent), `ChatStore` per open agent (timeline + rows + outgoing), `WorkspaceStore`, `TerminalStore`, `GitStore`, `GitHubStore`, `FilesStore`, `PushStore`. Each store conforms to `HostStore`: `apply(_ msg) -> Bool`, plus `connected(hello)` to resubscribe and `disconnected()`; `HostModel` hands every message to every store (a chat sees only its agent's), so no feature edits the routing. Each store takes a `DaemonConnection?` for its own request methods. Each store file belongs to its feature. For the chat composer, `FilesStore` offers `search(tree:query:limit:)` (fuzzy go-to-file over fs.search, best first, for @ mentions), `upload(_ data:name:id:)` (an image as attach.put chunks; returns its absolute path on the Mac, with progress 0...1 in `uploads[id]`; throws `AttachmentError` for a type or size pocketd refuses), and `mention: FileMention?`, which the Files screen sets when the reader picks "Mention in Chat" (a new `id` each time) |
| `DesignSystem` | SwiftUI, Domain | colours from `packages/desktop/crates/theme` (light/dark), type scale, spacing, `Shimmer`, `Pill`, `FileChip` + language badge colours, `StatusGlyph`, `Glass` (iOS 26 glass inside `#if os(iOS)` + availability, material otherwise), haptics shim |
| `MarkdownView` | SwiftUI, MarkdownModel, Highlight, DesignSystem | renders `MDBlock`s: paragraphs as one `Text` of `AttributedString`, headings, lists (nested, ordered start, task boxes), quotes, alerts, code blocks (label, wrap toggle, copy, highlight off-main), tables (framed, aligned, horizontal scroll), rules, images, footnotes, path chips, links |
| `CodeView` | SwiftUI, Highlight, DiffModel, DesignSystem | virtualized fixed-row code and diff renderer (CoreText/`Canvas` per visible row; only the scroll host is iOS-specific), gutter, word-diff highlights, wrap off by default |
| `FeatureChat` | Stores, ChatModel, MarkdownView, CodeView, DesignSystem | the chat screen: every row view, composer, permission card, question wizard, trays, goal, btw, model/effort/access pills, account chip, context ring, sticky prompt, jump pill, agent map sheet |
| `FeatureSessions` | Stores, DesignSystem | projects → worktrees/Locals → sessions, Up next, new session (LaunchSpec), worktree create/delete, Locals, automations list |
| `FeatureTerminal` | Stores, SwiftTerm, DesignSystem | terminal list, attach view (SwiftTerm `TerminalView` under `#if os(iOS)`), key bar (esc, tab, ctrl, arrows), fit-to-phone |
| `FeatureGit` | Stores, CodeView, DiffModel, DesignSystem | changes (flat/tree, staged/unstaged, stage, discard, commit, push, pull), diff screen, commit graph, commit detail, branches |
| `FeatureGitHub` | Stores, MarkdownView, DesignSystem | PR card with gate, checks, reviews, threads (reply/resolve), create/merge/ready, PR list of a project |
| `FeatureFiles` | Stores, CodeView, MarkdownView, DesignSystem | file tree (lazy, git letters), fuzzy go-to-file, preview: code, markdown (rendered/source), image, binary/too-large states |
| `FeaturePairing` | Stores, Client, DesignSystem | scan QR (iOS) / paste link / enter code, confirm, reconnect/revoked/update screens |
| `FeatureSettings` | Stores, DesignSystem | Macs, notification prefs, Live Activity toggle, terminal access state, about |
| `AppShell` | all features | `RootView` (each feature's public entry view takes a `HostModel` and the `Route` it shows: `SessionsView`, `ChatView`, `TerminalsView`, `GitView`, `GitHubView`, `FilesView`; `PairingView` and `SettingsView` take the `AppModel`), iPhone `NavigationStack` / iPad `NavigationSplitView`, `Route` destinations, deep links, scene phase (send `agent.view([])` on background) |
| `LiveActivityUI` | ActivityModel, DesignSystem, WidgetKit | Lock Screen + Dynamic Island views and `AnswerIntent`, all under `#if os(iOS)` |
| `Fixtures` | Wire | sample servers messages and timelines used by tests, the gallery and previews |

Executables (macOS only, for proof and speed): `anywhere-probe` (drives a real pocketd through `Client` + `Stores`), `anywhere-gallery` (renders screens from `Fixtures` to PNG with `NSHostingView`), `anywhere-bench` (timings for markdown, rows, diff, highlight, status decode at 1000+ files).

Thin Xcode targets in `packages/ios/`: `App/` (`AnywhereApp`, `AppDelegate` for APNs + notification categories and actions, entitlements: aps-environment, app group `group.dev.mingo.anywhere`, keychain group, time-sensitive), `Widgets/` (WidgetBundle with the Live Activity), `NotificationService/` (opens `e`, sets title/subtitle/body/thread/badge).

## App shell: how the app is laid out

- **iPhone: three tabs, Sessions, Terminals and Settings**, each its own `NavigationStack` of `Route`s, scoped to the selected Mac. Switch Macs from the title menu of Sessions or Terminals. Changes, PRs and files are not tabs: each needs a tree, so they are pushed from a worktree's screens. A tab for them would repeat the sessions tree as a picker.
- **iPad: `NavigationSplitView`.** The sidebar lists each Mac with its Sessions and Terminals, plus Settings. The middle column is that section's list. The right column is a stack for the chat, diff or file it opens. It shares the iPhone's `Navigator`, so a size-class change keeps the place. The workspace slice's project tree goes into the sidebar under each Mac (marked `INTEGRATION` in `PadRoot.swift`).
- **`Destinations.swift` is the one place a `Route` meets a feature view.** Features push `NavigationLink(value: Route…)` and never import each other.
- **Deep links.** `onOpenURL` takes the scheme, notification taps (`AppDelegate` opens their link) and the Live Activity's `widgetURL`. `DeepLink.notification(userInfo)` reads a notification. A chat link selects its Mac at once and is held until that Mac says hello.
- **Connection banners.** "Reconnecting…" appears once a drop lasts 4 s, and "Can't reach" with a Tailscale hint after 30 s (`Reachability`). A Mac that revoked this phone, or that needs a side updated, shows `EndedView` in place of its sections.
- **Restoration.** The tab, the selected Mac and every stack are saved as JSON in `SceneStorage`. Appearance lives in `AppStorage("appearance")`.
- **Proof.** `anywhere-gallery <outdir> [filter]` renders every feature's `Gallery.scenes` to PNG. It first checks that a `ScrollView`'s `LazyVStack` actually draws. macOS draws prominent buttons unfilled in an offscreen window; iOS fills them. `scripts/mobile-e2e.sh` pairs `anywhere-probe` with a scratch pocketd and runs its checks. Each slice's check runs once pocketd offers its cap.

## Rules for every agent

- Follow `docs/mobile/protocol.md` exactly; if it is wrong or missing something, change the doc in the same change and say so in your summary.
- Desktop parity: port behaviour from the Rust source named in the research docs, with its tests (as swift-testing `@Test("sentence")`). No mocks, no render tests.
- Performance: nothing grows with the repo or the transcript on the main actor. Decode off-main. Lists that grow (chat rows, changes, files, diff, log) are lazy and keyed by stable ids; rows are `Equatable` value views. Each `@Observable` object is small so one change redraws one row.
- Model modules never import SwiftUI. iOS-only API inside `#if os(iOS)`.
- The local SDK's SwiftUI predates `View` being main-actor isolated, so mark views `@MainActor` and static helpers tests call `nonisolated`.
- Comments only for the why the code can't show.
