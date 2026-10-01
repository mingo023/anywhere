# 15. Landscape: other agent orchestrators and their ideas

Date: 2026-09-30.

Sources (everything checked live on 2026-09-30):
- GitHub clones and API: herdrdev/herdr @ `331775c3` (v0.9.3, 2026-09-29); manaflow-ai/cmux @ `02dac3c1`; slopus/happy @ `4cf54d18`; superset-sh/superset @ `7886c5ad`; nimbalyst/nimbalyst @ `9979578c` (v0.79.0, 2026-09-29); smtg-ai/claude-squad @ `ce1ffb43` (v1.0.20, 2026-08-20).
- Release and README checks with `gh api`: generalaction/emdash (v1.2.7, 2026-09-27), imbue-ai/sculptor (sculptor-v0.48.0, 2026-09-21), amantus-ai/vibetunnel (v1.0.0-beta.18, 2026-07-11), BloopAI/vibe-kanban (v0.1.44, 2026-04-24), stravu/crystal, coder/mux, omnara-ai/omnara, terragon-labs.
- Official docs: herdr.dev/docs (`llms-full.txt`), docs.superset.sh, code.claude.com/docs, learn.chatgpt.com/docs (developers.openai.com redirects here with a 308), docs.conductor.build and conductor.build, cursor.com/docs, docs.warp.dev and warp.dev/pricing, cmux.com/docs.
- Pocket worktree `/Users/mingo/.worktrees/anywhere/orchestrator-research` @ `86deb13`.
- Earlier reports 01-14 in this directory, used to avoid repeating ideas.

Citation legend:
- `P path:L` is a Pocket worktree path and line.
- `repo@sha path:L` is a line in a cloned competitor repo.
- A URL is an official site or docs page. `§Section` names the heading on that page.
- "rNN-x" points to idea x in sibling report NN.
- "code≠doc" marks a place where code and docs disagree. The code wins.

## TL;DR

- **The closest rivals already ship Pocket's pitch, a Mac app plus a phone app driving local CLI agents:**
  - Superset: iPhone access needs Pro at $20/user/mo.
  - Happy: MIT, E2EE, free.
  - cmux: GPL, iOS app on TestFlight.
  - Nimbalyst: MIT, iOS and Android.
- **Both vendors now ship first-party phone control of the local CLI:**
  - Anthropic: `claude remote-control`.
  - OpenAI: ChatGPT desktop "Control this Mac".
  - Pocket's edge has to be multi-provider, terminal-first, and self-hosted.
- **Session restore after a daemon restart is table stakes, and Pocket has none.**
  - herdr and cmux bring back layout, cwd, and the agent conversation via resume argv. Superset keeps terminals in a daemon and restores killed agents with "Resume Args".
  - pocketd persists only `config.json` and its plugin (P packages/pocketd/internal/config/config.go:60; P packages/pocketd/internal/daemon/plugin.go:48).
- **herdr has the cleanest open protocol for agent-reported state:**
  - `pane report-agent --state idle|working|blocked --seq N -- <resume argv>`.
  - A monotonic seq drops stale reports, and a shell-prompt fallback releases the pane.
  - It is a cheap way to add providers beyond claude and codex.
- **Attention UX converged on four pieces:**
  - An unread model with a "jump to latest unread" hotkey (cmux ⌘⇧U). Codex's ⌥⌘U only opens and closes its Activity view.
  - Agent-decided push, with push suppressed while the user is at the terminal (Claude).
  - Lock-screen Live Activities for up to 8 agents (Cursor).
  - A floating HUD over other apps (Codex pet).
- **The worktree lifecycle has hardened:**
  - Retention: Codex keeps the last 15 worktrees and snapshots before deleting.
  - `.worktreeinclude` for gitignored files (Claude and Codex).
  - A 10-port block per workspace (Conductor).
  - Auto-archive when the PR merges (Claude desktop).
- **Remote transport is split two ways.** Vendors and Superset run relays. Happy and Nimbalyst end-to-end encrypt with a key passed in a QR code. VibeTunnel uses Tailscale Serve TLS.
  - Pocket prints a raw `ws://<tailscale-ip>` and listens on every interface (P packages/pocketd/cmd/pocketd/serve.go:46,58).
- **Dead or pivoted:**
  - Crystal became Nimbalyst.
  - Vibe Kanban is sunsetting.
  - Terragon is gone.
  - Omnara pivoted to managed agents.
  - coder/mux was renamed coder/xum.
  - Cloud-only (out of scope): Jules, Devin, Factory, Cursor cloud agents.
- **15 ideas below. Must:** restore via resume argv (15-1).
  - **Should:** report API (15-2), OSC notifications (15-3), agent-requested push (15-4), keep-awake (15-7), worktree retention (15-8), `.worktreeinclude` (15-9), port block (15-10), E2EE (15-12), Tailscale TLS plus loopback bind (15-13), jump-to-unread (15-15).

## Findings

Ordered by closeness to Pocket: a Mac app plus a phone app driving local CLI agents.

### F1. Superset (closest commercial rival)

- **Positioning.** Electron desktop, macOS first. "The desktop app is free forever." (superset.sh/pricing)
  - Supports 20+ CLI agents. It "never touches model traffic" (docs.superset.sh §The Superset Model).
  - Source is open under the Elastic License 2.0 (superset@7886c5ad LICENSE). A desktop-canary build shipped on 2026-09-30.
- **Pricing.**
  - Pro: $20/user/mo, or $15 billed yearly ($180/yr) (superset.sh/pricing).
  - Pro includes the iPhone app, Automations and more.
- **Phone.**
  - The iPhone app (App Store id6788926383) needs Pro, iOS 26+, and "Allow remote access to this device via relay".
  - The desktop QR "opens the App Store; it does not sign you in". You sign in on the phone with the same account and organization.
  - "Keep the computer awake, online, and running Superset." (docs.superset.sh/remote-access §Connect from an iPhone)
  - Turning on the relay restarts the host service. It asks the user to type a confirmation phrase, and the docs warn to use a separate machine (same page §Option 1).
  - Traffic "passes through the Superset relay". The docs don't mention E2EE (same page §Ports on a remote host).
  - Wake a sleeping host: `superset hosts set-wake <host> <command...>`, then `superset hosts wake` (docs.superset.sh CLI reference).
- **Status.** Comes from "lifecycle hooks and command wrappers" in `~/.superset/bin` plus `superset-hooks`. It works only for agents launched through Superset (docs.superset.sh/agent-status).
  - "Claude Code reports both finished and waiting states". Other agents may send completion only (same page §Notifications).
  - Other surfaces:
    - Dock badge, which needs notification permission.
    - Custom ringtones: MP3/WAV/OGG ≤20 MB (docs.superset.sh §Notification Sounds).
    - A **Clear Status** action.
  - Agent chips are hidden unless more than one agent runs.
- **Persistence.** "Terminals are backed by a background daemon, so running processes and scrollback survive app restarts and updates." (docs.superset.sh §FAQ)
  - Each agent config has **Resume Args**: "Arguments used to restore a killed session by its id (e.g. `--resume`)" (docs.superset.sh §AI Agents).
- **Usage page** (docs.superset.sh/usage):
  - Quota meters "Session (5h)" and "Weekly", refreshed every 5 min.
  - Multiple accounts via `CLAUDE_CONFIG_DIR` / `CODEX_HOME`.
  - "Cost to you: $0".
  - Machine resources every 2 s, with a process table per pane (⌘⇧U).
- **Workspaces and ports.**
  - Lifecycle scripts in `.superset/config.json` `{setup, teardown, run}`.
  - Remote ports forward to the same local number. On a clash the row shows "**local port busy**" with a "**Use another port**" action, e.g. `3000 → localhost:54321` (docs.superset.sh/remote-access).
- **PRs.** The Pull requests split view shows a CI summary per row ("All 12 passed"; cancelled counts as failed).
  - You can select diff lines and send them "to a running agent or a new session" (docs.superset.sh §Pull Requests).
- **Other.**
  - ⌘I rich prompt editor.
  - CLI, SDK and MCP server; `superset:*` skills.
  - Automations (Pro); Design mode; Pages; Slack and Linear.
  - Recipes: "Race Agents on One Task", "Command Your Fleet from the CLI".

### F2. Happy (open-source phone client for Claude Code and Codex)

- **Positioning.** "Use Claude Code or Codex from anywhere with end-to-end encryption" (happy@4cf54d18 README.md).
  - MIT, about 24k stars.
  - Install with `npm install -g happy`, then run `happy claude` / `happy codex` instead of `claude` / `codex`.
  - Clients: iOS, Android, web, and the macOS CLI wrapper.
- **Local/remote mode switch in one process:**
  - The CLI shows "Remote Mode - Claude Messages" (happy@4cf54d18 packages/happy-cli/src/ui/ink/RemoteModeDisplay.tsx:173).
  - "Press space (or Ctrl-T) to switch to local mode • Ctrl-C to exit" (same file :228). Space needs a second press, "Press space again" (:223), within 15 s (:91-99). Ctrl-T switches at once (:32-36).
  - Launchers: `claudeRemoteLauncher.ts:31,56-57,102,511-522`, `claudeLocalLauncher.ts:84-94`.
- **Push.** Expo push through Happy's server `api.cluster-fluster.com` (packages/happy-cli/src/api/pushNotifications.ts:74-82).
- **Voice agent (ElevenLabs).** It has client tools:
  - `sendMessageToSession {sessionId, message}`, which returns "sent [DO NOT say anything else, simply say 'sent']".
  - `processPermissionRequest {requestId, decision: allow|deny}` (packages/happy-app/sources/realtime/realtimeClientTools.ts:20-52).
  - Limits (docs/paid-voice.md:39-46):
    - Free: 20 min per 30 days (about $0.19).
    - Subscribed: 5 h per 30 days.
    - Bring your own ElevenLabs key: unlimited.
    - Every tier: 100 conversations per 30 days.
- **Pricing.** The app and CLI are free. Paid voice is optional (docs/paid-voice.md).

### F3. cmux (Ghostty-based Mac terminal for agents)

- **Positioning.** "A Ghostty-based macOS terminal with vertical tabs and notifications for AI coding agents" (cmux@02dac3c1 README.md:2).
  - GPL-3.0-or-later for the app; the server, workers and relay are under BSL 1.1 (:525). "free, open source, and always will be" (:487).
- **Attention.**
  - "Panes get a blue ring and tabs light up when coding agents need your attention" (:38).
  - The notification panel lets you "jump to the most recent unread" (:47).
  - Rings, badges, the popover and macOS notifications "fire automatically via standard terminal escape sequences (OSC 9/99/777)", or through the cmux CLI or hooks (:396).
- **Unread shortcuts** (:217-219):
  - ⌘⇧U "Jump to latest unread".
  - ⌥⌘U "Toggle current item unread state".
  - ⌃⌘U "Mark current item as oldest unread and jump to next latest unread".
- **Restore.**
  - Brings back layout, cwd, best-effort scrollback, and browser history.
  - Agents resume through hooks: `cmux hooks setup [codex | --agent opencode]` (:280-285).
  - Custom resume: `cmux surface resume set --kind tmux --checkpoint work --shell "tmux attach -t work"` (:296).
  - Opt-out: `"autoResumeAgentSessions": false` (:317).
- **iOS.** A TestFlight beta, "cmux BETA", pairs from the Mobile Connect window. It can forward terminal notifications. Early access comes with Founder's Edition (:380).
  - Pairing code (`CmxUserTailscalePairingAuthorization.swift:11-45`):
    - A user-typed code authorizes only the exact numeric Tailscale peer and port, and is never persisted.
    - The host records a device-bound grant only after it authenticates.
- **Remote.** `cmux ssh user@remote`. Browser panes route through the remote host, and images upload via scp (:74).
- **Pricing.** Free. Founder's Edition (a Stripe link) adds early access to cmux AI, iOS, Cloud VMs, and Voice mode (:485-495).

### F4. herdr (terminal multiplexer for agents)

- **Positioning.**
  - A Rust TUI multiplexer with workspaces, tabs and panes, a server/client split, and `herdr --remote workbox` over SSH (herdr.dev/docs §Persistence and remote access).
  - Apache-2.0, about 41.5k stars, v0.9.3 on 2026-09-29.
  - Third-party phone clients exist: missuo/herdrm, dcolinmorgan/herdr-remote.
- **Detection.** Screen-manifest rules read "the live bottom of the pane". "When an integration also reports state, Herdr uses those reports instead of reading the screen." (herdr.dev/docs/agents)
  - Blocked detection is strict. Codex falls back to `unknown`; other known agents fall back to `idle` (same page).
  - Pocket's own note says hooks "were dropped as a state source in 0.6.7 because stale reports left panes stuck" (P docs/wayfinder/agent-sessions/01-herdr-agent-state.md:26).
  - Reading the seq rule below as herdr's fix for that problem is an inference; herdr's docs don't say so.
- **Report API** (herdr.dev/docs/add-herdr-support/):
  - `"$HERDR_BIN_PATH" pane report-agent "$HERDR_PANE_ID" --source my-agent --agent my-agent --state working --seq 1`.
  - States: `idle|working|blocked`, plus `--message` to explain a block.
  - `--seq` "must increase with every report from your source, including across sessions and restarts … A timestamp works well. Herdr ignores reports whose number is not higher than the last one it accepted".
  - Report only when `HERDR_ENV=1`.
  - The resume command goes after `--`.
    - Rules: first word is a plain command on `PATH`; no apostrophes or control characters; ≤64 args and ≤8 KiB.
    - Errors: `invalid_resume_argv`, `resume_not_accepted`.
    - Opt-out: `[session] resume_agents_on_restore = false`.
  - `pane release-agent` on exit. The fallback clears the agent "once the pane is back at its idle shell prompt … a second or two".
- **Restore** (herdr.dev/docs/session-state/):
  - The "What survives" table covers detach, server restart, and update with and without `--handoff`.
  - Up to 48 layout snapshots, at most one per 15 min. 3 recovery copies of a `session.json` that failed to load.
  - Pane screen history is off by default because it "can include secrets" (`[experimental] pane_history = true`).
  - Native resume commands include `claude --resume <id>`, `codex resume <id>`, `opencode --session <id>`, and `cursor-agent --resume <id>`.
- **Live handoff.** `herdr update --handoff` (experimental) passes PTY fds to the new server over SCM_RIGHTS (herdr@331775c3 src/server/handoff.rs:412):
  - `HANDOFF_VERSION=1`, `READY_TIMEOUT=30s`, `OWNED_ACK_TIMEOUT=500ms` (:20-24).
  - `FDS_PER_MESSAGE=64`, because of the SCM_RIGHTS caps of 253 on Linux and 254 on macOS (:25-29).
  - `MAX_REPLAY_BYTES_PER_PANE=8 KiB` (:31).
- **CLI** (herdr.dev/docs/cli-reference/):
  - `herdr notification show <title> [--body] [--sound none|done|request]`.
  - `herdr agent wait <x> --until blocked --timeout 120000`.
  - `ctrl+b q` detaches.
  - "`done` is idle but not yet marked seen".
  - New panes strip inherited Claude Code, Codex and OMP session markers (herdr.dev/docs §CLI reference, environment).
- **Pricing.** Free.

### F5. Anthropic: Claude Code Remote Control and Claude desktop

- **Remote Control** (code.claude.com/docs/en/remote-control):
  - Plans: Pro, Max, Team and Enterprise; no API-key auth.
  - Invocations:
    - `claude remote-control` (server mode). Space shows a QR; `--spawn same-dir|worktree`; `--capacity` defaults to 32.
    - `claude --remote-control` / `--rc`, or `/remote-control` / `/rc` inside a session.
  - Sessions get names like `myhost-graceful-unicorn`, and the footer shows `/rc active`.
  - A second client triggers "Another connection took over this session". `/mobile` shows an app-store QR.
  - Outbound HTTPS only; polls the Anthropic API. Server mode gives up after about 10 min of network outage.
  - Push:
    - "Claude decides when to push", e.g. "notify me when the tests finish".
    - `/config` toggles "Push when Claude decides" and "Push when actions required". Without a phone it shows "No mobile registered".
    - It skips push while you are typing in or focused on the terminal.
    - The `CLAUDE_CLIENT_PRESENCE_FILE` marker suppresses push.
- **Claude desktop Code tab** (code.claude.com/docs/en/desktop):
  - Worktrees under `<project-root>/.claude/worktrees/`, with `.worktreeinclude` for gitignored files.
  - Cmd+N and Ctrl+Tab.
  - CI status bar with Auto-fix, Auto-merge (squash), and a notification when CI finishes.
  - **Auto-archive when the PR merges or closes.**
  - Cross-session messages show as a card labeled with the sender and wait until the current turn ends.
  - Task chips spawn a worktree session.
  - Dispatch sessions get a **Dispatch** badge and push on finish or approval (Pro and Max).
  - `.claude/launch.json` preview config.
- **Pricing.** Included in Claude plans.

### F6. OpenAI: ChatGPT/Codex desktop remote, notifications, pets, worktrees

- **Remote** (learn.chatgpt.com/docs/remote):
  - Settings > Connections > "Control this Mac or PC", scan the QR, same account. "Keep your computer awake and online."
  - Approvals: "Approve / Always approve / Deny". Diff summary like "2 files changed +38 -12".
- **Notifications** (learn.chatgpt.com/docs/notifications):
  - Turn-completion alerts: never / background-only / always, with separate permission and question toggles.
  - Activity view: bell, ⌥⌘U, filters Work/Chat/Pinned/Scheduled, "Mark all as read".
- **Pets** (learn.chatgpt.com/docs/pets):
  - A floating pet sits over other apps. Mini mode has no pet. Option+Space shows the floating controls and focuses Quick Chat; pressing it again keeps them open, so it is not a toggle. `/pet` shows or hides the pet.
  - States: Running / Needs input / Ready / Blocked, with priority needs input > blocked > ready > running.
  - Reduced motion shows a still frame.
  - Custom sprite: 1536×1872 PNG/WebP, ≤20 MiB.
  - The CLI `/pets` needs iTerm2 3.6+, Kitty or Sixel, and doesn't work in tmux.
- **Worktrees** (learn.chatgpt.com/docs/environments/git-worktrees):
  - Stored in `$CODEX_HOME/worktrees` on a detached HEAD.
  - `.worktreeinclude` (e.g. `.env`, `.env.local`, `config/secrets.json`) skips symlinks and never overwrites.
  - Handoff Local⇄Worktree.
  - **Keeps the most recent 15 managed worktrees.** Protected: pinned chats, in-progress chats, permanent worktrees.
  - A worktree is deleted on archive or when over the limit. A snapshot is saved first, and restore is offered on reopen.
- **Pricing.** Included in ChatGPT plans.

### F7. Conductor (Mac app, now local plus cloud)

- **Positioning.** "Run a team of coding agents in the cloud". Multiplayer workspace links via ⌘⇧C (conductor.build).
- **Pricing** (conductor.build/pricing):
  - Free: $0, local workspaces, bring your own subscriptions.
  - Pro: $50/mo, adds cloud hours, multiplayer up to 5, the Conductor API, and the mobile app.
  - Teams: $60/user/mo. Enterprise: custom.
- **Changelog** (conductor.build/changelog):
  - 0.89.1 (2026-09-29): GPT-6.1 Sol.
  - 0.89.0: Sign in with ChatGPT, PR card, descriptive branch names.
  - 0.86.0 (2026-09-16): shared loadouts, ⌘Y port forwarding.
  - 0.85.0 (2026-09-09): sidebar sections; model-picker loadouts ⌃⌘1-5, effort ⌘⇧/, speed ⌘⇧E; cloud routines; private workspaces; "Workspace status matrix".
- **Scripts** (docs.conductor.build/core/scripts):
  - `.conductor/settings.toml` `[scripts]` has `setup = "pnpm install"`, `run = "pnpm dev --port $CONDUCTOR_PORT"`, `archive = "./script/workspace-archive.sh"`, `run_mode = "concurrent"`.
  - "Conductor allocates ten ports to each workspace: CONDUCTOR_PORT through CONDUCTOR_PORT+9"; pick others with `$((CONDUCTOR_PORT + 1))`.
  - Other env: `CONDUCTOR_WORKSPACE_PATH`, `CONDUCTOR_ROOT_PATH`.
- **Checkpoints** (docs.conductor.build/core/checkpoints):
  - "Before each supported agent responds to a user message, Conductor captures the working branch state in a private Git ref."
  - Revert "will permanently delete all user and AI messages from the selected turn and later".
- **Checks tab.** Git status, PR, CI, deployments, comments, todos.

### F8. Nimbalyst (formerly Crystal)

- **Status.** stravu/crystal's README says "Crystal is now Nimbalyst"; its last push was 2026-02-26.
  - nimbalyst/nimbalyst is MIT, v0.79.0 on 2026-09-29.
- **Product.** Electron desktop with a session kanban, task tracking, and visual markdown and mockups.
  - iOS (Capacitor) and Android companions: see who needs you, reply by text or voice, swipe through diffs, queue tasks, get push.
- **Sync** (nimbalyst@9979578c design/MobileSync/SYNC_ARCHITECTURE.md):
  - CollabV3 on Cloudflare Durable Objects, with AES-256-GCM E2EE.
  - The 32-byte `encryptionKeySeed` is "Shared via QR … never over network". The key is derived with PBKDF2 (salt `nimbalyst:{userId}`, 100,000 iterations, SHA-256).
  - The index room `user:{userId}:index` carries `queuedPrompts`.
- **Pricing.** Free / open source.

### F9. Cursor for iOS

- iOS and iPadOS 26+, paid plans. It controls cloud agents, plus local sessions through Remote Control (cursor.com/docs/cloud-agent/mobile.md).
- Push when an agent finishes a turn. It can "track up to eight agents at once with Live Activities on the lock screen and Dynamic Island".
- Dictation with live transcription. Cache-first loading. Design Mode. No terminal or editor.

### F10. Warp

- **Pricing** (warp.dev/pricing):
  - Free: $0.
  - Build: $20/mo, 1,500 credits.
  - Max: $200/mo, 18,000 credits.
  - Business: $50/user/mo.
- Third-party CLI agents get notifications, rich input, and code review. Warp Factories run agents in the cloud.
- **Remote Control** (docs.warp.dev/agents/cli-agents/remote-control):
  - The `/remote-control` chip publishes the session to Warp's cloud and copies a link.
  - Access is view or edit. There's "Stop sharing" and a red broadcast icon.

### F11. Claude Squad

- smtg-ai/claude-squad, AGPL-3.0, v1.0.20 on 2026-08-20. A tmux plus worktrees TUI launched as `cs`.
- **Keys.** `n`/`N` new, `D` kill, `↵`/`o` attach, `ctrl-q` detach, `s` commit and push, `c` checkout (commits and pauses), `r` resume, `tab` switches preview/diff.
- **Pause** (claude-squad@ce1ffb43 session/instance.go:423-503):
  - Commits `"[claudesquad] update from '%s' on %s (paused)"` (:466).
  - Removes the worktree but keeps the branch, then prunes.
  - Copies the branch name to the clipboard.
- **Resume** (:510) recreates the worktree.
- `-y/--autoyes` runs a daemon that polls every 1000 ms (config/config.go:95) and presses Enter on prompts (daemon/daemon.go). This is risky.

### F12. Emdash

- generalaction/emdash, Apache-2.0, YC W26, v1.2.7 on 2026-09-27. macOS, Windows and Linux.
- Worktrees; SSH/SFTP remotes; issue intake from Linear, GitHub, Jira, GitLab, Asana and more.
- Lifecycle hooks tagged with markers; local SQLite (README).

### F13. VibeTunnel

- amantus-ai/vibetunnel, MIT, v1.0.0-beta.18 on 2026-07-11. The Mac app is Apple Silicon only.
- Browser terminal at `localhost:4020`. `vt <cmd>` runs a command in a browser-visible session. `vt follow` is Git follow mode: the terminal follows the IDE's branch switches.
- **Tailscale Serve** gives HTTPS with automatic certificates. Private mode is the tailnet; Public mode is Funnel. ngrok is also supported (README, Tailscale section).

### F14. Sculptor

- imbue-ai/sculptor, MIT, sculptor-v0.48.0 on 2026-09-21. A research preview.
- Workspaces, Pi and Claude agents, bundled skills, Cmd+K. Docker and remote are experimental (README).

### F15. Dead, pivoted, and out of scope

- **Vibe Kanban:** the README says "Vibe Kanban is sunsetting". A 2026-04-10 blog post says bloop shut down, the project continues as community-maintained, and remote services are removed after 30 days. Last release v0.1.44 on 2026-04-24.
- **Terragon:** the repo says it was "formally known as Terragon Labs"; last push 2026-02-10. Dropped.
- **Omnara:** pivoted to "The open-source alternative to Claude Managed Agents". Dropped.
- **coder/mux:** renamed coder/xum. Not reviewed.
- **Cloud-only:** Jules (jules.google), Devin (Free/Pro/Max/Teams/Enterprise), Factory Droids, Cursor cloud agents. No local-CLI angle.

### F16. Product x feature matrix

`y` = yes, `-` = not offered, `?` = not found in primary sources.

| Product | Parallel worktrees | Phone client | Push | Relay / E2EE | QR pairing | Restore after restart | State source | PR / CI | Voice | CLI / API | License | Price |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Pocket** | y, desktop (P packages/desktop/crates/git/src/git.rs:166-173) | Expo, manual host+token (P packages/app/src/screens/ConnectScreen.tsx:36) | - (r14-9) | - / -; raw tailnet ws | - | - (P packages/pocketd/internal/config/config.go:60) | Claude hooks, transcripts, Codex RPC | - | - (mic button dead, r14) | `pocketd run/attach/hook` (P packages/pocketd/cmd/pocketd/main.go:10) | none (no LICENSE file at repo root) | - |
| Superset | y | iPhone (Pro) | desktop, dock | vendor relay / ? | App Store only | daemon + Resume Args | hooks + wrappers | y, lines → agent | ? | CLI, SDK, MCP | ELv2 | free / $20 |
| Happy | - | iOS, Android, web | y (Expo) | vendor server / E2EE | y | ? | wrapper (`happy claude`) | - | y (ElevenLabs) | CLI | MIT | free |
| cmux | ? | iOS beta | desktop + phone forward | Tailscale peer grant | pairing code | y (resume hooks) | OSC 9/99/777, hooks | - | early access | CLI | GPL-3.0+ (server BSL 1.1) | free / FE |
| herdr | ? | 3rd-party | desktop | - / SSH | - | y (resume argv, handoff) | screen manifests + report API | - | - | CLI, socket API | Apache-2.0 | free |
| Claude RC / desktop | y | Claude app | y, agent-decided | vendor / TLS | y | `-c` continue | native | y (auto-fix, merge, archive) | ? | native | proprietary | Pro+ |
| ChatGPT/Codex | y (15 kept) | ChatGPT app | y | vendor / TLS | y | y (snapshots) | native | ? | ? | native | proprietary | plan |
| Conductor | y | mobile (Pro) | ? | cloud | ? | checkpoints | native | y (Checks tab) | ? | API (Pro) | proprietary | free / $50 |
| Nimbalyst | ? | iOS, Android | y | CF DO / E2EE | y (key seed) | ? | native | ? | y | ? | MIT | free |
| Cursor iOS | cloud | iOS 26+ | y + Live Activity | vendor | ? | cloud | native | ? | dictation | - | proprietary | paid |
| Warp | - | web link | y | Warp cloud | - | ? | native | code review | ? | ? | proprietary | $0-200 |
| Claude Squad | y | - | - | - | - | tmux sessions + pause/resume | tmux pane capture | push | - | TUI | AGPL | free |
| Emdash | y | - | ? | - / SSH | - | ? | marker-tagged hooks | ? (issue intake) | ? | ? | Apache-2.0 | free |
| VibeTunnel | - | browser | ? | Tailscale Serve TLS | - | ? | - | - | - | `vt` | MIT | free |

## Ideas to clone into Pocket

Not already covered by reports 01-14. Checked against: QR pairing (r08-18), relay (r02-1), push (r08-1, r14-9), lock-screen Allow/Deny (r08-19), seen-aware suppression (r08-4, r11-12), quotas (r09-5), accounts (r09-8), automations (r09-13), MCP/control CLI (r11-1), setup env (r06-15), checkpoints (r10-14), CI repair (r09-17), dev server (r06-9), SSH remote (r09-18), dock badge (r09-1), queue (r08-12), timeline persistence (r14-4), more providers (r14-22), mic (r14-17), worktree in pocketd (r11-4), PR badge (r06-8), Cmd+1..9 (r04-2).

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 15-1 | **Restore agents after a pocketd or Mac restart.** Save `{terminalId, cwd, argv, provider, ProviderSessionID}` per PTY. On start, respawn `claude --resume <id>` / `codex resume <id>` in the same cwd. Validate argv with herdr's rules (plain PATH command, no `'` or control chars, ≤64 args, ≤8 KiB). Add a `resume_agents_on_restore` toggle. | A reboot or crash doesn't lose the fleet or its conversations. | herdr.dev/docs/add-herdr-support/, /session-state/; cmux@02dac3c1 README.md:261-317; Superset "Resume Args" | pocketd `internal/terminal` plus new `internal/session` state file. The ID already exists (P packages/pocketd/internal/agent/agent.go:170). Port. | M | none; pairs with r14-4 |
| 15-2 | **`pocketd report` API for any agent CLI.** `--source --agent --state idle\|working\|blocked --seq N [--message] [-- resume argv]`, plus `pocketd release`. Only active when `POCKETD_PTY` is set. Drop reports with seq ≤ last. Clear on return to the shell prompt. Reports override detection. | Any agent (opencode, pi, droid, custom) gets Needs-you/Done status without a new provider adapter. | herdr.dev/docs/add-herdr-support/, /agents/ | new `cmd/pocketd/report.go` over the ops socket (like P packages/pocketd/cmd/pocketd/hook.go:14-37); `internal/daemon` state merge. Port. | M | 15-1 for the resume argv; complements r14-22 |
| 15-3 | **OSC 9/99/777 notifications from PTY output**, plus `pocketd notify <title> [--body] [--sound none\|done\|request]`. | Scripts, tests and unknown agents can raise Needs-you with no integration. | cmux@02dac3c1 README.md:396; herdr CLI `notification show` | pocketd `internal/vt` → `internal/daemon` event; desktop inbox (P packages/desktop/crates/pocket/src/inbox.rs:28) and the phone agent list (P packages/app/src/status.ts:13). The vt wrapper exposes no OSC callback today (P packages/pocketd/internal/vt/vt.go), so it needs a new hook into libghostty-vt. New. | S-M | desktop inbox exists |
| 15-4 | **Agent-requested push plus presence suppression.** Ship a skill or instruction so "notify me when tests finish" calls `pocketd notify --push`. Suppress push while the Mac is in use (desktop focused or recent keyboard input), with a marker-file override. | Push only when it matters and never while you're at the Mac. | code.claude.com/docs/en/remote-control §push, `CLAUDE_CLIENT_PRESENCE_FILE` | pocketd push dispatcher; claude plugin (P packages/pocketd/internal/daemon/plugin.go). Adapt. | S | r08-1 / r14-9 push, 15-3 |
| 15-5 | **iOS Live Activity / Dynamic Island fleet tracker.** Up to 8 agents with status and elapsed time, updated by push. | See the whole fleet on the lock screen without opening the app. | cursor.com/docs/cloud-agent/mobile.md | `packages/app` Expo native widget extension; pocketd APNs liveactivity pushes. New. | L | APNs (r14-9) |
| 15-6 | **Floating status HUD on the Mac.** An always-on-top mini panel over other apps. Priority Needs you > Blocked > Done-unseen > Working. Click to jump. Option-key hotkey. Still frame when reduced motion is on. | Glanceable attention while in another app, instead of hunting windows. | learn.chatgpt.com/docs/pets | desktop `crates/pocket`: a second GPUI window with always-on-top options, opened like the main one (P packages/desktop/crates/pocket/src/main.rs:1072). `overlay.rs` is an in-window picker, not a window. New. | M | none |
| 15-7 | **Keep the Mac reachable.** Hold an IOPM `PreventUserIdleSystemSleep` assertion while any agent is Working or a phone is connected. Toggle; show "Keeping Mac awake" in the UI. | The phone doesn't lose the Mac mid-task. Rivals only tell users to "keep your computer awake". | learn.chatgpt.com/docs/remote; docs.superset.sh/remote-access; `superset hosts wake` | pocketd `internal/daemon` (cgo IOKit or `caffeinate -i -w <pid>`). New. | S | none |
| 15-8 | **Worktree retention.** Cap managed worktrees at 15; protect pinned, running and permanent ones. Before deleting, snapshot to a private ref (`refs/pocket/snapshots/<name>`) and offer restore on reopen. | Worktrees don't pile up, and no work is lost when they're culled. | learn.chatgpt.com/docs/environments/git-worktrees; docs.conductor.build/core/checkpoints | desktop `crates/git` (P packages/desktop/crates/git/src/git.rs:166-173) or pocketd per r11-4. Port. | M | worktree creation (exists) |
| 15-9 | **`.worktreeinclude`.** Pocket already copies a per-repo list, or the root `.env*` files by default, into new worktrees (P packages/desktop/crates/pocket/src/forms.rs:283-291,426-436). Add `.worktreeinclude` as a source for that list, skip symlinks, and stop overwriting (`std::fs::copy` overwrites today). Same filename as Claude and Codex. | The copy list is shared with other tools and lives in the repo. | code.claude.com/docs/en/desktop; learn.chatgpt.com/docs/environments/git-worktrees | desktop `crates/pocket` `copy_list` and the create path (P packages/desktop/crates/pocket/src/forms.rs:283,426). Adapt. | S | none |
| 15-10 | **Per-worktree port block and archive script.** Export `POCKET_PORT` (a block of 10) and `POCKET_ROOT_PATH` into every PTY in the worktree. Run an optional archive/teardown script before removal. | Parallel dev servers don't collide; cleanup is automatic. | docs.conductor.build/core/scripts; Superset `teardown` | pocketd spawn env (P packages/pocketd/cmd/pocketd/run.go:23 sends Env) plus desktop remove path (P packages/desktop/crates/git/src/git.rs:171). Port. | S | per-repo setup script (exists: P packages/desktop/crates/pocket/src/forms.rs:427,447-448) |
| 15-11 | **Auto-archive when the PR merges or closes.** | The sidebar cleans itself up. | code.claude.com/docs/en/desktop §Monitor PR status | desktop `crates/pocket` sidebar plus a PR poller. Pocket has no PR poller yet. Port. | S once r06-8 lands | r06-8 PR badge, 15-8 |
| 15-12 | **E2EE for any relay.** Generate a 32-byte seed on the Mac, pass it only in the pairing QR, AES-256-GCM on every frame, derive keys with PBKDF2 100k SHA-256. The server sees only ciphertext. | Off-tailnet access without trusting a server. | nimbalyst@9979578c design/MobileSync/SYNC_ARCHITECTURE.md; happy README | pocketd `internal/wsserver` framing plus `packages/app` crypto. Port. | M | r02-1 relay, r08-18 QR |
| 15-13 | **Tailscale Serve TLS and a safe bind.** Serve `wss://<mac>.<tailnet>.ts.net` through `tailscale serve`, and show the MagicDNS name instead of a raw IP. Bind pocketd to loopback plus the tailscale interface, not `:4517` on every interface. | TLS on the phone link, a stable hostname, and no exposure on café Wi-Fi. | vibetunnel README, Tailscale section; P packages/pocketd/cmd/pocketd/serve.go:46,57-64 | pocketd `cmd/pocketd/serve.go`; `packages/app` ConnectScreen placeholder (P packages/app/src/screens/ConnectScreen.tsx:36). Adapt. | S | Tailscale installed |
| 15-14 | **Live pocketd upgrade.** Hand PTY fds to the new daemon over SCM_RIGHTS: batches of 64 fds, 30 s ready timeout, 8 KiB replay per pane. | Updating Pocket never kills running agents. | herdr@331775c3 src/server/handoff.rs:20-31,412 | pocketd `internal/terminal`, `internal/ops`. Port. | L | 15-1 as the fallback |
| 15-15 | **Jump to latest unread / Needs you.** ⌘⇧U jumps to the newest unseen; ⌥⌘U toggles unread; "Mark all as read". | One key clears the attention queue. | cmux@02dac3c1 README.md:217-219; learn.chatgpt.com/docs/notifications | desktop `crates/pocket` actions over the existing seen state (P packages/pocketd/internal/agent/agent.go:133; P packages/desktop/crates/pocket/src/inbox.rs:104); a button on the phone agent list (Pocket has no phone inbox). Port. | S | seen state (exists) |

Considered but not cloned:
- Claude Squad pause and `--autoyes`: pause fits r11-4; autoyes presses Enter blindly on prompts.
- Codex Local⇄Worktree handoff: overlaps r11-4.
- Happy voice agent: covered by r14-17. Happy's local/remote switch doesn't apply, since Pocket already drives the same PTY from both sides.
- Superset resources panel, Design mode and Pages.
- Warp public share links: Pocket is single-user.
- Kanban boards from Vibe Kanban and Nimbalyst.
- All cloud agents.

## UI/UX spec to copy

- **Unread keys (cmux README.md:217-219):**
  - ⌘⇧U "Jump to latest unread".
  - ⌥⌘U "Toggle current item unread state".
  - ⌃⌘U "Mark current item as oldest unread and jump to next latest unread".
  - Pane state: blue ring on the pane; the tab lights up (:38).
- **HUD (learn.chatgpt.com/docs/pets):**
  - States Running / Needs input / Ready / Blocked, with priority needs input > blocked > ready > running.
  - Option+Space shows the controls and focuses Quick Chat; a second press keeps them open. "Mini" mode is the no-art variant. Reduced motion shows a still frame.
- **Notification settings (learn.chatgpt.com/docs/notifications):**
  - Turn completion: never / background-only / always.
  - Separate toggles for permission requests and questions. "Mark all as read".
- **Push settings (code.claude.com/docs/en/remote-control):**
  - "Push when Claude decides" and "Push when actions required".
  - Empty state: "No mobile registered".
  - Takeover: "Another connection took over this session".
  - Footer when active: `/rc active`.
- **Status reset (docs.superset.sh/agent-status):**
  - A **Clear Status** action.
  - Agent chips are hidden when only one agent runs.
- **Port clash (docs.superset.sh/remote-access):** a "local port busy" row with a "Use another port" action → `3000 → localhost:54321`.
- **Remote-enable friction (docs.superset.sh/remote-access):** type a confirmation phrase before exposing the host; warn that running terminals will restart.
- **Awake copy:**
  - "Keep your computer awake and online" (learn.chatgpt.com/docs/remote).
  - Pocket should say instead what it's doing: "Keeping Mac awake while agents run".
- **Mode hint (happy RemoteModeDisplay.tsx:223-228):**
  - "Press space (or Ctrl-T) to switch to local mode • Ctrl-C to exit", then "Press space again" to confirm. The confirmation expires after 15 s.
- **Live Activity (cursor.com/docs/cloud-agent/mobile.md):** up to 8 agents per activity.
- **Worktree retention (learn.chatgpt.com/docs/environments/git-worktrees):**
  - Default limit 15.
  - Protected badges: pinned, in progress, permanent.
  - "Restore" offered when reopening a culled chat.
- **Model loadouts (conductor.build/changelog 0.85.0):** ⌃⌘1-5 switch loadout, ⌘⇧/ effort, ⌘⇧E speed.
- **Sharing (docs.warp.dev/agents/cli-agents/remote-control):** a red broadcast icon while shared, and a "Stop sharing" action.

## Open questions / risks

- **Exposure today.** pocketd listens on `":"+port` on every interface and prints a plain `ws://` URL (P packages/pocketd/cmd/pocketd/serve.go:46,58).
  - The token is the only guard on a café LAN. 15-13 is cheap and should come first.
- **Vendor squeeze.** `claude remote-control` (Pro+, QR, push) and ChatGPT "Control this Mac" are free with the plan.
  - Pocket must win on multiple providers, a single fleet view, terminal fidelity, and no vendor relay. Is that enough of a pitch?
- **Superset** is the nearest rival: $20/mo, ELv2, 20+ agents, iPhone, daemon-backed terminals. Its phone app is gated on iOS 26+ and a vendor relay without documented E2EE. That gap is where Pocket can win.
- **Resume argv trust (15-1).**
  - Restored commands run automatically, so they need an allowlist (claude, codex) plus herdr-style argv rules.
  - Never persist env or secrets; cmux drops sensitive env keys and auto-runs only trusted bindings: live-detected tmux or user-approved signed prefixes (README.md:299-307).
  - Screen history stays opt-in, as in herdr.
- **Report API (15-2).** herdr dropped hook-reported state once because stale reports stuck panes (P docs/wayfinder/agent-sessions/01-herdr-agent-state.md:26).
  - Seq plus the shell-prompt fallback is mandatory, not optional.
  - Should the flags match herdr's so integrations work in both?
- **OSC false positives (15-3).** `OSC 9;4` is a progress sequence, not a notification. Parse only notification forms, and rate-limit.
- **Live Activities (15-5).**
  - Expo needs a native widget-extension target.
  - APNs `liveactivity` pushes have a budget.
  - Unclear how 8 agents fit on one compact layout.
- **SCM_RIGHTS handoff (15-14).** Heavy for a Go daemon: fd batches, a versioned protocol, rollback. Restore via 15-1 may be enough.
- **Relay key loss (15-12).** If the QR seed is lost, a re-pair is needed and queued data is lost. Is it acceptable that there's no recovery path?
- **Business signal.** Vibe Kanban sunset after its company shut down, and Terragon is gone. Free-only orchestrators struggle, while paid tiers gate the phone (Superset, Conductor, Cursor).
- **Unverified here:**
  - Superset and Conductor mobile internals (closed source).
  - Whether Claude desktop restores sessions after a crash.
  - Warp's pricing for CLI-agent features.

## Verification

Date: 2026-09-30. Checked 15 claim groups against Pocket @ `86deb13`, herdr v0.9.3 raw docs and `src/server/handoff.rs` @ `331775c3` (via `gh api`), cached cmux/Happy/Claude Squad/Nimbalyst/VibeTunnel/Emdash sources, and the Claude, ChatGPT, Superset, Conductor and Cursor docs. 16 corrections.

Confirmed: Pocket persists only `config.json` and the plugin; the listener binds every interface on port 4517; `add_worktree`, `ProviderSessionID`, the hook ops socket, spawn Env, the `pocketd` usage line, and the missing LICENSE; the herdr report API, seq, resume-argv rules and handoff constants; the cmux unread keys, OSC 9/99/777 and restore; the Claude Remote Control flags and push copy; the Codex 15-worktree retention; the Conductor ten-port block and $50 Pro; Superset pricing, ports and quota copy; Nimbalyst E2EE parameters; Cursor's eight Live Activities.

- TL;DR: Codex ⌥⌘U opens and closes its Activity view. It is not a jump-to-unread key.
- TL;DR: Superset's docs cover daemon-backed terminals and "Resume Args", not layout or cwd restore. Claim narrowed.
- ChatGPT pets and UI spec: Option+Space shows the controls and focuses Quick Chat, and a second press keeps them open. It is not a toggle.
- `serve.go:57` → `:58` for the `ws://` print (two places).
- herdr `handoff.rs:19-23` → `:20-24`, and `19-31` → `20-31`.
- Happy: the confirm timeout is at `:91-99`, not `:95-97`. Ctrl-T switches at once and only space needs the second press. The push citation moved to `:74-82` and the voice-tools citation to `:20-52`.
- herdr 0.6.7 note: the quote now matches the Pocket source. "Seq is herdr's fix for this" is marked as an inference.
- cmux license: the app is GPL-3.0-or-later and the server/relay is BSL 1.1 (README :525). Fixed in F3 and the matrix.
- cmux resume trust: auto-run covers live-detected tmux or user-approved signed prefixes; citation narrowed to :299-307.
- VibeTunnel: `vt follow` is Git branch-follow mode, not terminal mirroring.
- 15-3: Pocket's `vt.go` exposes no OSC hook, so "libghostty-vt already parses" was unsupported. The phone has no inbox; it now points at the agent list (`status.ts`).
- 15-6: `overlay.rs` is an in-window picker. The mapping now points at `cx.open_window` (`main.rs:1072`).
- 15-9: Pocket already copies a per-repo list or root `.env*` into new worktrees (`forms.rs:283-291,426-436`) and overwrites. The idea is rewritten as an adapt of that code, not new code in `git.rs`.
- 15-10: a per-repo setup script already exists (`forms.rs:427,447-448`). The prerequisite is updated and the remove path cited (`git.rs:171`).
- 15-11: Pocket has no PR poller. The idea now depends on r06-8.
- 15-15: the "Seen flag" is now cited (`agent.go:133`, `inbox.rs:104`). There is no phone inbox.
- Unverified, kept as stated: Conductor Teams $60, the Warp tier prices, Emdash and Sculptor README details, and the dates of the Vibe Kanban, Terragon and Omnara pivots.
