# 09 — MonoCode: product and feature inventory

Date: 2026-09-30

Sources:
- hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6 (HEAD 2026-09-30 08:11 +0100)
- https://usemono.dev
- https://dl.usemono.dev/MonoCode.dmg (HEAD only)
- `gh api repos/hardbeat920/monocode/releases` (63 releases) and `gh api repos/hardbeat920/monocode`
- Screenshots: `docs/screenshot.jpg` (v0.1.0) and the README hero image (v0.1.31)
- Pocket worktree `orchestrator-research`, for mapping only

Citation legend:
- `M path:L` is a file and line in the MonoCode clone at the SHA above.
- `P path:L` is a file and line in this Pocket worktree (`packages/...`).
- `Z` (Zeron) is not used in this report.
- A URL is cited as is. "CL x.y.z" means that version's section in `M CHANGELOG.md`.
- Line numbers are 1-indexed.
- Where code and docs disagree, the code is treated as true and the disagreement is listed in §F12.

## TL;DR

- **Positioning.** MonoCode is a free, MIT-licensed Tauri 2 + React 19 desktop GUI over 10 agent CLIs the user already has logged in. The README says it "does not sell tokens" (`M README.md:15`). There are no accounts, pricing, telemetry or paywall (§F1).
- **Architecture.** It drives each CLI headless over a structured protocol: Claude stream-json, `codex app-server` JSON-RPC, ACP for 5 CLIs, Pi RPC and OpenCode HTTP+SSE. It renders its own transcript. Pocket runs real PTYs, so transcript-level features do not port directly (§F2).
- **Pace.** 63 releases in 40 days (v0.1.0 2026-08-20 to v0.5.0 2026-09-29), about 1.5 per day, from one maintainer. 1742 stars, 196 forks, about 180 open issues and PRs (GitHub's `open_issues_count`, which includes PRs). New providers are paused (§F5, `M CONTRIBUTING.md:5`).
- **First run.** There is no wizard.
  - Startup stats about 100 paths for each CLI (30 s TTL). Missing CLIs are disabled and show an install hint.
  - It never checks whether a CLI is logged in. An "Authentication required" modal with browser sign-in appears only after the first failed send (§F3).
- **Distribution.**
  - macOS (ARM and Intel) and Windows: signed builds with Tauri updater feeds on Cloudflare R2.
  - Linux (deb, AppImage, rpm): no auto-update.
  - It checks for updates once at launch and on manual request, then shows "What's new" (§F4).
- **Attention features Pocket lacks.** Dock badge counting Needs-you sessions; notification controls (opt-in, per-project categories, mute 1/4/8 h or until resumed); Pocket desktop already shows native notifications, but without these controls; a working-agents card sorted needs-input, then working, then unseen done; session reminders (§F6.8, §F6.9).
- **Quota features.** Claude and Codex plan-usage footer (5 h and weekly). A usage-limit notice with a reset countdown, and auto-resume at reset + 30 s that holds the queue (§F6.7).
- **Launch surfaces.**
  - Quick composer (macOS only): global ⌘⇧Space panel that starts a session in the background.
  - Automations: schedule triggers plus 9 Inbox event triggers, and 14 templates.
  - `/operator`: an agent-facing app CLI.
  - Orchestration: 1 lead + 1–4 workers (§F6.3–F6.5).
- **Remote access** (v0.5.0, "very early"): a Node host over an SSH local forward, installed as a version-matched OS service, with revocable per-device tokens. Sessions survive the desktop closing. Many features are not available remotely (§F8).
- **Best clones for Pocket**, all mapping onto the existing Needs you / Done / Seen model:
  - Dock badge (09-1)
  - OS notifications with mutes (09-2)
  - CLI probe with install hints (09-9)
  - Usage-limit auto-resume (09-4)
  - Quick composer (09-6)

## Findings

### F1. Positioning and business model

**Copy**
- Tagline: "A desktop UI for your coding agents." (`M README.md:8`)
- Site H1: "A GUI for your coding agents".
- Site meta description: "A desktop UI for the coding agents already on your machine." (https://usemono.dev)
- Cargo description: "MonoCode — one UI for every agent harness", author "Nick" (`M src-tauri/Cargo.toml`).

**Value proposition**
- "Works with your subscriptions … If they're installed and logged in, MonoCode can run them. Tabs are sessions. The composer is the input. MonoCode does not sell tokens." (`M README.md:15`)

**Business model: none visible**
- MIT license (`M README.md:124`).
- The site is a single hero page with CTAs "Download for Mac" and GitHub (1.7K stars). It has no pricing, sign-up or waitlist (https://usemono.dev).
- `grep` for posthog, sentry, telemetry or analytics found no SDKs.
- The only growth hook is an optional "star MonoCode on GitHub" prompt that uses `gh`:
  - `M src/app/shell/GithubStarPrompt.tsx:10`, dismiss key `monocode.githubStarPrompt.dismissed.v1`
  - `M src/features/inbox/model/githubTasks.ts:264-269`
  - CL 0.1.52

**Governance**
- Single maintainer (`M CONTRIBUTING.md:3`).
- "New providers paused"; such PRs are closed (`M CONTRIBUTING.md:5,50-54`).
- NOTICE disclaims any affiliation with the provider vendors.

**Maturity**
- README: "very early… expect bugs" (`M README.md:44`).
- `App.tsx` is 11,342 lines (one god component).
- Rust side: 42,927 lines across 42 modules.

### F2. Providers supported

All 10 providers are shown on the site and in the README (`M README.md:15-30`). The union type is at `M src/features/sessions/model/session.ts:18-41`. The shared adapter contract is `HarnessAdapter` (`M src/integrations/harness/core/registry.ts:47-101`).

| CLI | Login (README) | Spawn protocol (code) | Cite |
|---|---|---|---|
| Claude Code | `claude auth login` | `--output-format stream-json --verbose --input-format stream-json --permission-prompt-tool stdio --include-partial-messages --setting-sources=… --settings <json> --model --effort --permission-mode [--allow-dangerously-skip-permissions] [--resume\|--session-id] [--max-turns]` | `M src/integrations/harness/providers/claude/claudeProtocol.ts:239-291` |
| Codex | `codex login` | `codex app-server` (JSON-RPC v2) | `M …/providers/codex/codex.ts:520` |
| Cursor | `agent login` | `agent acp` | `M …/providers/cursor/cursor.ts:298-308` |
| Grok Build | curl install, then `grok login` | `--no-auto-update agent --no-leader --model --reasoning-effort [--always-approve] stdio` | `M …/providers/grok/grokProtocol.ts:94-104` |
| OpenCode | `opencode auth login` | `serve --hostname=127.0.0.1 --port=` (HTTP+SSE) | `M …/providers/opencode/opencode.ts:411` |
| Antigravity | curl install, then run `agy` once | spawns `agy_acp_server.par` (ACP), not `agy acp` | `M …/providers/antigravity/antigravity.ts:83,133` |
| Pi | `npm i -g @earendil-works/pi-coding-agent` | `--mode rpc` (JSONL) | `M …/providers/pi/piProtocol.ts:153` |
| omp | `curl -fsSL https://omp.sh/install \| sh` | `--mode rpc` (Pi flavor) | `M …/providers/pi/piFlavor.ts:5` |
| fx | curl setup, then `fx login` | `fx acp [--model]` | `M …/providers/fx/fx.ts:443` |
| Hermes Agent | install script, then `hermes model` | `acp` | `M …/providers/hermes/hermes.ts:256` |

**Adapter surface** (`M core/registry.ts:47-101`)
- Turn control: `sendTurn`, `steerTurn`, `cancelTurn`, `compactContext`, `rewindLastTurn`
- Prompts: `respondApproval`, `respondQuestion`, `keepQuestionOpen`
- Lifecycle: `stopSession`, `forgetSession`, `bindSession`, `refreshCatalog`
- Generated text: `generateTitle`, `generateCommitMessage`, `generatePrContent`, `generateBranchName`
- Idle processes are parked after `HARNESS_IDLE_PARK_MS = 5*60_000` (`M core/registry.ts:107`).

**Claude side calls** (title, commit message and similar) are isolated: they add `disableAllHooks`, `--no-session-persistence` and an empty strict MCP config (`M claudeProtocol.ts:239-291`).

**Claude Code hooks** used to be disabled. Since 0.1.19 they run for normal turns, behind a toggle `CLAUDE_HOOKS` that defaults to true (`M src/features/settings/model/settings.ts:915`; CL 0.1.19).

**Runtime modes** (`M session.ts:355-388`)
- Values: `supervised | auto-accept-edits | auto | full-access`. Default is `supervised` (`M session.ts:373`).
- Turn intents: `default | plan | build | orchestrate` (`M session.ts:73`).

**Named accounts** (0.1.50) are implemented by setting env vars: `CLAUDE_CONFIG_DIR` and `CODEX_HOME` (`M src-tauri/src/harness.rs:681,688`). The default account id is `"default"` (`M src/features/providers/model/providerAccounts.ts:8`).

### F3. Onboarding / first run

**Availability probe** (`M src/integrations/harness/core/availability.ts`)
- Checks only that the binary exists, never auth (`:33-53`).
- `PROBE_TTL_MS = 30_000`. About 100 candidate paths are statted, including those from nvm, fnm, mise and Volta found via the login-shell PATH (TTL and ~100-path comment `:57-64`; version managers `M src-tauri/src/harness.rs:2355,2518`; CL 0.1.6).
- Hint copy (`:66-70`): "`<name> not found<how>. Install it, or restart MonoCode if it is already installed.`"
- Missing CLIs are disabled with an install hint (`M CONTRIBUTING.md:29`).
- Codex falls back to the CLI bundled in Codex.app (CL 0.1.6).

**Auth**
- Nothing happens until a send fails. Then `ProviderSignInDialog` appears with title "Authentication required" and body "Sign in to continue using X." (`M src/features/sessions/ui/ProviderSignInDialog.tsx:44-47`).
- Browser sign-in exists for Claude, Codex, Cursor, Grok and fx (CL 0.1.49).
- Antigravity has no browser sign-in; its help text is "Run `agy` once in Terminal to sign in." (`M antigravity.ts:83`).

**Empty state**
- A welcome scene (`AstraWelcome`, `OpusWelcome`).
- An empty-terminal arcade (snake, pacman, grid), on by default: `GRID_ARCADE` true (`M settings.ts:801`).

**Opt-ins**
- Notifications are off by default. Enabling them asks the OS for permission: `NOTIFICATIONS_DEFAULT = false` (`M src/features/notifications/model/notifications.ts:12-13`).

**Per-provider CLI binary path override** (CL 0.3.0; `M src/features/providers/model/providerBinaryPaths.ts`).

### F4. Distribution and update

**Platforms**
- macOS Apple Silicon: `dl.usemono.dev/MonoCode.dmg`
- macOS Intel: `dl.usemono.dev/MonoCode_x64.dmg`
- Linux x86_64: `.deb` and AppImage, plus `.rpm` for Fedora/EL10
- Windows: NSIS installer, per user (`M README.md:32-38`; `M src-tauri/tauri.conf.json`)
- The DMG is served by Cloudflare: 13,963,539 bytes, `Cache-Control: no-store`.

**Release pipeline** (`M .github/workflows/release.yml`)
- A `v*` tag triggers it. The tag must match the versions in package.json, Cargo.toml and tauri.conf.json, and `CHANGELOG.md` must contain that version (`:42-65`).
- `APPLE_SIGNING_IDENTITY` must be a real identity, not ad-hoc. The updater pubkey and endpoint are injected from secrets (`:67-107`).
- Both mac architectures are signed and notarized, then checked with `lipo` and `codesign` (`:109-121`).
- Uploads go to R2 at `releases/<ver>/` with immutable cache headers. The "latest" DMGs are mutable and served `no-store` (`:166-282`).
- `latest.json` covers `darwin-aarch64`, `darwin-x86_64` and `windows-x86_64`. **There is no Linux updater** (`:234-265`).
- The GitHub Release is created as a resumable draft, then published (`:284-378`). The updater feed is published last (`:380-393`).

**Updater UX**
- Probes once on mount; checks manually from the menu or Settings (`M src/app/shell/SidebarUpdate.tsx:34-68`; `M src/app/shell/MenuBar.tsx:198`; `M src/features/settings/ui/SettingsView.tsx:1732`).
- Flow: `probeForUpdate`, then `runUpdateFlow`, then `installPendingUpdate` (`M src/app/model/updater.ts:39,46,112`).
- "What's new" dialog after an update (CL 0.1.23; `M src/app/shell/WhatsNewDialog.tsx`, `M src/app/model/releaseNotes.ts`).
- Quit asks for confirmation if chats are running, and restores them on relaunch (CL 0.1.2).

**Harness CLI updates**
- Versions are compared against `https://registry.npmjs.org` (`M src-tauri/src/harness_updates.rs:8-22`).
- Only 4 CLIs are covered. Each is updated with its own command (`:26-35`):

  | CLI | npm package | Update command |
  |---|---|---|
  | claude | `@anthropic-ai/claude-code` | `claude update` |
  | codex | `@openai/codex` | `codex update` |
  | opencode | `opencode-ai` | `opencode upgrade` |
  | pi | `@earendil-works/pi-coding-agent` | `pi update --self` |

- UI: `HarnessUpdateNotice`.

**Security defaults at v0.1.0** (CL 0.1.0)
- `harness_exec` allowlist
- CSP
- No remote images in agent markdown
- Updater key injected at release time
- Ad-hoc signing by default for local builds

### F5. Release cadence and feature evolution (latest first)

**Cadence**
- 63 GitHub releases, from v0.1.0 (2026-08-20T17:22Z) to v0.5.0 (2026-09-29T12:38Z): about 1.5 per day.
- Busiest days: 2026-09-01 (4 releases), 09-08 (3), 09-28 (3).
- Every release body reads "See CHANGELOG.md for details."

**Asset growth**

| Version | Assets |
|---|---|
| v0.1.0–0.1.24 | 3 (ARM mac only) |
| v0.1.25 | adds Linux |
| v0.1.35 | adds Windows |
| v0.1.39 | adds `.exe.sig` |
| v0.1.43 | adds x64 DMG |
| v0.4.0 | adds rpm |
| v0.5.0 | 23 assets, including 6 host archives + `.sha256` |

**v0.5.0 GitHub download counts** are small because mac downloads come from R2: exe 50, AppImage 34, host-linux-x64 29, aarch64 dmg 21, x64 dmg 15, host-darwin-arm64 13, rpm 11.

**Evolution** (CL line numbers in parentheses)
- **Unreleased (L8-14):** Windows host ACL fix. The remote catalog is re-probed after a CLI update or after 5 min.
- **0.5.0, 09-29 (L20-24):** experimental SSH remote access (#432) for all 10 providers, including Explorer, Changes, worktrees, attachments, drafts and Plan remotely.
- **0.4.3 / 0.4.1, 09-28:** performance work; ⌘B toggles the rail.
- **0.4.0, 09-28 (L58-67):**
  - Operator gains `notes.write`, worktrees and pane placement.
  - Multi-folder open; paste a screenshot or file.
  - Codex generated images.
  - Claude and Codex account readiness and usage, plus an "account headroom" suggestion (#492); Pi usage.
  - rpm packages; ⌘⇧C copies a path.
  - Celebrations when plan or orchestrator turns finish.
- **0.3.0, 09-27 (L87-91):** record, disable or reset keybindings (#438); per-provider binary path; `.jsonc`; archive or delete everything in a tab.
- **0.2.0, 09-25 (L117-121):**
  - BTW read-only side conversation (#353).
  - `/operator` (#423).
  - Account identity (plan, email, org).
  - Usage-limit notice with a reset countdown, and auto-resume that pauses the queue.
- **0.1.56, 09-24 (L140-146):**
  - Quick composer.
  - PR checks with Actions jobs; "send failures to an agent" (#364).
  - Preview tabs; per-project default provider and model; format on save.
- **0.1.55, 09-23 (L166-172):** Jira and Jira automations; in-transcript Find; ⌘↑↓ within a tab; ⌘⇧B session sidebar; background effects.
- **0.1.54, 09-22 (L191-197):** edit, rewind or resend the last message; link sessions to GitHub issues and PRs; markdown preview; Pull in Changes.
- **0.1.53, 09-21 (L216):** attach to an existing worktree.
- **0.1.52, 09-20 (L228-237):**
  - Automations; Antigravity; Azure DevOps.
  - Persistent drafts.
  - Draft workspace: a worktree plus a descriptive branch is created on the first turn.
  - Amend; star prompt; files as tabs.
- **0.1.51, 09-18 (L259-263):** git worktrees for parallel sessions and a Settings → Worktrees page; named accounts can be renamed or removed; command palette; save messages to Notes.
- **0.1.50, 09-17 (L280-290):** Hermes; project groups; Windows tray; multiple named Claude/Codex accounts per project (#280); create-branch dialog; open in an external editor.
- **0.1.49, 09-16 (L309-313):** detailed limit views; redeem Codex banked resets; mark all read; browser sign-in.
- **0.1.48, 09-16 (L336-339):**
  - Drag a pane into the tab strip to detach it.
  - **Per-project notification categories, and mute for 1/4/8 h, custom, or until resumed.**
  - PR merge, draft, close and reopen.
  - **Title-bar teal check for unseen responses.**
- **0.1.47, 09-15 (L357-365):** Settings search; merge tab layouts; accent color; model flyouts with an effort picker; orchestration lead cards; **Working agents preview when the rail is collapsed**.
- **0.1.46, 09-14 (L385-393):** orchestration v1 (lead + up to 4 workers); token and cache hover; link previews.
- **0.1.45, 09-13 (L418-424):** subagent rows; GitLab To-Dos; linked-session activity notices; skill preview.
- **0.1.44, 09-12 (L442-445):** **session reminders**; the six most recent models (⌘.); note tags.
- **0.1.43 … 0.1.35 (L468-606):** GitLab; Settings → Skills; staged/unstaged review; signed Windows updates; project picker; interface scale 50–200%; **notifications (off by default)**; Windows support.
- **0.1.33, 0.1.32 (L638-656):** /compact and a context meter; Plan mode; queued follow-ups with Steer; checkpoint Review.
- **0.1.31 … 0.1.19 (L680-844):** git graph; unified diff; session folders; Inbox comments; Linux builds; Handoff; running-terminal chip; "What's new"; Grok; **Working agents card (≥2 in flight; finished stay until opened)**; Notes; Claude hooks enabled.
- **0.1.18 … 0.1.10 (L862-992):** sounds; Second opinion; zen mode; Inbox (gh, Linear); **plan usage footer (5 h / weekly)**; mascot; omp.
- **0.1.8 … 0.1.0 (L1006-1125+):** project archive; Deck layout; terminal dock ⌘J; ⌘K search; filters; Handoff on provider switch; login-shell PATH; Pi; hide on window close; light mode; context ring; snake game; 0.1.0 shipped macOS ARM with Claude, Codex, Cursor and OpenCode, files, git, checkpoints, terminal and the updater.

### F6. Feature inventory by area (one line per `src/features/*`, confirmed in code)

**F6.1 Sessions** (`M src/features/sessions`)
- `model/session.ts` contains:
  - `HarnessId` (10 providers)
  - `TurnIntent`
  - `RuntimeMode` with labels and hints (`:355-388`)
  - `WorkspaceMode current|worktree` (`:390`)
- Model files:
  - `btw`
  - `handoff`
  - `secondOpinion`
  - `plan`
  - `messageQueue`: follow-up behavior defaults to `"steer"` (`M settings.ts:534`)
  - `usageLimit`
  - `contextUsage`
  - `liveAgents`
  - `sessionReminders`
  - `editLastTurn`
  - `checkpoint`
  - `compact`
  - `sessionFolders`
  - `sessionFilters`: working, needsApproval, done (`:9-61`)
  - `draftCache`
  - `promptOutline`
  - `transcriptFind`
  - `inFlight`
- UI: `Composer`, `AgentTranscript`, `SessionPane`, `ModelPicker`, `ApprovalToasts`, `BtwSheet`, `LiveAgentsPreview`, `ProviderSignInDialog`, `turnCelebration`.

**F6.2 Workspace** (`M src/features/workspace`)
- `PaneTree`, `SurfaceTabs`, `TabGroupMenu`, `WorkspacePicker`
- Models: `layout`, `paneDrop`, `workspaceSnapshot`, `tabVisitHistory`, `tabGroups`
- Split panes, tabs that can be detached, and back/forward history.

**F6.3 Quick composer** (`M src/features/quick-composer`; `M src-tauri/src/quick_composer.rs`)
- Doc comment (`quick_composer.rs:1-4`): "Spotlight-style composer: a global shortcut floats a small panel over whatever app is in front, and its prompt starts a session in a workspace window without bringing that window forward."
- macOS only (CL 0.1.56; keybinding row gated on `IS_MAC`, `M settings.ts:946-954`). Default shortcut `Command+Shift+Space` (`M src/features/quick-composer/model/quickComposerShortcut.ts:4`; `quick_composer.rs:55`).
- Return starts the session in the background; ⌘Return starts it and opens it (CL 0.1.56).
- UI: `QuickModelSelector`, `QuickPermissions`, `QuickWorkspaceControls`, `QuickGitPopup`.

**F6.4 Automations** (`M src/features/automations`; `M src-tauri/src/automations.rs`)
- Triggers: `time|github|linear|jira|gitlab|azuredevops` (`model/automations.ts:10-11`).
- Schedules: `hourly|daily|weekdays|weekly` (`:9`).
- Workspace: `current|worktree|existing` (`:8`).
- Run status: `pending|running|succeeded|failed|skipped|cancelled` (`:27-28`).
- Other fields: `reuseSession`, `runtimeMode`, `missedRunGraceMinutes` (`:32-60`).
- **Event triggers** (`model/automationEvents.ts:34-40`):
  - github: `draft_opened`, `pull_request_opened`, `issue_opened`
  - gitlab: `merge_request_opened`, `issue_opened`
  - linear and jira: `issue_created`
  - azuredevops: `pull_request_appeared`, `work_item_appeared`
- **14 templates in 6 categories** (`model/automationTemplates.ts:3-10,52-337`):
  - Categories: Popular, Code Review, Security, Incidents & Triage, Data & Research, Environment.
  - Templates: Find critical bugs, Scan codebase for vulnerabilities, Generate docs, Add test coverage, Review pull requests, Review draft PRs, Audit dependencies, Scan for secrets, Triage GitHub issues, Triage new issues, Watch failing checks, Weekly changelog, Repo health check, Environment doctor.
- **Scheduler** runs in the renderer: `setInterval(evaluate, 30_000)` in `M src/app/App.tsx:7286`. It calls `claimDueAutomations`, which invokes `automations_claim_due` (`model/automations.ts:497,520`). The app must be running.
- UI: `AutomationsView`. Not available remotely (`M docs/remote-access.md:40`).

**F6.5 Orchestration** (`M src/features/orchestration`)
- One lead plus `maxWorkers` 1–4, enforced with the error "Choose 1 to 4 workers" (`model/orchestration.ts:694-695`).
- Workers originally shared the lead's checkout (CL 0.1.46). Since 0.1.52 each worker gets a recoverable worktree seeded from the lead checkout (CL 0.1.52 L242; `M src/app/App.tsx:8663`; `M src-tauri/src/worktrees.rs:392`).
- Worker envelope `<monocode_assignment>` carries a write scope. Workers must not spawn agents, create worktrees, switch branches, stage, commit or push (`:174`).
- Lead envelope `<monocode_orchestration>` uses the control CLI:
  - `requestId` retry
  - `dependsOn`
  - `needsInput`
  - steer, message, review and finish (`:833`)

**F6.6 Agent app / `/operator`** (`M src/features/agent-app`; `M README.md:46-55`)
- `/operator` (aliases `/mono`, `/monocode`) gives the agent MonoCode access through the local `app` CLI for that thread.
- Actions:
  - `models.list`
  - `sessions.start`: placement right/down, `besideSessionId`, `draft:true`, `worktreeCwd`, `runtimeMode inherit`
  - `worktrees.create`
  - `sessions.list`, `sessions.read` (3 exchanges plus a cursor), `sessions.send`, `sessions.draft`
  - `folders.list`, `folders.move`
  - `notes.list`, `notes.read`, and since 0.4.0 `notes.write`
- The operator bubble is amber and translucent. Orchestration workers do not get it.
- Handler `handleAgentApp` (`model/agentApp.ts:284`).
- Transport (`M src-tauri/src/control.rs`, `control_cli.rs`): an authenticated loopback. The desktop executable doubles as a JSON-only client. "App windows own execution; callers never receive arbitrary Tauri command access or direct database write access."

**F6.7 Providers / usage** (`M src/features/providers`)
- **Rate limits** (`model/rateLimits.ts:3,45-48`):
  - `RateLimitProvider = claude|codex|opencode`
  - `SESSION_WINDOW_MINUTES 300`, weekly `10_080`, monthly `43_200`
  - `RATE_LIMIT_POLL_MS` 15 min
- **Claude usage** is fetched from `https://api.anthropic.com/api/oauth/usage` with header `oauth-2025-04-20`, user agent `claude-code/2.1.0`, and the Keychain item "Claude Code-credentials" (`M src-tauri/src/rate_limits.rs:14-23`). This is an undocumented endpoint.
- **Codex usage** comes from `account/rateLimits/updated` app-server notifications (`M …/providers/codex/codexProtocol.ts:274,411`).
- **Usage limit handling** (`M src/features/sessions/model/usageLimit.ts:5-26`):
  - `USAGE_LIMIT_RESUME_GRACE_MS = 30_000`
  - Reset format: "3:16 AM · in 4h 42m", or "Sep 26, 3:16 AM · in 1d 4h" on a later day.
  - `usageLimitResumeDue` fires when the session is idle, armed, and `now >= resetsAt + 30s`.
  - Claude limits are detected from a rate-limit event or a usage-limit result (`M …/providers/claude/claude.ts:663,909-910`).
- Accounts: `providerAccounts.ts`. Harness updates: `harnessUpdates.ts` (`:12-17`). Pi usage: `piUsage.ts`.
- UI: `ProviderAccountUsage`, `HarnessUpdateNotice`.
- Context hover shows "69% context used" and "176K / 256K tokens" (`M src/features/sessions/model/contextUsage.ts:42`). The ring turns amber at 75% and red at 90% (CL 0.1.1).

**F6.8 Notifications** (`M src/features/notifications`; `M src-tauri/src/notifications.rs`)
- Categories (`model/notificationPreferences.ts:1-7`): "Pull requests / Merge requests", "Issues and Linear tasks", "Agent finished", "Agent approvals and questions", "Reminders".
- `NOTIFICATION_MUTE_HOURS [1,4,8]`; `mutedUntil null` means until resumed; `resumedAt` suppresses stale activity (`:11-23`).
- Preferences are per project, key `monocode.projectNotifications.v1`.
- Delivery: `UNUserNotificationCenter` on macOS, freedesktop on Linux, WinRT toast on Windows. Clicking jumps to the session (`NOTIFICATION_CLICK_EVENT`, `notifications.ts:18`).
- **Dock badge** is the count of `sessionNeedsInput`, deduped against the last count (`model/dockBadge.ts:6-15`). Native side: `set_dock_badge` calls `macos::set_window_badge` per window (`M src-tauri/src/lib.rs:192-198`; `M src-tauri/src/macos.rs:98-112`).
- Also: `approvalToast.ts`, `useInputNotifications`, `useSessionReminders`, `NotificationMuteDatePicker`.

**F6.9 Sessions: live agents and reminders**
- **Live agents** (`M src/features/sessions/model/liveAgents.ts:23-80`):
  - Includes in-flight sessions plus unseen finished ones. Excludes Inbox-Ask sessions and orchestration workers.
  - Sort: needs approval or question first, then done last, else by turn start.
  - Activity text: "Done", the question title, or the current tool title.
  - Elapsed format: `12s`, `4m 3s`, `1h 5m`.
  - The rail card appears when ≥2 chats are in flight; finished ones stay until opened (CL 0.1.21).
  - Setting `LIVE_AGENTS` defaults to true (`M settings.ts:758`).
- **Reminders** (`M src/features/sessions/model/sessionReminders.ts:7-33`):
  - Record fields: `sessionId`, `dueAt`, `firedAt`, `title`, `harness`, `cwd`.
  - Presets: `1h`, `3h`, `evening` (18:00), `tomorrow` (09:00), `next-week` (next Monday 09:00).
  - Stored in the SQLite `session_reminders` table (`M src-tauri/src/reminders.rs`).

**Other feature folders**

| Folder | Contents | Cites |
|---|---|---|
| F6.10 inbox | `InboxProvider = github\|linear\|jira\|gitlab\|azuredevops`. PR state actions; composer chip; CI repair (`ciRepair.ts`, `ciRepairSessions.ts`, `githubPrChecks.ts`); `inboxAsk.ts`; `linkedSessionUpdates.ts`; `inboxSeen.ts`, where the first snapshot does not add badges. UI: `InboxView`, `InboxPrDiff`, `InboxPrChecks`, `CheckRepairForm`. | `M src/features/inbox/model/githubTasks.ts:75-76,518,1210`; `inboxSeen.ts:161` |
| F6.11 connections | Remote machines: `ConnectionsSettings`, `AddRemoteProjectDialog`, `RemoteSession`. Models: `protocol`, `remoteCommands`, `remoteSessionState`, `remoteAttachments`. | `M src/features/connections` |
| F6.12 files | `FileTree`, `FileEditor` (CodeMirror 6 with git gutter, lint, scrollbar markers, mermaid, autocomplete), `FilePreview`, `FilePicker`, `fileMentions`, `BinaryFileView`, `EditorSelectionMenu` ("Add to chat"). | `M src/features/files` |
| F6.13 notes | `notes.ts`, `noteImages.ts`, `NotesView`, `NoteMiniCard`; SQLite `notes` table. | `M src/features/notes`; `M src-tauri/src/notes.rs:56-58` |
| F6.14 projects | 8×8 pixel mascots with rest and talk frames; project groups; logos; chat backgrounds; recents; project terminal. UI: `CwdPicker`, `SearchableProjectPicker`, `RemoveProjectDialog`. | `M src/features/projects/model/projectMascots.ts:1-6,247`; `projectGroups.ts:10` |
| F6.15 search | Global ⌘K with scopes `all\|conversations\|files\|projects` and per-scope limits. | `M src/features/search/model/appSearch.ts:19,100`; backend `M src-tauri/src/search.rs`: git grep then fallback, 500 matches, 512 KiB/file, 4 MiB grep output |
| F6.16 settings | Sections: App (general, connections, appearance, keybindings), Agents (chat, providers, skills), Workspace (inbox, archive, worktrees). Keybinding table; `appearance`, `uiScale`, `sounds`, background effects. | `M src/features/settings/model/settings.ts:20-146`; keybindings `:938-1030` |
| F6.17 skills | `SkillsPage`, `SkillPicker`, `SkillDocumentPreview`, `createSkill`. Roots: `.agents/skills` first, then native dirs for each CLI (`.claude`, `.cursor`, `.codex`, `.opencode`, `.pi`, `.omp`, `.fx`, `.grok`, `.hermes`), `~/.gemini/antigravity/skills`, and Claude plugin skills from `~/.claude/plugins/installed_plugins.json`. | `M src-tauri/src/skills.rs:80-156,201` |
| F6.18 source-control | Worktrees page and picker; create/delete worktree; branch picker; create/switch branch; `GitChangesPanel`; `GitHistoryGraph` (200 commits); `UnifiedDiffView`; `SessionChangesDiff`; `DiffCommentComposer` (line comments go to the composer). | `M src/features/source-control` |
| F6.19 terminal | `TerminalView` (xterm 6), `ProjectTerminalDock` (⌘J), arcade games, `terminalKeys`. PTY backend: 32 KiB reads, 8 ms coalesce, 1 s kill escalation. | `M src/features/terminal`; `M src-tauri/src/pty.rs` |

**Settings defaults worth noting** (`M settings.ts`)

| Setting | Default | Line |
|---|---|---|
| `FILE_TAB_MODE` | `"pane"` | 557 |
| `TAB_ANIMATIONS` | false | 577 |
| `MODEL_CONTROLS` | `"menu"` | 636 |
| `COMPOSER_RUNNER` | true | 677 |
| `NOTES_ENABLED` | true | 696 |
| `QUICK_COMPOSER_ENABLED` | true | 723 |
| `CLOSE_TO_TRAY` | true | 787 |
| `DIFF_VIEWER` | `"editor"` | 832 |
| `FORMAT_ON_SAVE` | true | 872 |
| `AUTOSAVE` | false | 885 |

**Persistence**
- SQLite `monocode.db` (`M src-tauri/src/session_store.rs`, 3530 lines).
- Tables: `sessions`, `in_flight_sessions`, `workspace_snapshot`, `worktree_removals`, `notes`, `session_reminders`, `automations`, `automation_runs`, `automation_event_claims`.

### F7. `src-tauri/src` modules (line counts; key commands)

**Sessions and agents**
- `harness` (3475): `harness_resolve_*` ×10, `harness_free_port`, `harness_write/kill/kill_all/http/sse_open/sse_close/exec`, `provider_account_remove`
- `session_store` (3530)
- `checkpoint` (2072): `session_checkpoint_ensure/prepare/capture/status/apply/cleanup_safe/forget/file_diff/undo/keep`
- `pty` (834): `pty_spawn/write/resize/status/kill/kill_all`
- `control` (844) and `control_cli` (560)
- `harness_updates` (161)
- `account_identity` (238)
- `rate_limits` (886): `fetch_claude_usage`, `fetch_opencode_go_usage`
- `pi_usage` (618)
- `cursor_store` (815)

**Git and files**
- `fs` (8606): list_dir, git diff/stage/unstage/discard/commit/push/pull/sync/history/pr_create, GitHub star
- `worktrees` (1204): `git_worktrees`, create, orchestration worktree create/remove, rename_branch, check_remove, remove
- `worktree_lifecycle` (107)
- `search` (679)
- `external_editor` (275)

**Integrations**
- `jira` (1247)
- `linear` (944)
- `gitlab` (1314)
- `azure_devops` (2420)
- `inbox_media` (350)
- `link_preview` (526)

**Automation, notes, reminders**
- `automations` (1302): `automations_list/upsert/delete`, `automation_runs_list/recover`, `automation_run_now`, `automations_claim_due/claim_event`, `automation_run_update`
- `notes` (702)
- `reminders` (564)
- `notifications` (753)

**Remote**
- `remote` (728): the renderer never sees the bearer credential
- `remote_ssh` (887)
- `ssh_askpass` (201): nonce-authenticated loopback socket

**Shell and OS**
- `lib` (626): `set_dock_badge`, `set_traffic_lights_visible`, `set_window_background_blur`, `open_new_window`, `default_cwd`, `home_dir`
- `macos` (798): traffic lights, blur. The overlay titlebar is about 28 pt; the HTML tab bar is 40 px.
- `macos_background` (153)
- `window` (715): glass, hide, quit poll and decision
- `window_transfer` (30)
- `windows` (354)
- `tray` (48, Windows only)
- `menu` (551): `keybindings_set_overrides`, `autosave_set_enabled`
- `quick_composer` (791)
- `pasteboard` (465): file URLs and images from the native clipboard
- `chat_background` (210)
- `project_logo` (201)
- `skills` (1123)
- `main` (20)

**Total:** 42,927 lines.

### F8. Remote access (v0.5.0, experimental) — `M docs/remote-access.md`

**Model**
- The host owns the provider processes and the session DB. Sessions survive the desktop closing, a tab closing, or the tunnel dropping (`:3`).
- A remote folder appears as a project with a globe marker (`:5`).

**Setup** (`:9-11`)
- Path: Settings → Connections → Add machine, using an SSH address or alias.
- Setup downloads a version-matched host, verifies its checksum, installs it as a service, pairs, and opens the forward. Node is bundled with the host.

**Services** (`:16-20`)
- Linux: systemd user service plus `loginctl enable-linger`.
- macOS: LaunchAgent `com.monocode.host`.
- Windows: per-user Task Scheduler task.
- Supported hosts: Windows 10/11/Server 2019+, Linux, macOS; x64 and arm64.

**Security**
- Host-key and password prompts appear inside Settings; passwords are not saved (`:22`).
- The forward binds to a temporary loopback port (`:24`).
- Credentials are stored in `remote-machines.json` with mode 0600, not in the keychain (`:69`).
- Two removal options: Remove (desktop only) or "Revoke access and remove" (`:26`).
- Tokens can be revoked per device via the host CLI: `status`, `devices`, `revoke`, `stop`, `service uninstall` (`:73-89`).

**Transport** (`:57,62,67,101`)
- The host listens on `127.0.0.1:3774`; state lives in `~/.monocode-host`.
- Manual forward: `ssh -N -L 3774:127.0.0.1:3774`.
- The desktop polls every 0.75 s while a turn runs and every 3 s otherwise.
- Responses are capped at 16 MiB; syncs above 4 MiB are chunked. Output is written in 120 ms batches.
- Commands retried with the same ID are idempotent.

**Not available remotely** (`:40,103`)
- @ mentions
- Skills and slash commands other than `/plan` and `/compact`
- Operator
- Terminals
- Worktree deletion
- Automations
- Orchestration
- LAN discovery
- Account tunnels

**Remote file limits** (`:36`): reads and edits up to 1 MiB each, refreshed every few seconds. Commit messages are not generated remotely.

**Future:** the host reuses the TS adapters on a Node backend; "a Rust daemon is planned" (`:105`).

### F9. Inbox / Jira — `M docs/jira.md`

- Setup: Settings → Inbox → Jira with site, email and an unscoped API token, validated before save. Scoped tokens and Data Center are not supported (`:3-7`).
- Actions: browse, comment, Ask, Start work (`:9-11`).
- Limits (`:15-17`):
  - 40 open issues, or 100 when closed ones are included
  - "all statuses" covers the last year
  - latest 50 comments
- The automation "Jira → Issue appeared" is driven by polling, not webhooks (`:19-22`).
- Credentials are stored in the app data dir with owner-only permissions. Disconnect clears them (`:24-26`).

### F10. UI observed (screenshots)

**v0.1.0** (`M docs/screenshot.jpg`)
- **Left sidebar:** Sessions/Files tabs. Each session card shows:
  - model ("Claude Opus 5", "Cursor Grok 4.6")
  - relative age (3m / 22m / 7h 59m)
  - bold title
  - `monocode/main` branch row
  - `+478 −2`
  - provider icon
- **Title bar:** two-line tabs (title and subtitle) and a `+949 −10` badge.
- **Transcript:** "Worked for 2m 39s" rows, inline diff cards, and a "2 Files · Undo All / Keep All / Review" bar.
- **Composer:**
  - cwd and branch row
  - placeholder "Ask, build, / for skills..."
  - chips: `+`, model, effort (High / Extra High), Fast, Supervised
  - send button
- **Right Changes panel:** "Message (⌘↩ to commit)", a Commit split button, and "CHANGES 6" with M/U letters.

**v0.1.31** (README hero)
- **Deck layout rail:**
  - Search ⌘K, Inbox, Notes
  - Projects with pixel mascots and `+3 −3`
  - "Check for updates", Settings ⌘,
- **Workspace column:** Sessions/Explorer/Changes, "Search conversations...", session folders with counts.
- **Transcript footer:** "GPT-5.6-Sol worked for 4m 3s · 6:28" with copy, handoff and second-opinion icons.
- **Composer:** placeholder "Ask, build, / for skills, @ for references...".
- **Bottom:** terminal dock; usage footer "9% 6d 5h".
- **Background:** glass vibrancy over the wallpaper.

### F11. Contribution policy — `M CONTRIBUTING.md`

- Layout: `src/app`, `src/features`, `src/integrations/harness` (core + providers/*), `src/platform/tauri`, `src/shared`, `src-tauri/src` (`:31-40`).
- Gate: `npm run check` runs vitest, tsc, cargo fmt, clippy and cargo test (`:44-48`).
- Provider PRs are closed while new providers are paused (`:50-54`).

### F12. Code vs docs disagreements

- **Quick composer default.** Code enables it (`QUICK_COMPOSER_ENABLED` true, `M settings.ts:723`). CL 0.1.56 says "Enable it in Settings → General". The code wins: it is on by default.
- **"If they're installed and logged in, MonoCode can run them"** (`M README.md:15`). The probe only checks that the binary exists (`M availability.ts:33-53`). Logged-out CLIs look available until the first send fails and opens the sign-in modal.
- **Scheduled automations** (a constraint rather than a doc conflict). They are stored in SQLite, but the scheduler is a 30 s renderer interval in `App.tsx`, so nothing runs while the app is quit. `missedRunGraceMinutes` (`M automations.ts:32-60`) exists to cover that gap. CL 0.1.52 does not mention it.

## Ideas to clone into Pocket

Pocket context:
- pocketd already computes Needs you / Done / Working / Idle from Claude hooks (`P pocketd/internal/daemon/daemon.go:101-118`; `P pocketd/internal/daemon/plugin.go:10-22`).
- The desktop Inbox already lists Needs you plus unseen Done/failed (`P desktop/crates/pocket/src/inbox.rs:28-29`).
- pocketd speaks `codex app-server` (`P pocketd/internal/codex/session.go:83,153-162`).
- Spawn messages accept `Env` (`P pocketd/internal/ops/ops.go:23`).
- The Codex socket path already follows `CODEX_HOME` (`P pocketd/internal/codex/rpc.go:164`).
- Pocket desktop already posts native notifications when an unviewed agent enters Needs you / Failed / Done, dismisses them when you view it, and click focuses the agent (`P desktop/crates/pocket/src/main.rs:314-327,1103-1106`; `P desktop/crates/pocket/src/status.rs:29-31,120-124`). There is no opt-in toggle, mute or category control.
- Pocket has no dock badge, global hotkey or updater: grep in `P desktop/crates` finds none, and the version is `0.0.0` (`P desktop/crates/pocket/Cargo.toml:3`).
- The iPhone app has no `expo-notifications` dependency (`P app/package.json`).

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prereqs |
|---|---|---|---|---|---|---|
| 09-1 | Dock badge = number of Needs-you agents | See pending approvals from any app without opening Pocket | `M src/features/notifications/model/dockBadge.ts:6-15`; `M src-tauri/src/macos.rs:98-112` | desktop `pocket` crate: new; count from `inbox::notes` filtered to Needs you; NSDockTile via objc2 | S | None |
| 09-2 | Native OS notifications for Needs you / Done, click to jump, off by default, per-project mute 1/4/8 h / until resumed, per-category toggles | Walk away while agents run; no spam from noisy projects | `M notificationPreferences.ts:1-23`; `M notifications.ts:12-18`; `M src-tauri/src/notifications.rs`; CL 0.1.48 | desktop: adapt the existing `sync_alerts` / `status::alerts` + `show_system_notification` path (`P desktop/crates/pocket/src/main.rs:314`; `P desktop/crates/pocket/src/status.rs:120`): add opt-in, categories and mutes. pocketd: new per-project mute state so phone and Mac share it. app: needs push (new) | M | APNs for phone |
| 09-3 | Working-agents list ordered needs-input → working (by turn start) → unseen done; activity line + elapsed `4m 3s`; done rows stay until seen | One glance at every agent across projects | `M src/features/sessions/model/liveAgents.ts:23-80`; CL 0.1.21, 0.1.47 | desktop `inbox.rs` / rail: adapt sort, add activity text and elapsed; app: same list | S | Turn start time exposed by pocketd |
| 09-4 | Usage-limit detection + reset countdown ("3:16 AM · in 4h 42m") + opt-in auto-resume at reset + 30 s, holding queued input | Long jobs continue overnight after the 5 h window resets | `M usageLimit.ts:5-26`; `M claude.ts:663,909`; `M codexProtocol.ts:411`; CL 0.2.0 | pocketd: new. Codex from `account/rateLimits/updated` on the existing app-server client; Claude from `StopFailure` / transcript. Resume = write "continue" to the PTY. app + desktop: show the countdown | M | Reliable Claude limit signal (see Q1) |
| 09-5 | Plan-usage meter (5 h / weekly %, reset time) for Claude and Codex | Choose which account or agent to start before hitting a wall | `M src/features/providers/model/rateLimits.ts:3,45-48`; `M src-tauri/src/rate_limits.rs:14-23`; CL 0.1.12 | pocketd: new poller (15 min) + proto field; desktop footer + app chip: new | M | Undocumented Anthropic endpoint and Keychain read (Q2) |
| 09-6 | Quick composer: global ⌘⇧Space floating panel → prompt, project/worktree, provider, mode; Return = start in background, ⌘Return = open | Start an agent from any app in 2 s | `M src-tauri/src/quick_composer.rs:1-4,55`; `M quickComposerShortcut.ts:4`; CL 0.1.56 | desktop: new (gpui global hotkey + borderless panel); pocketd: existing spawn path | L | Global-hotkey support in gpui-kit |
| 09-7 | Session reminders: presets 1 h, 3 h, evening 18:00, tomorrow 09:00, next Mon 09:00 → notification + resurfaces in Inbox | Park a Done agent and come back later | `M sessionReminders.ts:7-33`; `M src-tauri/src/reminders.rs`; CL 0.1.44 | pocketd: new store + timer (daemon is always on, unlike MonoCode's renderer); app + desktop: menu item | M | 09-2 |
| 09-8 | Named accounts per provider via `CLAUDE_CONFIG_DIR` / `CODEX_HOME`, chosen per project | Separate work and personal subscriptions; spread quota | `M src-tauri/src/harness.rs:681,688`; `M providerAccounts.ts:8,23`; CL 0.1.50 | pocketd: adapt; `ops.Msg.Env` exists, the Codex socket already follows `CODEX_HOME`; desktop/app: account picker | M | Hook plugin install per config dir |
| 09-9 | CLI availability probe with login-shell PATH, about 100 candidate paths, 30 s TTL, disabled provider + install hint and exact login command | First run works without docs; clear "not found" vs "not logged in" | `M src/integrations/harness/core/availability.ts:33-70`; `M README.md:19-30` | pocketd: adapt `terminal.LookPath` (`P pocketd/internal/terminal/terminal.go:90`) into a status endpoint; desktop/app: empty-state copy. Improve on MonoCode by also probing auth | S | None |
| 09-10 | CLI update notice: compare with the npm registry, run the CLI's own updater (`claude update`, `codex update`, `opencode upgrade`) | Stay current without a terminal | `M src-tauri/src/harness_updates.rs:8-35` | pocketd: new check; desktop/app: banner | S | None |
| 09-11 | Pocket self-update + "What's new" from the CHANGELOG section; release gate requires matching versions and a CHANGELOG entry | Ship fast without users reinstalling | `M .github/workflows/release.yml:42-65,234-265,380-393`; `M src/app/model/updater.ts:39-112`; CL 0.1.23 | desktop: new (Sparkle-style feed, signed); CI: new | M | Developer ID signing and notarization |
| 09-12 | Navigation keybindings: ⌘⇧↑/↓ across sessions, ⌘⇧←/→ across projects, ⌘1–9 tabs, ⌘⌥arrows between panes; record/disable/reset overrides | Keyboard-only triage | `M settings.ts:938-1030`; CL 0.3.0, 0.1.34 | desktop: adapt; Pocket already binds cmd-k/p/n/j/\\/./t (`P desktop/crates/pocket/src/main.rs:1047-1056`) | S | Keep ⌘J = NextWaiting (conflicts with MonoCode's ⌘J dock) |
| 09-13 | Scheduled + event automations (hourly/daily/weekdays/weekly, Run now, missed-run grace, worktree mode) with templates like "Watch failing checks" | Recurring agent work without attention | `M src/features/automations/model/automations.ts:8-60`; `automationTemplates.ts:52-337`; `M src/app/App.tsx:7286` | pocketd: new, run by the daemon so it works with the UI closed; app: list + Run now | L | Agent launch with an initial prompt in a PTY |
| 09-14 | Agent-facing control CLI (`/operator`-style): list/read/send sessions, start a session beside me, create a worktree | Agents can fan out work and report back | `M README.md:46-55`; `M src-tauri/src/control.rs`; `M src/features/agent-app/model/agentApp.ts:284` | pocketd: adapt the existing CLI/socket with a scoped per-session token | L | Auth scoping; loop guards |
| 09-15 | Session organization: pin, folders, archive (⌘⇧A), filters by status and provider | Scales past about 20 sessions | CL 0.1.7, 0.1.25, 0.1.28; `M sessionFilters.ts:9-61` | pocketd store: new fields; desktop/app: adapt lists | M | None |
| 09-16 | Draft worktree: the first prompt creates a worktree with an LLM-named branch | Parallel agents without choosing branch names | CL 0.1.52; `M core/registry.ts` `generateBranchName` | pocketd + desktop: adapt the existing worktree flow; branch name via a one-shot `claude -p` | M | Headless helper call |
| 09-17 | CI repair: PR check failures + job log → prompt an agent in the PR worktree | One click from a red CI to a fix | `M src/features/inbox/model/ciRepair.ts`, `githubPrChecks.ts`; CL 0.1.56 | new pocketd `gh` integration; desktop/app button | L | GitHub auth |
| 09-18 | Remote hosts over SSH: version-matched host installer, OS service, per-device revocable tokens | Drive agents on a remote dev box | `M docs/remote-access.md:3-105` | pocketd distribution + desktop: new | XL | Phone-to-Mac model first; don't mix axes |
| 09-19 | Handoff / Second opinion / BTW into a split pane with a recap | Cross-check with another model | CL 0.1.6, 0.1.17, 0.2.0 | Not portable. Needs structured transcripts; Pocket's timeline is read-only from the PTY. At most "open another agent beside with the last prompt pasted" | M | None |

## UI/UX spec to copy

**Status and attention**
- Dock badge shows only the Needs-you count. It does not count Done (`M dockBadge.ts:9-11`). Updates are deduped against the last value.
- Live-agent rows:
  - Sort: needs input first, then working ascending by start, then done.
  - Activity text is one of: "Done", the question title, or the current tool title.
  - Elapsed format: `12s` / `4m 3s` / `1h 5m` (`M liveAgents.ts:38-47,74-81`).
  - Show the card only when ≥2 agents are in flight (CL 0.1.21).
- Unseen Done shows a teal check in the title-bar tab (`M src/app/shell/TitleBar.tsx:239`, `text-teal-400`).
- The first Inbox snapshot never adds badges (`M inboxSeen.ts:161`). Pocket should apply the same rule to pre-existing Done agents at daemon start.

**Notifications**
- Categories: "Agent finished", "Agent approvals and questions", "Reminders", plus PR and issue categories.
- Mute choices: 1 h, 4 h, 8 h, custom date, or "until resumed". Resuming suppresses events that happened while muted (`M notificationPreferences.ts:1-23`).
- Off by default. Enabling triggers the OS permission prompt (`M notifications.ts:12`).

**Quotas**
- Usage-limit copy: "3:16 AM · in 4h 42m" on the same day, "Sep 26, 3:16 AM · in 1d 4h" otherwise. Auto-resume waits 30 s past the reset (`M usageLimit.ts:5-17`).
- Context meter: ring turns amber at ≥75% and red at ≥90%. Hover shows "69% context used" and "176K / 256K tokens" (CL 0.1.1; `M contextUsage.ts:42`).
- Footer usage chip: "9% 6d 5h", meaning percent used and time to reset (README hero).

**Runtime modes** (label hints, `M session.ts:375-388`)
- Supervised: "Ask before commands and file changes."
- Auto-accept edits: "Auto-approve edits, ask before other actions."
- Auto: "An AI reviewer can approve or deny actions."
- Full access: "Allow commands, edits, and supported MCP confirmations in non-plan turns without prompts."
- Default is Supervised.

**First run**
- Missing CLI: "`<name> not found<how>. Install it, or restart MonoCode if it is already installed.`" (`M availability.ts:66-70`).
- Sign-in modal: "Authentication required" / "Sign in to continue using X." (`M ProviderSignInDialog.tsx:44-47`).
- Show the exact login command for each CLI (`M README.md:19-30`).

**Composer** (screenshots)
- Placeholder: "Ask, build, / for skills, @ for references...".
- Chip order: `+` · model · effort · speed · mode · send.
- The cwd and branch row sits above the input.

**Session card** (screenshot)
- Line 1: model and relative age.
- Line 2: bold title.
- Line 3: `repo/branch` and `+N −M`.
- Provider icon on the right.

**Window**
- 1280×800, minimum 800×520.
- Overlay title bar with hidden title, transparent, background `#171717`, window shadow (`M src-tauri/tauri.conf.json`).
- The overlay title bar is about 28 pt while the tab strip is 40 px (`M src-tauri/src/macos.rs`).

**Quick composer**
- ⌘⇧Space toggles it. Return starts in the background; ⌘Return starts and opens. The panel does not bring the main window forward (`M quick_composer.rs:1-4`).

**Keybindings** (`M settings.ts:938-1030`)

| Key | Action |
|---|---|
| ⌘, | Settings |
| ⌘K | Search |
| ⌘P | Go to File |
| ⌘⇧P | Palette |
| ⌘T | New tab |
| ⌘⌥T | Close others |
| ⌘⇧W | Close all |
| ⌘1–8, ⌘9 | Tab N, last tab |
| ⌘[ / ⌘] | Back / forward |
| ⌘⇧A | Archive |
| ⌘⇧↑↓ | Sessions |
| ⌘↑↓ | Sessions in tab |
| ⌘⇧←→ | Projects |
| ⌘D / ⌘⇧D | Split right / split down |
| ⌘⌥arrows | Focus pane |
| ⌘J | Terminal dock |
| ⌘B / ⌘⇧B | Sidebars |
| ⌘. | Model |

Pocket conflicts: ⌘J, ⌘. and ⌘, have different meanings in Pocket (`P desktop/crates/pocket/src/main.rs:1050-1055`).

## Open questions / risks

- **Q1. Claude usage-limit signal in PTY mode.**
  - MonoCode reads a stream-json rate-limit event (`M claude.ts:663`). Pocket only gets hooks and the transcript.
  - Does Claude Code's `StopFailure` hook input carry the error type and reset time? Pocket currently maps it to `TurnEnded(true)` only (`P pocketd/internal/daemon/daemon.go:114-115`).
  - This needs verification before 09-4.
- **Q2. Claude plan-usage endpoint.**
  - `api.anthropic.com/api/oauth/usage` with beta `oauth-2025-04-20` is undocumented. It needs the Keychain OAuth token and a spoofed `claude-code/2.1.0` user agent (`M rate_limits.rs:14-23`).
  - Risks: ToS and breakage. Codex is safer because its limits come from app-server notifications on the connection Pocket already holds; pocketd does not handle `account/rateLimits/updated` today (no match in `P pocketd`).
- **Q3. Auto-resume in a PTY means typing into the user's terminal.**
  - It must not fire if the user has started typing, or while the agent is not Idle.
  - MonoCode guards with `session.busy` (`M usageLimit.ts:22`). Pocket needs an equivalent input-idle check.
- **Q4. Where state lives.** MonoCode keeps notification mutes in renderer localStorage (`monocode.projectNotifications.v1`, `M notificationPreferences.ts:26`) and runs automations in the renderer (§F12). Pocket should keep mutes, reminders and automations in pocketd so phone and Mac agree and they work with the UI closed. This is a design choice to confirm.
- **Q5. Pace versus polish.** MonoCode ships about 1.5 releases per day with about 180 open issues and PRs and an 11k-line `App.tsx`. Its feature count is not evidence of how much users rely on each feature. The only usage data is GitHub download counts (tens per asset on v0.5.0).
- **Q6. Remote access overlap (09-18).** MonoCode's host model (desktop → SSH → headless host) competes with Pocket's pocketd-plus-phone model. Deciding whether Pocket targets remote Linux boxes is a product call, not a clone.
- **Q7. Structured versus PTY.** Handoff, Second opinion, BTW, plan cards, edit or rewind last turn, and checkpoints all rely on MonoCode owning the transcript and protocol (`M core/registry.ts:47-101`). Pocket would need Claude/Codex structured modes beside PTYs to match them, which is an architecture fork.
- **Q8. Notifications on the phone** (09-2) need APNs and a relay. pocketd is local-only today, and the app has no notifications dependency. This is scoped separately from the desktop notifications.

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 8.

Confirmed against source: dock badge counts only `sessionNeedsInput` with dedup; notification categories, mute hours `[1,4,8]` and `NOTIFICATIONS_DEFAULT = false`; usage-limit 30 s grace, reset copy and `session.busy` guard; live-agents filter, sort and elapsed format; quick composer doc comment and `Command+Shift+Space` default; availability probe (binary only, 30 s TTL, hint copy); Claude usage endpoint, beta header, user agent and Keychain item; rate-limit windows and 15 min poll; keybinding table (`M settings.ts:938-1030`); provider spawn args for Codex, Grok, OpenCode, Antigravity, Pi, omp, fx, Hermes, Cursor; `CLAUDE_CONFIG_DIR` / `CODEX_HOME`; 9 event triggers and 14 templates; remote-access numbers; Pocket keybindings, hook mapping, `ops.Msg.Env`, `codex.Sock` following `CODEX_HOME`, `terminal.LookPath`, no dock badge / global hotkey / updater, no `expo-notifications`; 63 releases and 1742 stars (`gh api`).

Corrections:
- Pocket desktop already has native OS notifications (`P desktop/crates/pocket/src/main.rs:314-327`, `status.rs:120-124`). Fixed the TL;DR, the Pocket context bullet and the 09-2 mapping/prereqs (adapt, not new).
- Orchestration workers no longer share the lead checkout; since 0.1.52 each gets a worktree seeded from it (CL L242, `M src/app/App.tsx:8663`). Worker restrictions restated from `:174`.
- Claude spawn flags were incomplete: added `--setting-sources`, `--allow-dangerously-skip-permissions` (bypass mode) and `--max-turns`.
- Scheduler citation path was `src/App.tsx`; the file is `M src/app/App.tsx:7286`. Fixed in F6.4 and 09-13.
- Login-shell PATH / version-manager cite pointed at the TTL comment; added `M src-tauri/src/harness.rs:2355,2518`.
- "182 open issues" is GitHub's `open_issues_count`, which includes PRs (181 at check time). Reworded in TL;DR and Q5.
- Quick composer is macOS only (CL 0.1.56; `IS_MAC` gate). Added to TL;DR and F6.3.
- Q2 said Pocket already receives Codex rate-limit notifications; pocketd has no handler for `account/rateLimits/updated`. Q4 "localStorage per window" reworded to renderer localStorage with the key cited.
