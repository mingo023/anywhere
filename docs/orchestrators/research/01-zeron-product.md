# 01 · Zeron: product and feature inventory

Date: 2026-09-30.
Sources:
- `zeronsh/zeron` (formerly `zeronsh/comet`) @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5`, cloned at `/Users/mingo/tmp/orchestrators/zeron`.
- https://zeron.sh.
- https://zeron.sh/releases/latest.txt, which returns `0.2.99`.
- `gh api repos/zeronsh/comet/releases --paginate` (162 releases).
- Pocket worktree `/Users/mingo/.worktrees/coding-pocket/orchestrator-research`.

Citation legend:
- `Z path:L` is a file and line in the Zeron clone, relative to its root.
- `P path:L` is the same for the Pocket worktree.
- `M` (monocode) is not used in this report.
- `R vX.Y.Z` is the GitHub release note at `https://github.com/zeronsh/zeron/releases/tag/vX.Y.Z`.
- A bare URL is the live site.
- `:1` or a line range such as `:1-5` points at a module's doc header, where the feature is described by its implementer.
- When docs and code disagree, code wins. The disagreement is flagged in §F10.

## TL;DR

- **What Zeron is.** A native desktop app, a headless engine and an iOS app that drive existing agent CLIs from one window across all your machines (Z README.md:1, `apps/landing/public/index.html:871`).
  - It is not a harness and not a PTY wrapper. It speaks each CLI's structured protocol and renders its own transcript and composer (Z crates/harness/src/claude/mod.rs:1-30).
  - It supports 9 agents: Claude Code, Codex, Cursor, Devin, Grok, Hermes, Pi, OpenCode and Antigravity (Z crates/proto/src/agent.rs:7-28).
- **Target user.** A solo developer running many agent sessions in parallel on several machines (laptop, home server, cloud box) who wants to watch and steer them from any device.
  - The pitch: "Start Claude Code at your desk, watch it from your phone, approve the diff from the couch" (Z apps/landing/public/index.html:986).
- **Local-first by default.** No account is needed. Sign-in via WorkOS switches to a synced profile on the next restart (Z ARCHITECTURE.md:44-70).
  - Synced devices are fully trusted with each other's files.
  - There is no E2EE: the transport is TLS plus WorkOS bearers (Z docs/PARITY.md:101).
- **Unattended by design.** Claude gets `bypassPermissions` when `auto_approve` is set, and the `can_use_tool` handler auto-allows every tool except `AskUserQuestion` either way (Z crates/harness/src/claude/mod.rs:209-217, 967-1008).
  - Codex yolo mode forces `danger-full-access` (Z crates/harness/src/codex/mod.rs:1-37).
  - "Needs you" therefore means only questions, not permissions.
- **Status model is nearly Pocket's.**
  - The states are Working, Input, Failed, Done (unseen on every device) and Idle, plus the send states Queued and Failed.
  - Rows go stale after 45 s (Z crates/proto/src/view.rs:41).
  - The sidebar sorts by recency; status only drives the dot (Z crates/proto/src/view.rs:93-99).
- **Notifications on three surfaces.**
  - Desktop banner plus chime, with a background-only option (Z crates/ui/src/shell.rs:2585-2760).
  - iOS APNs push computed at the edge, with `thread-id = chatId` so a newer push replaces the older one (Z edge/src/push-notify.ts:1-100).
  - Copy: "Run finished", "Waiting on your input", "Run failed".
- **Workspace surfaces around the chat.**
  - Changes pane showing a live branch diff, with review comments that get folded into the next prompt.
  - Git history graph, and PR badges read through the host's `gh`.
  - File editor, terminal (alacritty_terminal), native browser tabs, P2P dev-server previews, project actions and worktrees (§F3).
- **Distribution.**
  - Artifacts: signed and notarized macOS arm64 DMG, Windows per-user installer plus a portable ZIP, and a Linux tarball plus curl installer.
  - iOS is on TestFlight; the App Store is "coming soon".
  - The desktop auto-updates: it checks hourly, downloads in the background and installs on quit (Z README.md:60).
- **Business model.** Free, MIT-licensed and sponsor-funded. The sync edge is hosted at `edge.zeron.sh` at no charge, and no paid tier exists anywhere (Z README.md:64-74, https://zeron.sh).
- **Cadence.** 162 releases in 70 days, from v0.1.0 on 2026-07-22 to v0.2.99 on 2026-09-30, averaging 2.3 a day with a peak of 10 a day.
  - The last week ran at 2 a day, falling to 1 a day over the last 3 days.
- **Best ideas for Pocket:**
  - Push, plus chime, background-only toggle and Zeron's copy on top of Pocket's existing desktop notifications.
  - Mod+1–9 jump, Ctrl+Tab, archive.
  - PR badges.
  - Diff-line comments folded into the next prompt.
  - Project actions (Pocket already has a per-repo worktree setup command).
  - Pins and sections.
  - Daemon service install with self-update when idle.
  - An MCP server that lets agents drive sessions.

## Findings

### F1. Positioning, target user, product shape

- **Title.** "Zeron — control your coding agents from any device" (Z apps/landing/public/index.html:6).
- **Hero.** "The open source workspace for your coding agents. Zeron runs Claude Code, Codex, OpenCode, Cursor, Antigravity, and more on your machines." (Z apps/landing/public/index.html:871).
- **Section headings and captions** (Z apps/landing/public/index.html:959-984):
  - "Runs what you already use — Zeron is not a new harness."
  - "Every machine's sessions, one list"
  - "Branch diffs, live as agents work"
  - "Commit history for every workspace"
  - "Three screens, one session"
- **Social proof.** "Used by developers from" Cursor, Unsloth and The Context Company, plus a tweet wall (Z apps/landing/public/index.html:883).
- **Topology** (Z ARCHITECTURE.md:7-42; port from Z apps/zeron/src/main.rs:288):
  - One `zeron` binary runs either headed (the GPUI desktop with an embedded engine on IPC port 27654) or as `zeron headless`.
  - Engines on different devices meet through the Cloudflare DeviceRoom Durable Object relay.
  - Each chat is a Loro CRDT document holding messages and a command queue. The host device executes commands such as run, steer, interrupt and respondInput.
- **The unit of work is a chat, not a terminal.** Each chat belongs to a project (a folder on one device), which fixes the host device and the working directory (Z crates/mcp/src/tools.rs:71).
  - This inverts Pocket, where the unit is a PTY terminal and the agent status is layered on top (P CONTEXT.md:32, 42).

### F2. Providers (harnesses)

- **Harness IDs** (Z crates/proto/src/agent.rs:7-28): ClaudeCode, Codex, Cursor, Devin, Grok, Hermes, Pi, Opencode, Antigravity and Mock.
- **Claude** is driven with `claude --print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --replay-user-messages --thinking-display summarized --permission-prompt-tool stdio` (Z crates/harness/src/claude/mod.rs:166-200).
  - A `model[1m]` suffix selects the 1M-token context (Z crates/harness/src/claude/mod.rs:200-204).
  - `--effort` maps the reasoning level (Z crates/harness/src/claude/mod.rs:206-208).
  - Sessions resume with `--resume=<id>` (Z crates/harness/src/claude/mod.rs:218-220).
  - Steering writes user lines to stdin mid-turn, and the CLI folds them in at its next step boundary (Z crates/harness/src/claude/mod.rs:27-30).
  - Subagent frames are wrapped separately and never folded into the parent (Z crates/harness/src/claude/mod.rs:23-26).
- **Codex** runs as `codex app-server` over JSON-RPC, validated against codex-cli 0.153.4 (Z crates/harness/src/codex/mod.rs:1-37).
  - Steering uses `turn/steer`.
  - Interrupt escalates from `turn/interrupt` to SIGTERM to SIGKILL.
  - Subagents run as child threads.
- **ACP harnesses:** Cursor `cursor-agent acp`, Devin `devin acp`, Grok `grok agent stdio`, Hermes `hermes acp` and Antigravity `agy_acp_server` (Z crates/harness/src/acp/).
- **Other protocols:** Pi uses `pi --mode rpc` and requires ≥0.85.1 (R v0.2.99). OpenCode uses `opencode serve` over HTTP/SSE (Z crates/harness/src/opencode/).
- **Agent CLI update monitoring** (Z crates/engine/src/harness_updates.rs:1-3, Z crates/proto/src/agent.rs:33-42):
  - Policies: `Notify` (default), `AutoWhenIdle` or `Off`.
  - Detected install sources: Npm, Homebrew, Cargo, Vendor, ManagedByZeron or Unknown.
  - Update phases run from Dormant through ManualActionRequired and Failed.
  - The desktop banner reads "{n} agent update(s) available" after a 1 s debounce (Z crates/ui/src/shell.rs:2585-2760).
- **Install action** for every missing agent (R v0.2.83, Z crates/harness/src/install.rs, adapter_install.rs).
- **Agent accounts** (Z crates/engine/src/agent_accounts/stores.rs:1-25):
  - Multiple saved login slots per agent, with a live credential swap that rewrites each CLI's own auth file (0600, under the CLI's lock).
  - "Add account" runs a device-code or PKCE login against a throwaway `$HOME`, so a remote login works without a tunnel.
- **Usage meters** show each account's plan-window usage and reset time (Z crates/engine/src/agent_accounts/usage.rs:1-15, Z crates/ui/src/settings/accounts.rs:51-72, 148).
- **Context ring** under the composer turns warning colour at ≥75% and danger colour at ≥90% (Z crates/ui/src/context_usage.rs:11-12).
  - v0.2.89 added the plan usage display beside the ring (R v0.2.89).
- **Model picker** opens with Mod+/ (Z crates/ui/src/settings.rs:1109). It is virtualized for about 7k models (R v0.2.23), has harness tabs (R v0.2.28) and remembers model options (R v0.2.62).
- **Slash picker** lists harness slash commands, skills and Zeron actions such as `/model` and `/new` (Z crates/harness/src/claude/mod.rs:487, docs/harness-skill-completion.md:1).
- **Automatic titles** are generated from the first prompt by a configurable harness and model, run outside the repo (Z crates/engine/src/titles.rs:1-3).
  - On failure the title falls back to the prompt's first words.
  - A user rename always wins.

### F3. Feature inventory by area (confirmed in code)

**Sessions and sidebar**
- **Status indicator** (Z crates/proto/src/entities.rs:259-278, Z crates/proto/src/view.rs:30-82):
  - The values are Working, AwaitingInput ("Input"), Errored ("Failed"), Completed ("Done", meaning `chat.unseen()`) and Idle.
  - The send-state overlays "Queued" (warning) and "Failed" (danger) take priority.
  - A Working or Input status older than `SESSION_STALE_MS = 45_000` renders as nothing.
  - `attention_rank` runs Input 0 < Failed 1 < Working 2 < Done 3 < Idle 4, but the sort is recency only: "status drives the DOT, never the position".
- **Seen marker** is synced across devices as `lastSeenAt` (Z crates/proto/src/entities.rs:259-278).
- **Sidebar organization** can be ByProject, ByDevice or InOneList, sorted by LastUpdated or Created (Z crates/ui/src/settings.rs:605, 614).
- **Row toggles** (Z crates/ui/src/settings.rs:730+):
  - compact
  - project icon
  - harness
  - branch
  - pull request
- **Synced pins** are stored per `sidebarPins/{sessionId}` (Z crates/ui/src/shell/sidebar_pins.rs, docs/regressions/sidebar-pin-integrity.md:1).
- **Custom sections** are account-synced (Z crates/ui/src/shell/sidebar_sections.rs:1). The screenshot shows Pinned, custom groups such as "Focus" and "Later" (empty state "Drop sessions here"), Sessions and Archived (Z docs/screenshots/sidebar-sections/sections.png).
- **Archive** with Mod+Shift+A, plus Settings → Archived (Z crates/ui/src/settings.rs:1127, Z crates/ui/src/shell.rs:515-545).
- **Navigation** (Z crates/ui/src/settings.rs:148, 1116-1127):
  - Mod+1–9 jumps to a slot (`JUMP_SLOTS = 9`).
  - Ctrl+Tab and Ctrl+Shift+Tab step through sessions, or through right-pane tabs depending on context (docs/regressions/contextual-tab-navigation.md:1).
- **Command palette** opens with Mod+K and covers actions (New chat, New project, Open settings) plus chat-history search.
  - Footer hints: "↑↓ Navigate, ↵ Open, esc Close" (Z crates/ui/src/shell.rs:488, Z crates/ui/src/shell/command_palette.rs:1, docs/screenshots/command-palette/actions-and-history.png).
- **Side chats** are child chats of a parent, with a selectable harness (R v0.2.97). They never notify (Z crates/ui/src/shell/side_chats.rs:3, Z edge/src/push-notify.ts:1-100).
- **Explorer** shows Subagents and Chats sections, with running subagents listed first (R v0.2.92, R v0.2.99).
- **Project icons** follow Conductor's filename priority for repo artwork, raster-bounded to 64 px (Z crates/ui/src/shell/project_icon.rs, docs/screenshots/sidebar-sections/README.md).
  - The fallback is an initial on one of 8 hashed colours, drawn as an 8% tone background with an 85% letter.
- **Projectless sessions** are supported (R v0.2.55), as is a projectless file explorer (R v0.2.92).

**Multi-device and sync**
- **Scope** is fixed when the engine starts (Z ARCHITECTURE.md:49-68):
  - Local stores data in `{data_dir}/profiles/local/`.
  - Synced stores data in `{data_dir}/orgs/{org}/{user}/`.
- **Login and logout** apply at the next start (Z ARCHITECTURE.md:60).
- **Remote workspaces:** list, read and write files, run agents and run terminals on another device through the edge relay (Z README.md:42).
- **Message queue** (Z docs/reference/message-queue.md:1, Z crates/ui/src/queue.rs:94-116, 1064):
  - The queue is shared by desktop and iOS and stored as a Loro movable list inside the session doc.
  - Messages always queue while a response is running.
  - The primary action is "Send now (interrupt)".
  - Steer was removed as a user action in v0.2.53 (R v0.2.53).
- **Local-to-synced switch wizard** (Z crates/ui/src/shell.rs:8815-8860, Z crates/engine/src/local_import.rs:1-23):
  - The buttons are "Later", "Start fresh" and "Bring my work".
  - The import is idempotent and marked in `local-import.json`.
- **Sync capacity:** 28 clients per profile and 32 chat sockets. The doc recommends 20 main agents plus 8 viewed chats (Z docs/sync-capacity-calibration.md:1).

**Git**
- **Changes pane** shows a live checkout diff (Z crates/engine/src/diff_sync.rs:1-12, Z docs/research/feature-inventory.md:179):
  - One bounded snapshot per checkout, capped at 3 MiB and hashed with sha256.
  - Pushed to the edge so remote devices can review it.
  - Side-by-side view (R v0.2.20) and synchronized scrolling (R v0.2.32).
  - Options: `diff_split` and `diff_wrap` (Z crates/ui/src/settings.rs:730+).
- **Branch changes mode:** base → head ref selector and a "N Changed files vs <base> +A −D" header (Z apps/landing/public/assets/app-screenshot.jpg).
- **Review comments** are pinned to a diff line or file line, staged in the composer and folded into the next prompt as plain text (Z crates/ui/src/comments.rs:1-5, R v0.2.4). They are editable (R v0.2.62).
- **Discard** of the working tree is safe (R v0.2.91).
- **Git history graph:** paged commits with a topological lane graph (Z crates/ui/src/history.rs:1, R v0.1.55).
- **PR status** comes through the host's `gh` install and auth (Z crates/engine/src/source_control.rs:183, 204).
  - Cache TTL is 2 min when a PR exists and 45 s when none does; failure backoff starts at 20 s (Z crates/engine/src/change_requests.rs:23-25).
  - Badges are coloured by state on desktop and iOS, e.g. #90 green, #84 purple, #77 red (Z docs/screenshots/mobile-polish/home-glass.png).
- **Worktrees** are created at `~/.zeron/worktrees/<repoName>/<worktreeName>` with an auto-generated name (Z crates/engine/src/repos.rs:7).
- **"Add project"** works from other drives (R v0.2.11).

**Project actions**
- Named shell commands per project, run from the chat title bar (Z crates/engine/src/project_actions.rs:32, docs/reference/project-actions.md:1).
- An optional setup action runs on each new worktree.
- Actions are stored device-private in `project-actions.json`.
- `zeron.json` candidates from the repo need explicit import.
- Remote runs use `targetDeviceId` (R v0.2.80).

**Files**
- A right-sidebar editor with a searchable tree and git-status decorations (Z crates/ui/src/files/{editor,tree,search,git_status}.rs, R v0.2.47).
  - Save with Mod+S.
  - Autosave defaults to 900 ms and accepts 100–10000 (Z crates/ui/src/settings.rs:33-56).
  - Options for word wrap and "Show ignored files". `.git` is always hidden (Z README.md:42).
- **Markdown preview:** 900 px reading column, Mermaid, add-comment on blocks (Z docs/markdown-preview.md:1, R v0.2.61).
- **Image preview:** zoom 1%–3200% (Z docs/image-preview.md:1).
- **Copy Path** and jump to referenced lines (R v0.2.91).

**Terminal**
- The emulator is `alacritty_terminal 0.26` plus `vte 0.15` (Z crates/ui/src/terminal/emulator.rs:1-20).
- Engine limits (Z crates/engine/src/terminals.rs:34-37, 229-231):
  - 32 terminals maximum; beyond that: "Too many open terminals (maximum 32)".
  - 1 MiB replay buffer.
  - Exited terminals are swept after 30 min.
- Input is coalesced at 12 ms, resize is debounced at 80 ms, the background is #090909, and an exited process prints "[process exited N]" (Z docs/research/feature-inventory.md:103-104).
- The dock toggles with Mod+J. Height is 160 px minimum, 280 px default and 55vh maximum (Z crates/ui/src/settings.rs:33-56).

**Browser and previews**
- Native browser tabs live in the right sidebar: WKWebView on macOS and WebKitGTK 4.1 on Linux (Z crates/ui/src/browser/mod.rs:1, docs/reference/linux-browser.md:1, R v0.2.50).
  - Reload with Mod+Shift+R.
  - An `open_web_links_in_zeron` setting exists.
- **Project previews:** HTTP discovery of dev servers with stable routes at `http://<device>.<project>.localhost:7331`, reachable on remote devices over authenticated P2P (Z crates/preview/src/lib.rs:1-3, docs/preview-networking.md:1).
  - Edge signalling never carries the request bodies.

**Composer and transcript**
- **Composer** (Z crates/ui/src/composer.rs, settings.rs:569, 730+):
  - Auto-grows from 76 to 260 px (Z docs/research/feature-inventory.md:64).
  - Send key is `Enter` or `Mod+Enter`.
  - Markdown editing and reference chips (R v0.2.83).
  - Attachments (Z crates/ui/src/attachments.rs).
  - Escape stops the agent; this is opt-in (`escape_stops_active_agent`, R v0.2.60).
- **Screenshot placeholder** in the composer is "Do anything…", with model and effort chips and a round send button (Z apps/landing/public/assets/app-screenshot.jpg).
- **QuestionPanel** renders `AskUserQuestion` options. Keys 1–9 pick an option and auto-advance after 220 ms (Z crates/ui/src/composer.rs:104).
- **Appshots** (Z crates/ui/src/appshots.rs:1-5, docs/appshots.md:1):
  - A global shortcut captures the frontmost app window into the composer. It is Ctrl+Alt+Space on macOS and Mod+Alt+Space elsewhere (Z crates/ui/src/settings.rs:1099-1100).
  - Optional Accessibility context is added to the prompt, marked as untrusted.
- **Transcript** (Z crates/ui/src/transcript.rs):
  - Fold rows such as "Ran 8 commands · edited 1 file · read 5 files" (app-screenshot.jpg).
  - Compact mode, "Worked for …" (R v0.2.83).
  - Per-subagent transcripts (R v0.2.6).
  - A stick-to-bottom band of 70 px (Z ARCHITECTURE.md).
  - Transcript width setting and `code_fences_fit_content` (Z crates/ui/src/settings.rs:730+).
- **Message rail:** a vertical minimap of the user's prompts at the left edge (Z crates/ui/src/rail.rs:1-4).
  - Hovering grows the tick and shows a preview card; clicking smooth-scrolls.
  - The rail hides below a 48rem width.
- **WorkingIndicator:** a matrix spinner plus a rotating flavour word, 20 words cycling every 7 s (Z docs/research/feature-inventory.md:113).

**Notifications**
- **Desktop delivery** (Z crates/ui/src/notify.rs:1-80):
  - macOS uses NSUserNotification with an osascript fallback, and clicking routes to the chat.
  - Linux uses `notify-send`.
  - Windows has no banner, only the chime.
  - `ZERON_DISABLE_NOTIFICATIONS` turns banners off.
- **One detector feeds both banners and chimes** (Z crates/ui/src/shell.rs:2585-2760):
  - The first observation seeds a baseline silently. Side chats never notify.
  - Bodies: Done → "Run finished", Input → "Waiting on your input", Failed → "Run failed". The title is the chat title, or "New session".
  - `notifications_background_only` suppresses banners while any Zeron window has focus.
  - Connectivity banner: "Connection unavailable", with "Your device is offline" or "Zeron is trying to reconnect".
- **Sounds** have separate toggles for completion, input and attention (Z crates/ui/src/settings.rs:730+, R v0.2.62).
- **iOS push** (Z edge/src/push-notify.ts:1-100, called from edge/src/registry-room.ts:513-535):
  - Computed at the edge RegistryRoom, so it requires synced sign-in.
  - Categories are done, input and failed, all on by default, with per-device preferences.
  - Side chats and archived chats are skipped.
  - The APNs payload sets `thread-id: chatId` and `sound: default`.

**Settings**
- **Sections:** General, Appearance, Notifications, Shortcuts, Providers (the harnesses, with accounts folded in), Devices, Files, Appshots and Archived (Z crates/ui/src/shell.rs:515-545).
- **Storage:** `ui-settings.json`, with writes debounced by 400 ms (Z crates/ui/src/settings.rs:33-56).
- **Appearance** (Z crates/ui/src/settings.rs:730+, docs/theme-system.md:1, docs/adr/0002-separate-surface-preference-from-theme-selection.md:1):
  - Theme family and variant, with VS Code theme import.
  - Accent.
  - Surface, Frosted or Opaque.
  - UI, terminal and code fonts, each with a size.
  - New-thread backgrounds and a wallpaper folder with shuffle (Mod+U).
  - `reduce_motion` and `pause_animations_in_background` (R v0.2.99).
- **Shortcuts** are rebindable. A combo that is already bound is refused and the dialog names its owner (Z crates/ui/src/settings/shortcuts.rs:261, 463).

**Updates**
- **Desktop** (Z README.md:60, Z crates/ui/src/app_update.rs:12-14, 404):
  - Checks at start, hourly, and on wake.
  - Downloads in the background and verifies.
  - Shows "Update ready — restart to apply" in the sidebar.
  - Installs on quit if the user never restarts.
  - `ZERON_AUTO_UPDATE=0` reports updates without downloading.
- **Headless services** stage and apply updates themselves, then restart once no agent run and no terminal is active (Z crates/update/src/lib.rs:984-1016, Z README.md:62).
- **Manifest:** `manifest.json` with SHA-256 hashes, polled hourly from `{edge}/releases`.

**Mobile (iOS)**
- A SwiftUI and UIKit shell over a Rust core (`crates/mobile`: client_ffi, layout). The screens are Threads, Session, Transcript, Composer and Shell (Z apps/ios/Zeron/, docs/mobile-rewrite.md:1).
- The transcript is a UITableView (docs/mobile-polish.md:1).
- Row status sits in the top-right corner (Z apps/ios/Zeron/Threads/SessionCell.swift:94-102):
  - a "Failed" dot, a "Working" spinner, an "Input" dot,
  - a "Done" check while unseen,
  - otherwise the timestamp.
- The home screen has an "All" filter pill, + and account buttons, a collapsible Archived section, and remote rows such as "edge @ hetzner-01" (Z docs/screenshots/mobile-polish/home-glass.png).
- Push notifications (Z apps/ios/Zeron/App/PushNotifications.swift). An Android version is planned (Z docs/mobile-rewrite.md:1).

**CLI and MCP**
- **Subcommands** (Z apps/zeron/src/main.rs:14-82):
  - `zeron [open_url]`
  - `headless`, `login`, `logout`, `status`, `sync`, `appshot` (Linux only), `mcp`
  - `daemon {install|uninstall|start|stop|restart|status}`
  - `update [--check]`, which exits 1 when an update is available
- **Edge URL:** `DEFAULT_EDGE_URL=https://edge.zeron.sh`, overridable with `ZERON_EDGE_URL`.
- **`zeron mcp`** is a stdio MCP server proxying to `ws://127.0.0.1:$ZERON_IPC_PORT` (Z docs/mcp.md:1, R v0.2.82).
  - Tools (Z crates/mcp/src/tools.rs:58-183): `whoami, list_devices, list_projects, list_harnesses, list_models, list_chats, get_chat, create_chat, read_chat, send_message, wait_for_turn, interrupt_chat, respond_to_input, archive_chat`.
  - Chats created this way link to their parent.

### F4. First run and onboarding

- **No sign-in wall.** The app opens local-only, and the sidebar footer reads "Local only" (Z docs/screenshots/command-palette/actions-and-history.png, ARCHITECTURE.md:55).
- **Empty state:** a boot splash with a dot loader, then a bare new-session canvas with the target pickers (project, device, harness, model) above the composer (Z docs/research/feature-inventory.md).
- **Missing agents** show an Install action (R v0.2.83).
- **Sign-in** can start from the desktop or from `zeron login`.
  - Organisation gate: "Create your workspace".
  - The switch wizard follows (Z crates/ui/src/shell.rs:8815-8860, 9013-9015):
    - "Later", "Start fresh" or "Bring my work".
    - Then "Quit and reopen Zeron to start the synced workspace. Existing local sessions stay on this device and will not be uploaded."
- **"Star on GitHub" banner**, dismissible (`github_star_banner_dismissed`, R v0.2.95).

### F5. Install, update, distribution

- **Artifacts** under `https://zeron.sh/releases/`, with `latest.txt` holding the current version (Z apps/landing/public/downloads.js:2-37):
  - `zeron-<v>-macos-arm64.dmg`
  - `windows-x86_64-setup.exe`
  - `windows-x86_64.zip`
  - `linux-x86_64.tar.gz`
  - `linux-aarch64.tar.gz`
- **Version shown on the site.** The static fallback is 0.2.97; the live `latest.txt` returns 0.2.99.
- **macOS.** Developer ID signed and notarized (R v0.1.54). Only the arm64 DMG exists; there is no Intel build.
  - `zeron daemon install` installs a launchd service (Z README.md:54).
- **Windows.** A per-user Inno Setup installer that needs no admin rights, installs to `%LOCALAPPDATA%\Programs\Zeron` and registers `zeron://`.
  - The portable ZIP needs `zeron-update.json` beside the exe (Z README.md:56, docs/reference/windows-development.md:1).
- **Linux.** Install with `curl -fsSL https://zeron.sh/install.sh | sh`, which starts the daemon, persists it across reboots and writes `zeron.desktop` (Z README.md:14-15).
  - Desktop requirement: WebKitGTK 4.1.
- **iOS.** TestFlight workflow (R v0.2.28). The landing page says "iOS is on the way" and the App Store is "coming soon" (Z apps/landing/public/index.html:1153, 1175).

### F6. Business model

- Free and MIT-licensed (Z README.md:74).
- Sponsors: The Context Company and GitHub Sponsors (Z README.md:64-66).
- The sync backend is a Zeron-hosted Cloudflare Worker, Durable Objects and R2, with WorkOS auth. No price, quota or paid tier appears in the code, the README or https://zeron.sh.
- **Self-hosting** is possible only through `ZERON_EDGE_URL` (Z apps/zeron/src/main.rs:80-82). Nothing documents it as a supported path.
- **Telemetry** covers the landing page only: PostHog pageview and `download_clicked` (Z apps/landing/public/telemetry.js:2-69).
  - It sends `ip=0` and `$process_person_profile:false`, and honours GPC/DNT.
  - No app telemetry was found.

### F7. Security and trust model

- **Local by default.** Online transports are disabled until a saved WorkOS session exists (Z ARCHITECTURE.md:53-58).
- **Every signed-in device is fully trusted.** It can list, read and write the files of every other device's workspaces; `Show ignored files` also exposes `.env`, while `.git` stays excluded (Z README.md:42).
  - "Ignored-file visibility is not an authorization boundary" (Z ARCHITECTURE.md).
- **No E2EE.** The transport is TLS plus WorkOS bearers (Z docs/PARITY.md:101). The per-user registry is private.
- **Agents run unattended:**
  - Claude with `--permission-mode bypassPermissions --dangerously-skip-permissions` when `auto_approve` is set (Z crates/harness/src/claude/mod.rs:209-217).
  - Even without it, the `can_use_tool` handler approves every tool (Z crates/harness/src/claude/mod.rs:967-1008).
  - Codex yolo sets approval to "never" and the sandbox to `danger-full-access` (Z crates/harness/src/codex/mod.rs:1-37).
- **Repo-supplied commands are gated.** `zeron.json` actions must be imported explicitly (docs/reference/project-actions.md:1).
- **Credentials** are written 0600 under the CLI's own lockfile (Z crates/engine/src/agent_accounts/stores.rs:484).
- **Appshot accessibility text** is labelled as untrusted observed data in the prompt (Z crates/ui/src/appshots.rs:1-5).

### F8. Releases, latest first

Cadence:
- 162 releases in 70 days, averaging 2.3 a day.
- Peaks of 10 a day on Aug 11 and Aug 14.
- A 9-day gap from Aug 25 to Sep 3.
- Sep 24–30 averaged 2 a day, ending at 1 a day.
- The versioning is 0.x patch-per-merge with no semver signal. There have been 100 patches on 0.2.

| Ver | Date | Headline |
|---|---|---|
| v0.2.99 | 09-30 | Pi native RPC; reduce motion and pause animations in background; hover tooltips; running subagents first; Linux launcher entry; wallpaper shuffle |
| v0.2.98 | 09-29 | Windows installer is the default download; Cursor loads user MCP servers and plugins |
| v0.2.97 | 09-28 | iOS push (done / needs you / failed); durable desktop updates; harness choice for side chats |
| v0.2.95 | 09-27 | iOS rewrite on a Rust core (UIKit, Liquid Glass); agent-update monitoring; Star banner |
| v0.2.92 | 09-26 | Explorer Subagents/Chats; Zeron MCP injection; projectless explorer |
| v0.2.91 | 09-25 | Copy Path; safe discard; accounts for Grok, Devin, OpenCode, Pi and Hermes; jump to lines |
| v0.2.89 | 09-25 | Plan usage beside the context ring |
| v0.2.83 | 09-22 | Install missing agents; Compact mode; Markdown composer and reference chips |
| v0.2.82 | 09-21 | `zeron mcp`, parent-linked chats |
| v0.2.80–81 | 09-21 | Project Actions (remote run, worktree setup); files as a separate right sidebar; custom sidebar sections, synced |
| v0.2.78–79 | 09-18/19 | Synced pins; project icons; compact sidebar |
| v0.2.73 | 09-17 | Antigravity; Mod+/ model picker; window geometry |
| v0.2.66–67 | 09-16 | Native Windows app; Mod+K palette; landing downloads for every OS |
| v0.2.62–63 | 09-13/14 | Appshots; sounds; image preview; editable comments; thread backgrounds; notification click opens the chat |
| v0.2.60–61 | 09-10/11 | Title-generation settings; Esc-stop opt-in; Markdown preview with Mermaid |
| v0.2.50–55 | 09-09 | Browser tabs; shared queues; "always queue, Send now interrupts"; Linux browser; previews; projectless sessions |
| v0.2.47 | 09-08 | File editor, remote workspaces |
| v0.2.32–44 | 09-04..06 | Performance work: idle CPU 2.11%→0.75%, RSS 202.7→171.2 MiB; Devin |
| v0.2.28 | 08-24 | TestFlight; Ctrl+Tab; archive and jump keys; themes and accents |
| v0.2.20–23 | 08-22 | Side-by-side diff; virtualized model picker |
| v0.2.11 | 08-19 | PR status everywhere; "Queued" sends |
| v0.2.6–10 | 08-18 | Native Claude, Codex and Cursor drivers (ACP reverted); per-subagent transcripts; OpenCode |
| v0.2.4 | 08-17 | Diff-line comments staged into the next prompt |
| v0.2.0 | 08-14 | Local→synced switch with a one-time import wizard |
| v0.1.62 | 08-14 | Local-first with optional sync |
| v0.1.54–55 | 08-13 | macOS signing and notarization; git history graph |
| v0.1.48 | 08-12 | Cursor ACP; desktop notification banners |
| v0.1.36 | 08-10 | Activity sidebar, "the end of tabs" |
| v0.1.22–28 | 08-08/09 | Grok, Hermes, Pi; Claude and Codex moved to ACP |
| v0.1.10–20 | 08-03..05 | Light mode; file mentions; Windows caption controls; row registry |
| v0.1.1 | 07-29 | Detachable ratatui TUI (`comet tui`), since removed (not in Z apps/zeron/src/main.rs:31-62) |
| v0.1.0 | 07-22 | First release |

Product arc:
1. Web-style tabs.
2. A sidebar of sessions (v0.1.36).
3. Local-first (v0.1.62).
4. Native drivers instead of ACP (v0.2.6).
5. A workspace around the chat: files, browser, previews, actions (v0.2.47–0.2.80).
6. Multi-agent orchestration: MCP and subagents (v0.2.82–0.2.92).
7. Mobile and push (v0.2.95–0.2.97).

### F9. Where Pocket already overlaps

- **Status vocabulary and seen semantics** match. Pocket uses Needs you > Done > Working > Idle with one seen flag, and a failed turn is Done-marked-failed (P CONTEXT.md:42-61).
  - The Pocket desktop already renders that as a separate "Failed" status ranked between Needs you and Done (P packages/desktop/crates/pocket/src/status.rs:6-12, 21, 36), so Zeron's own addition is only the 45 s staleness gate.
- **Pocket already has:** Cmd+K palette, Cmd+P go to file, Cmd+J next waiting, Cmd+Shift+N new worktree (P packages/desktop/crates/pocket/src/main.rs:1047-1056).
- **Pocket's attention inbox** (P packages/desktop/crates/pocket/src/inbox.rs:29) has no Zeron equivalent. Zeron sorts only by recency.
- **Pocket already has desktop OS notifications.** `sync_alerts` posts a system notification (title = session title or provider name, body = status label) when an agent turns Needs you, Failed or Done while not viewed. It dismisses it once viewed, skips agents first seen on connect, and a click focuses the agent (P packages/desktop/crates/pocket/src/main.rs:300-327, 1103-1106; P packages/desktop/crates/pocket/src/status.rs:119-124). It has no chime, no background-only toggle and no phone push.
- **Pocket already has a per-repo worktree setup command** (`RepoConfig.setup`, placeholder "pnpm install"), run before the agent in each new worktree (P packages/desktop/crates/store/src/store.rs:13; P packages/desktop/crates/pocket/src/forms.rs:427-448).
- **Pocket lacks, per rg over its code:**
  - PR status (`rg "gh pr|pull request"` finds nothing)
  - Mod+1–9, Ctrl+Tab, archive, pins
  - named project actions beyond the setup command

### F10. Docs vs code disagreements (code wins)

- **Mobile.** PARITY says "Mobile app — out of scope" (Z docs/PARITY.md:100). Code ships `apps/ios` and `crates/mobile`.
- **Theme.** PARITY says "always-dark theme" (Z docs/PARITY.md:12). Code has a ThemeFamily/Variant system, light mode (R v0.1.10) and VS Code import.
- **Token usage.** PARITY says the "Token-usage display dropped" (Z docs/PARITY.md:96). Code has the context ring (Z crates/ui/src/context_usage.rs:11-12) and plan usage (R v0.2.89).
- **Import on sign-in.** README says "Signing in does not upload, move, or import existing local sessions" (Z README.md:44). The desktop wizard offers an opt-in "Bring my work" import (Z crates/ui/src/shell.rs:8842-8850, Z crates/engine/src/local_import.rs:1).
  - The README is true only for the CLI path and the default.
- **Steer.** PARITY lists a composer "Send/Steer/Stop morph" (Z docs/PARITY.md:18). Since v0.2.53 there is no user Steer, only queue plus "Send now (interrupt)" (Z crates/ui/src/queue.rs:102, R v0.2.53).
- **Permissions.** The Claude harness honours `--permission-mode default` when `auto_approve` is off, but `can_use_tool` still auto-allows every tool (Z crates/harness/src/claude/mod.rs:216, 967). A permission prompt never reaches the user.

## Ideas to clone into Pocket

Pocket is PTY-first (P CONTEXT.md:32). Any feature that needs structured turns applies only to attached agents.

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prereqs |
|---|---|---|---|---|---|---|
| 01-1 | Extend desktop notifications: chime, Zeron's copy ("Run finished", "Waiting on your input", "Run failed") and a background-only toggle. Pocket already posts banners with a silent first-sight baseline and click-to-focus | Leave the window and still get pulled back only when it matters | Z crates/ui/src/shell.rs:2585-2760; Z crates/ui/src/notify.rs:1-80 | `sync_alerts` in `packages/desktop/crates/pocket/src/main.rs:313`, `status::alerts` in `status.rs:120`: **adapt** | S | — |
| 01-2 | iPhone push on Needs you, Done and Failed. `thread-id = session id` so a newer push replaces the older one; per-device category toggles | The phone buzzes when an agent needs you, even with the app closed | Z edge/src/push-notify.ts:1-100; Z apps/ios/Zeron/App/PushNotifications.swift | pocketd emits the transition; a push relay (APNs/Expo push) is needed; `packages/app` registers the token: **new** | L | Hosted relay or Expo push service; APNs key |
| 01-3 | Staleness gate: a Working or Needs you status older than 45 s with no update renders as none | No phantom "Working" after a daemon or network drop | Z crates/proto/src/view.rs:41-64 | `packages/protocol` status plus the app and desktop renderers: **port** | S | Heartbeat timestamp on status |
| 01-4 | Mod+1–9 jump to the nth visible session; Ctrl+Tab and Ctrl+Shift+Tab cycle; Mod+Shift+A archive | Keyboard-only triage across 10+ sessions | Z crates/ui/src/settings.rs:148, 1116-1127 | `packages/desktop/crates/pocket/src/main.rs:1047` keymap; `keys` crate: **port** | S | Archive state in pocketd |
| 01-5 | GitHub PR badge per worktree, read through the host's `gh`. Cache TTL 2 min with a PR, 45 s without; failure backoff 20 s; colour by state | See which session's branch has an open, merged or closed PR | Z crates/engine/src/source_control.rs:183; Z crates/engine/src/change_requests.rs:23-25 | pocketd new `gh` probe; protocol field; desktop `sessions.rs` and app row badge: **new** | M | `gh` installed and authenticated on the host |
| 01-6 | Diff-line review comments, staged and then pasted into the agent's next prompt as plain text | Code review without copy-paste | Z crates/ui/src/comments.rs:1-5; R v0.2.4 | desktop `diff.rs`, app `DiffView.tsx`; pocketd writes the text into the PTY: **adapt** | M | Diff view already exists |
| 01-7 | Project actions: named shell commands per project run from the title bar. Device-private JSON; repo-provided config needs explicit import. (Pocket already has the worktree setup command.) | One-click dev server or test run | Z crates/engine/src/project_actions.rs:32; Z docs/reference/project-actions.md:1 | next to `RepoConfig.setup` in `packages/desktop/crates/store/src/store.rs:13`; spawn like `daemon::setup_op` (`forms.rs:448`); phone can trigger: **new** | M | — |
| 01-8 | Pins plus custom sidebar sections, shared by desktop and phone | Keep "Focus" and "Later" groups | Z crates/ui/src/shell/sidebar_sections.rs:1; sidebar_pins.rs | pocketd state and protocol; desktop sidebar; app list: **new** | M | Stable session IDs |
| 01-9 | Row anatomy: `project @ device`, title, agent mark, branch, PR badge, status word (Working, Needs you, Done, Failed) or relative time; compact toggle | Scan status and context in one line | Z crates/ui/src/shell.rs:6940-6990; Z apps/ios/Zeron/Threads/SessionCell.swift:94-102 | desktop `sessions.rs`; app `AgentsScreen.tsx`: **adapt** | S | 01-5 for the badge |
| 01-10 | `pocketd service install|uninstall|status` (launchd/systemd), plus self-update that restarts only when no agent turn and no terminal is active | Daemon survives reboot; updates never kill a running agent | Z apps/zeron/src/main.rs:64-78; Z crates/update/src/lib.rs:984-1016; Z README.md:62 | `packages/pocketd/cmd/pocketd`: **new** | M | Release artifacts plus a signed manifest |
| 01-11 | Desktop auto-update: check on start, hourly and on wake; background download with SHA-256 verify; "Update ready — restart to apply"; install on quit; env opt-out | Users stay current with no effort | Z crates/ui/src/app_update.rs:12-14, 404; Z README.md:60 | `packages/desktop`: **new** | L | Signed and notarized builds; release host |
| 01-12 | Queued follow-ups with "Send now (interrupt)". A message typed during a turn is delivered when the agent reaches Idle | Type the next instruction without waiting or clobbering the turn | Z crates/ui/src/queue.rs:94-116; R v0.2.53 | pocketd per-session queue; delivered on Working→Idle; Send now = interrupt, then type: **adapt** | L | Reliable Idle detection (attached agents only) |
| 01-13 | QuestionPanel: `AskUserQuestion` options as rows; keys 1–9 pick, then auto-advance after 220 ms | Answer multi-question prompts in seconds, even on the phone | Z crates/ui/src/composer.rs:104 | app `PermissionSheet.tsx`, desktop overlay; pocketd `internal/claude`: **adapt** | M | Structured question event from attached Claude |
| 01-14 | Stdio MCP server that lets an agent list, create, read, message, wait on, interrupt and archive sessions | Agents can orchestrate other agents inside Pocket | Z crates/mcp/src/tools.rs:58-183; Z docs/mcp.md:1 | `pocketd mcp` subcommand over the existing WebSocket: **new** | M | Session create API in the protocol |
| 01-15 | Agent CLI health: detect missing or outdated claude and codex, show Install or Update, policy Notify / AutoWhenIdle / Off | Fewer broken sessions from stale CLIs | Z crates/engine/src/harness_updates.rs:1-3; Z crates/proto/src/agent.rs:33-42 | pocketd `internal/agent`; desktop settings: **new** | M | Install-source detection (npm or brew) |
| 01-16 | Context ring under the input (≥75% warning, ≥90% danger), plus plan usage and reset time | Know when to compact or hand off before hitting limits | Z crates/ui/src/context_usage.rs:11-12; Z crates/engine/src/agent_accounts/usage.rs:1-15 | pocketd `internal/claude`, `internal/codex` usage; desktop and app: **adapt** | M | Token counts from attached agents |
| 01-17 | Auto session title from the first prompt via a cheap model; fall back to the first words; a user rename always wins | Readable session list without manual naming | Z crates/engine/src/titles.rs:1-3 | pocketd `internal/timeline`: **port** | S | First-prompt capture |
| 01-18 | Rebindable shortcuts; a bound combo is refused and the dialog names the owner | Power-user fit without conflicts | Z crates/ui/src/settings/shortcuts.rs:261, 463 | `packages/desktop/crates/keys`: **port** | M | Keymap persisted in config |
| 01-19 | Project icon discovery from repo artwork, fallback monogram on 8 hashed colours (8% background, 85% letter), 64 px | Faster visual scanning of the project list | Z crates/ui/src/shell/project_icon.rs | desktop `workspace` and `theme` crates; app: **port** | S | — |
| 01-20 | Message rail: prompt ticks at the transcript's left edge, hover preview, click to scroll; hidden below 48rem | Navigate long timelines | Z crates/ui/src/rail.rs:1-4 | no desktop timeline view exists (timeline data only in `packages/desktop/crates/agents/src/agents.rs:118`); app `TimelineView.tsx`: **new** on desktop | M | Structured timeline (attached agents) |
| 01-21 | Git history graph pane (paged commits, lane graph) | Understand what agents committed | Z crates/ui/src/history.rs:1 | desktop `git` crate: **new** | L | — |
| 01-22 | Appshot: a global hotkey captures the frontmost window and attaches it to the prompt | Show the agent the bug on screen | Z crates/ui/src/appshots.rs:1-5 | desktop: **new** | L | Screen Recording permission; PTY image paste path |
| 01-23 | Dev-server previews at a stable `device.project.localhost` URL, reachable from other devices | Open the agent's app on the phone | Z crates/preview/src/lib.rs:1-3 | pocketd proxy: **new** | XL | Phone↔host transport beyond WebSocket |

## UI/UX spec to copy

**Status**

| State | Zeron label | Glyph | Tone |
|---|---|---|---|
| Working | "Working" | Spinner (mini glyph in compact mode) | Accent (pink/magenta on iOS) |
| AwaitingInput | "Input" | Dot | Warning |
| Errored | "Failed" | Dot | Danger |
| Completed (unseen) | "Done" | Check | Success (green) |
| Idle | Relative time ("6h") | None | Muted |
| Send not yet delivered | "Queued" | — | Warning |
| Send failed | "Failed" | — | Danger |

Sources: Z crates/ui/src/shell.rs:6940-6990; Z apps/ios/Zeron/Threads/SessionCell.swift:94-102.

**Status rules**
- Stale after 45 s.
- Sort by recency, never by status.
- Side chats never notify.

**Notification copy**
- Title: the chat title, or "New session".
- Bodies:
  - "Run finished"
  - "Waiting on your input"
  - "Run failed"
- Connectivity: "Connection unavailable" with "Your device is offline" or "Zeron is trying to reconnect".
- Agent updates: "{n} agent update(s) available" or "A coding agent update is ready".

**Sidebar row**
- Line 1: `project @ device`, relative time.
- Line 2: title.
- Line 3: agent mark, branch icon, branch.
- Right side: PR badge (#N) coloured by state (green open, purple merged, red closed), and a globe icon for remote.
- Groups: Pinned, custom sections, Sessions, Archived (collapsible). An empty custom section shows "Drop sessions here".

**Palette**
- Placeholder: "Type a command or search chats…"
- Groups: Actions first, then history.
- Rows 32 px, 16 px backdrop blur.
- Footer: "↑↓ Navigate · ↵ Open · esc Close".

**Default shortcuts** (Z crates/ui/src/settings.rs:1087-1127, Z crates/ui/src/shell.rs:488)

| Action | Key |
|---|---|
| Palette | Mod+K |
| New session | Mod+N |
| New project | Mod+Shift+N |
| Sidebar | Mod+B |
| Changes | Mod+R |
| Files | Mod+E |
| Terminal | Mod+J |
| Model picker | Mod+/ |
| Next / previous session | Ctrl+Tab / Ctrl+Shift+Tab |
| Jump | Mod+1..9 |
| Archive | Mod+Shift+A |
| Save | Mod+S |
| Browser reload | Mod+Shift+R |
| Appshot | Ctrl+Alt+Space (mac) |
| Wallpaper | Mod+U |

**Pane sizes** (Z crates/ui/src/settings.rs:33-56; Z docs/research/feature-inventory.md:20, 35)
- Sidebar: 224 min, 256 default, 400 max.
- Files panel: 220 min, 286 default, 440 max.
- Right pane: 360 min, 520 default; the Changes pane is capped at 52%.
- Chat panel: 300 min.
- Terminal: 160 min, 280 default, 55vh max.
- Window: 1320×880, minimum 900×600, traffic lights at {14,15}.

**Composer**
- Auto-grows from 76 to 260 px.
- Placeholder: "Do anything…"
- Chips: model and effort.
- Round send button.
- Below the input: checkout ("Local checkout") and branch.

**Motion** (Z docs/research/feature-inventory.md:58, 107, 115)

| Element | Duration and easing |
|---|---|
| Entrance fade | 0.5 s `cubic-bezier(0.16,1,0.3,1)`, translateY 4→0 |
| Menu in | 0.14 s, scale 0.96→1 |
| Dialog in | 0.18 s |
| List resort | 260 ms `cubic-bezier(0.22,1,0.36,1)` |
| File collapse | 180 ms |
| Chevron | 200 ms |
| Panes | 200 ms ease-out |

- `reduce_motion` and `pause_animations_in_background` are settings.

**Terminal**
- Background #090909.
- Input coalescing 12 ms, resize debounce 80 ms.
- "[process exited N]" on exit.
- Limits: 1 MiB replay, 32 terminals, exited terminals swept after 30 min.

**Settings saves** are debounced 400 ms.

**Update affordance**
- One sidebar row: "Update ready — restart to apply".
- The update installs on quit if ignored.

## Open questions / risks

- **Push needs a hosted relay.** Zeron computes push at its edge (Z edge/src/push-notify.ts). Pocket has no server. Does Pocket accept a hosted push relay (APNs key custody), or should it use Expo push as the relay?
- **PTY limits what Pocket can copy.** Zeron's queue, steer, QuestionPanel, context ring and rail all rely on structured protocols. In Pocket they work only for attached agents (P CONTEXT.md:32); unattached terminals get none of them.
- **Is Zeron's trust posture a differentiator or a gap?** Auto-approve everything plus full remote file access is Zeron's default (Z crates/harness/src/claude/mod.rs:967; Z README.md:42). Pocket's "Needs you" includes permissions. Keep that as an explicit safety difference in positioning.
- **Speed of competition.** At about 2 releases a day, Zeron's surface changes weekly. This inventory is a snapshot at ed3b1aa.
- **Auto-update prerequisites.** 01-10 and 01-11 need signing, notarization and a release host that Pocket may not have.
- **Monetisation is unclear.** Zeron runs the sync edge for free with no paid tier, so it is unclear how the edge is funded long-term. Pocket needs no cloud today, which is a cost advantage.
- **Unverified here:**
  - iOS push reliability, and whether push works when signed out (the code says it requires synced sign-in).
  - Windows notification banners (the code says there are none).
  - Actual App Store status.

## Verification

Date: 2026-09-30. Claims checked: 22. Corrected: 7.

Confirmed against source: harness list and IDs, Claude CLI flags, `can_use_tool` auto-allow, Codex version pin and yolo sandbox, `SESSION_STALE_MS`, recency-only sort, `attention_rank`, default shortcuts and `JUMP_SLOTS`, pane sizes and autosave bounds, 220 ms QuestionPanel advance, context ring thresholds, PR cache TTLs, terminal limits, worktree path, notification copy and `thread-id`, MCP tool list, CLI subcommands, README update/trust text, Pocket keybindings at main.rs:1047-1056. Release-note (R) citations and screenshot-derived details were not re-checked.

- F9 and the TL;DR said Pocket lacks OS notifications. Wrong: `sync_alerts` already posts, dismisses and click-routes system notifications (P main.rs:300-327, 1103-1106; status.rs:119-124). Idea 01-1 changed from **new**/M to **adapt**/S, scoped to chime, copy and background-only.
- F9 said Pocket lacks project actions without noting that its per-repo worktree setup command already exists (P store.rs:13; forms.rs:427-448). Added that, and removed the setup action from 01-7's scope.
- F9 said Zeron differs by splitting Failed out. The Pocket desktop already shows a separate Failed status (P status.rs:6-12, 36).
- 01-20 mapped to a "desktop timeline view" that does not exist in Pocket. Remapped, with effort raised to M.
- TL;DR said Claude always runs with `bypassPermissions`. That flag applies only with `auto_approve`; the handler auto-allows either way (Z claude/mod.rs:209-217, 967-1008).
- Pitch citation fixed: index.html:984 → 986 (984 is the section heading).
- IPC port 27654 is not in ARCHITECTURE.md. Cited Z apps/zeron/src/main.rs:288 instead.
