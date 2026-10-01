# 17 — Starting work: new-session composer, target pickers and agent/model setup (desktop)

- Date: 2026-09-30
- Sources:
  - Zeron `zeronsh/comet` @ ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (MIT), https://zeron.sh
  - MonoCode `hardbeat920/monocode` @ cdc1441dc51e3709cd843e5c316608a123f323c6 (MIT), https://usemono.dev
  - Pocket `orchestrator-research` worktree @ b9d14a1
  - Local CLIs: `claude` 2.1.285, `codex-cli` 0.159.0
  - Prior reports 03, 08, 10, 12, 13, 14 in this folder

**Legend.**
- `Z path:L` is Zeron, relative to `/Users/mingo/tmp/orchestrators/zeron`.
- `M path:L` is MonoCode, relative to `/Users/mingo/tmp/orchestrators/monocode`.
- `P path:L` is Pocket, relative to the worktree root at b9d14a1. `P docs/orchestrators/research/NN-*.md:L` points at a prior report.
- `$ cmd` is output of a command run locally on 2026-09-30 against the CLI versions above.
- Where code and docs or screenshots disagree, the code is taken as truth and the disagreement is listed in F12.
- IDs like `03-7`, `10-4` and `08 F10` refer to ideas or findings in earlier reports. They are cited here, not redone.

## TL;DR

- **Pocket's Codex "Auto-edit" never starts.**
  - It passes `--full-auto` (P packages/desktop/crates/pocket/src/forms.rs:412), which codex-cli 0.159.0 rejects: `error: unexpected argument '--full-auto' found` ($ `codex --full-auto --version`).
  - Because `agent_op` hands control back to the login shell (P packages/desktop/crates/daemon/src/daemon.rs:98-112), the user just sees an error and a shell prompt.
- **Pocket's other two modes misbehave too.**
  - Codex "Plan only" is `-s read-only`, which is not a plan mode (forms.rs:413).
  - "Ask" sends no flags (forms.rs:409), so the user's own `defaultMode` or `config.toml` wins. That is exactly the silent-`auto` risk 10-4 warns about.
- **Both references put the composer first.** The empty canvas is the composer. Target chips (project, checkout, base) sit around it. Agent·model·effort and access chips sit in its action row. Picks stick between sessions. Pocket instead uses a modal sheet with one combined agent+permission chip and remembers nothing on disk.
- **Zeron:**
  - The hero composer is centred at `(viewport − h)·0.5 + 8` and glides to the dock in 0.420 s on first send (Z crates/ui/src/composer_dock.rs:341,468-470).
  - Device and project chips sit top-right above the pill. Checkout and ref chips sit in a 24 px slot below it.
  - There is no permission picker: the sandbox is fixed at `WorkspaceWrite` (Z crates/ui/src/pickers.rs:152).
  - Picks live in `composer-defaults.json`.
- **MonoCode:**
  - A new session is a new blank tab that inherits the active session's cwd and access mode (M src/app/App.tsx:2250-2262).
  - The headline reads "What should we work on in {project}?".
  - Folder, workspace and branch pickers sit in the composer's top bar. Model and access pickers sit in its bottom bar.
  - There are 4 access modes. Plan is a separate per-turn toggle.
- **Recommendation: replace the modal with a draft "New session" canvas in the main pane.**
  - P1 (terminal mode, now): no motion, matching Pocket's stated no-motion stance for this surface (P packages/desktop/crates/pocket/src/overlay.rs:321).
  - P2 (headless, after 03-2/10-1/10-12): add the hero→dock glide.
- **One access picker: Ask / Auto-accept edits / Auto / Full access, plus a Plan toggle.**
  - Both axes are always sent explicitly (10-4).
  - Terminal flags were verified against the installed CLIs (F11).
  - Deviation from 10-4: the Codex CLI rejects `-a untrusted` ($ `codex -a untrusted --version`), so Ask becomes `-s read-only -a on-request`.
- **Mapping in three stages:**
  - A: fix the flags in the desktop.
  - B: add a `LaunchSpec` to `packages/protocol` and build argv in pocketd. This also unlocks starting a terminal session from the phone via `agent.create` (10-3, 08 F10) before headless exists.
  - C: the headless drivers consume the same spec.
- **Remember** provider, model and effort per provider, and access (never Full access). Do not remember checkout kind: ⌘N and ⌘⇧N already encode it (P packages/desktop/crates/pocket/src/main.rs:1064,1069), and Zeron doesn't persist it either.
- **Missing states** in Pocket today: CLI not installed, pocketd offline (writes are silently dropped, daemon.rs:190), and worktree errors after the sheet has already closed (forms.rs:449,459). Not-logged-in is left to the CLI's TUI in terminal mode and only matters for headless. The model list waits on 03-7/10-11.

## Findings

### F1. Pocket today: the modal sheet (baseline)

- **Entry points**
  - ⌘N `StartSession` and ⌘⇧N `NewWorktree` (P packages/desktop/crates/pocket/src/main.rs:1064,1069).
  - The rail button `nav-compose` (P packages/desktop/crates/pocket/src/view.rs:521).
  - The column "+" button (view.rs:628-629).
  - The palette row "New session in {project}" with keys "⌘ N", which calls `Pick::New` and opens `Overlay::NewSession` (P packages/desktop/crates/pocket/src/overlay.rs:126,144).
  - The blank page's "New session ⌘N" button. Its copy is "Add a project to begin." or "Pick a session, or start a new one." (view.rs:697-714).
- **Surface**
  - A modal sheet at top 110, w640, padding 17/17/15, gap 10 (P packages/desktop/crates/pocket/src/forms.rs:641-643).
  - Entrance motion `None`, backdrop alpha `0x2e`. The comment says "The palette and new-session sheet open from shortcuts many times a day; motion would only slow them" (overlay.rs:321-324).
  - Esc closes the picker first, then menus, then the overlay (view.rs:1114-1125).
- **Anatomy** (forms.rs:544-645)
  - Header: "New session" or "New worktree", 16 px bold, with an 18 px repo tile and the repo name (L555).
  - Agent chip: "{Provider} {model} · auto-edit|plan only" (L558-562).
  - Composer: r14, white, ring (L601).
  - Textarea: h105, 15/23.25, placeholder "Describe what the agent should do…", rows 4 (L205, L605).
  - Send button: 32 px round.
  - Footer: "New branch {name} from {base}" or "In {branch|~path}" (L622-629), then "⌘↵ to start · esc to cancel" (L640). Secondary Enter (⌘↵) starts (L209, L219).
- **Chip and menu tokens**
  - `chip`: h30, pl10, pr8, gap7, r8, FILL_2, becoming FILL_3 when open or hovered (forms.rs:138-153).
  - `pick_head`: 11.5 semibold TEXT_3, pt8 px8 pb4 (L155-157).
  - `pick_row`: h34, px8, gap9, r6, 13 px medium, meta 12 px TEXT_4, 14 px check in a 16 px column, selected row FILL_2 (L159-177).
  - `picker_menu`: p5, r10, max_h 360, closes on mouse-down outside (L179-193).
- **Agent menu** (w260)
  - One head per provider, each with a single row: the model hint, or "Default model".
  - Then a "Permissions" head with rows Ask / Auto-edit / Plan only (forms.rs:491-517).
  - The "model hint" is just the latest agent's model label (view.rs:926-929). It is not a catalog.
- **Branch menu** (w300)
  - Heads "Default" and "Recent". Rows show a branch icon, mono 12.5 text and `ago_long` meta (forms.rs:520-541).
  - At most 20 branches (L345). The default branch is rotated to the front (L362).
- **Worktree naming**
  - Slug from the first 4 alphanumeric words of the prompt, otherwise `adjective-noun` from 8×8 word lists (forms.rs:71-94).
  - Errors: "A worktree or branch with this name already exists" and "Use letters, digits, - _ or ." (L111, L113).
  - Base defaults to `cfg.base`, then `main`, then `master`, then the current branch (L124).
  - `copy_env` and `run_setup` default on when configured, with no UI toggle (L339-340).
- **Defaults and memory**
  - `provider: "claude"` survives reopening only in memory (forms.rs:234).
  - `perm` resets to `Ask` on every open (L235, L321).
  - `open()` resets the form (view.rs:136-146).
  - On disk, `desktop.json` stores only `projects`, `repos` and `collapsed` (P packages/desktop/crates/store/src/store.rs:8-28).
- **Start** (forms.rs:402-459)
  - `session_ready` needs a cwd. For a worktree it also needs a repo, loaded branches and a valid name (L393-400).
  - Current checkout: `agent_op(argv)` with `Intent::Tab` (L419).
  - Worktree: `git::add_worktree` runs in the background. Copy failures are ignored with `.ok()` (L436). Then `agent_op` or `setup_op` runs (L447-448).
  - The overlay closes synchronously (L459). A git error therefore lands in the app-level `error` after the prompt is gone (L449).
- **Spawn**
  - argv goes to `$SHELL -l -c` as arguments, and the script then runs `exec $SHELL -l` (P packages/desktop/crates/daemon/src/daemon.rs:98-112). An agent that fails to start leaves a shell prompt, and the UI gets no signal.
  - Send ignores write errors with `let _ = writeln!` (daemon.rs:190). A disconnect emits "pocketd disconnected" (L183).
- **Quick agent tabs**
  - The tab menu offers "New tab in {branch}", Claude Code and Codex rows with a model hint, and "New shell ⌘T" (view.rs:950-980).
  - `new_agent_tab` spawns a bare `[provider]` (main.rs:595-599), so no picks apply.
- **Phone**
  - The phone can't create sessions. Its empty state says "Start one on your Mac: pocketd run claude" (P packages/app/src/screens/AgentsScreen.tsx:24).
  - `agent.create` gets "Malformed message" (P packages/pocketd/internal/wsserver/wsserver_test.go:157-160).

### F2. Pocket permission flags vs installed CLIs

```rust
// P packages/desktop/crates/pocket/src/forms.rs:408-414
let mut args: Vec<String> = match (f.provider, f.perm) {
    (_, Perm::Ask) => vec![],
    ("claude", Perm::AutoEdit) => vec!["--permission-mode".into(), "acceptEdits".into()],
    ("claude", Perm::Plan) => vec!["--permission-mode".into(), "plan".into()],
    (_, Perm::AutoEdit) => vec!["--full-auto".into()],
    (_, Perm::Plan) => vec!["-s".into(), "read-only".into()],
};
```

| Pocket mode | claude 2.1.285 | codex 0.159.0 | Verdict |
|---|---|---|---|
| Ask | no flag | no flag | Inherits the user's `permissions.defaultMode` or `config.toml`. Per 10-4 this can silently run as `auto` (M src/integrations/harness/providers/claude/claudeProtocol.ts:105-126) |
| Auto-edit | `--permission-mode acceptEdits`: works | `--full-auto`: rejected with "error: unexpected argument '--full-auto' found" ($ `codex --full-auto --version`) | Codex is broken |
| Plan only | `--permission-mode plan`: works | `-s read-only` | Codex still runs as a normal agent that asks for escalations. `codex --help` has no plan flag. Codex plan is app-server `collaborationMode` (10-16) |

### F3. Zeron: new-session surface

- **Entry** (Z crates/ui/src/shell/tabs.rs:222-275)
  - `open_new_session` sets the route to Chat, focuses the composer and closes the canvas terminal drawer.
  - It picks the target: the sidebar's `space_filter`; otherwise, if `defaults.no_project` is set, no space plus the remembered device.
  - Then it calls `select_chat(None)`.
  - The canvas titlebar shows nothing. A "New session" header over the empty canvas was judged noise (L283-285).
- **Shortcuts**
  - NewSession `mod-n`, NewProject `mod-shift-n`, OpenModelPicker `mod-/`, ToggleTerminal `mod-j` (Z crates/ui/src/settings.rs:1104-1109).
  - The palette row is "New chat", with a shortcut badge (Z crates/ui/src/shell/command_palette.rs:41-51).
- **Layout**
  - The hero pill is centred. Its Y is `(viewport − height)·0.5 + 8` (Z crates/ui/src/composer_dock.rs:341,606).
  - Max width is 768 (Z crates/ui/src/composer.rs:73). Radius is 26 (L63).
  - The textarea runs from 76 to 260 px, with 20 px vertical padding (L50-56).
  - Input text is 14/22.75 (L99-100). The actions row is 2+32+8 (L58-59).
  - Placeholder: "Do anything…" (composer.rs:5469,7438,8517).
- **Target row (above the pill)** (composer.rs:9644-9660)
  - Device and project chips are absolutely positioned at top −28, inset `SPACE_LG(16)+10` left and right, `justify_end`. The comment reads "destination at the top-right".
  - The row is 20 px high (L81).
  - Opacity is bound to `new_thread_chrome_opacity`, so the row fades out as the pill docks.
- **Bottom slot (below the pill)** (composer.rs:9680-9760)
  - h24 (L84). Git selectors (checkout kind, ref) sit at px10 and fade with the same opacity.
  - When docked, the slot is replaced by the session footer plus account usage.
- **Action row inside the pill**: the agent/model/effort chip, attach, and send. The screenshot shows "Mock 1 Medium" (Z docs/screenshots/projectless/restored-selection.png).
- **Canvas background** is a setting: None / Dither / Ascii / Halftone / Scanlines (Z crates/ui/src/settings.rs:79-86).
- **Send path**
  - The new chat's cwd is the space path, otherwise "~".
  - A fresh worktree rides the queued Run as a `WorktreeSpec` instead of a blocking RPC. The RPC had no timeout, and a lost relay frame once wedged the send (2026-08-18) (composer.rs:8028-8060).

### F4. Zeron: picker anatomy

- **Chips**
  - Trigger chip: h32, max_w 248, px 6/10, r8, 12 px medium, 150 ms hover fade (Z crates/ui/src/pickers.rs:2689-2815).
  - Footer chip: h20, max_w 280, gap 6, px 8, r6, 12 px text. Text is text_muted at 0.7, brightening to text at 0.8 on hover. Icon 12, chevron 12 at 0.5 opacity (L2818-2877).
- **Popover widths**: space 280, device 224, checkout 224, branch 320 (pickers.rs:2907-3049). Config card 252 (L482). Model card 304 (L5427).
- **Menu placement**
  - Menus open below when at least 180 px is free, otherwise above (pickers.rs:2635-2686).
  - Height = 82 + 216 + tray, with tray = `min(n·32+7, 236)` (L5201-5222).
  - Setting rows are h30. Submenus flip left when +244 would overflow.
- **Device menu**
  - Rows show "This device" (L2928) and a "You" tag (L2327-2335).
  - Empty filter result: "No devices match." (L2298).
- **Project menu**
  - Header "Search projects…" (screenshot).
  - Empty states: "No projects on this device." / "No projects match." (L2380-2382).
  - Footer rows "New project…" (L2459) and "Don't work in a project" (L2439).
  - Chip labels "No project" (L2936) and "No project selected" (L3384).
- **Checkout model**
  - The doc comment says "t3code's env-mode: `local | worktree`". "Current worktree" is deliberately not a third mode (Z crates/proto/src/view.rs:540).
  - `CheckoutKind` is `Local` (the default) or `NewWorktree` (L547).
  - The plan is `CurrentCheckout{branch}`, `ReuseWorktree{path,branch}`, or `NewWorktree{base}` (L557-571).
  - Labels are "New worktree", "Current worktree" (the picked ref has a worktree path), or "Current checkout" (L586).
  - The engine mints `zeron/<adjective>-<noun>` branch names, with 50 attempts (Z crates/engine/src/repos.rs:1105-1136).
- **Ref menu**
  - Search placeholder "Search refs…" (L1207).
  - Title "Select ref"; label "From {name}" (L2133-2134).
  - Tags "current" and "worktree" (L3434-3436). Transient label "switching…" (L3460).
  - Empty state "No refs found." (L3412). Truncation notice "Showing {shown} of {total} refs" (L3510).
  - In Local mode, picking a ref runs a real `git checkout` (`switch_draft_ref`, L1562).
- **Agent/model card**
  - Tabs row h40 with 32×32 tabs and a 2 px accent underline. Search row h40, 13 px, "Search models…" (L1228, L3582-3918).
  - Model row: px8, py5/6, gap 10, label 12.5 medium, 11 px attribution, 22×22 star button. A ⌘N hint appears on the first 9 rows (L3921-4226).
  - The catalog default gets a "Default" badge (L4911).
  - A saved model that is missing from the catalog stays visible as "Selected in this chat; absent from the current model list" (L1961).
  - Empty states: "No models found" (L3810) and "No starred models yet — hit a row's star" (L3814).
  - There are no "Default" placeholder rows (L158).
  - Default reasoning is High, else Medium; the code comment says "user-corrected" (L179-190).
  - Offered harnesses are `installed && enabled`, with the mock harness hidden (L5146-5199).
- **Keys** (pickers.rs:2509-2620)
  - Esc closes. Up/Down wrap.
  - Right opens a setting submenu; Left or Esc closes it.
  - ⌘1-9 jump to a row. Enter or ⌘Enter picks.
- **Catalog freshness**
  - Catalogs are `Loadable` Idle/Loading/Ready/Error, and every open forces a refresh (pickers.rs:1268-1357).
  - Discovery reliability (live Claude `initialize` with a curated fallback, typed failures, last-good catalog kept on disk) is covered by 03-7/10-11 (Z docs/research/model-discovery-reliability.md). Not redone here.
- **Reuse**: Settings → General → Thread naming reuses the model picker. Copy: "Each thread is named by its own agent." / "A small model keeps titles fast and cheap." (Z crates/ui/src/settings/thread_naming.rs).

### F5. Zeron: states

- **No agents**
  - A new chat's send is blocked only by `no_agents_available` (Z crates/ui/src/composer.rs:7486-7502; Z crates/ui/src/pickers.rs:1019).
  - The chip reads "No agents available" (pickers.rs:5371).
  - The card's empty state reads "No agents available" / "Enable an installed agent in Settings → Providers, or install an agent CLI." (L3647-3655).
- **Missing CLI**
  - This state lives only in Settings → Providers: state is per device, a missing CLI is dimmed and never enabled, and the last enabled agent can't be turned off (Z crates/ui/src/settings/harnesses.rs:1-19).
  - Copy:
    - "{cli} CLI not installed — turn it off or install it" when enabled.
    - "Install the {cli} CLI to enable" otherwise.
    - ". Install with `{command}`" is appended when a manual install is needed, e.g. `npm install -g @openai/codex`.
    - "Installing {name}…" while installing.
    - Sources: L64-97, L1378.
  - The composer simply omits the missing agent (pickers.rs:5146-5199).
- **Not logged in**
  - An `AuthRequired` failure is detected from strings like "not logged in", 401/403 and "api key" (Z crates/harness/src/catalog_failure.rs:55-71,105-108).
  - The picker still shows only a generic error row with a "Retry" button (pickers.rs:3304-3340). There is no composer-level login state.
  - Accounts settings copy:
    - "No {name} login detected on this device — sign in with “{cli}” or add an account." (Z crates/ui/src/settings/accounts.rs:2026-2037).
    - "Sign in to {label} for {provider}" (L451-452).
    - "Waiting for you to finish in the browser…" (L463).
    - "Couldn't start the sign-in: {error}" (L895).
  - "Not signed in" in the shell refers to the relay account, not the harness (Z crates/ui/src/shell.rs:1642).
- **Offline**
  - Offline and loading never block send (composer.rs:7486-7502).
  - With no engine, the send fails with "Engine not connected" (L7600-7610; also accounts.rs:698).
  - An unreachable session device shows "The session's device is unreachable" (composer.rs:5241).

### F6. Zeron: what is remembered

- **Storage**
  - `composer-defaults.json` mirrors web localStorage `zeron.composer.defaults:v1` and is written atomically (Z crates/ui/src/settings/composer.rs:2,20).
  - Fields: `harness`, `model_by_harness`, `reasoning` (global), `model_options_by_model`, `model_labels`, `device`, `project`, `no_project`, `favorites` (L46-71).
- **Target**: `remember_target` stores the device and project on pick (Z crates/ui/src/pickers.rs:2223).
- **Not remembered**: checkout kind. `CheckoutKind` defaults to `Local` (Z crates/proto/src/view.rs:547), and the field list has no checkout entry. Permission is not remembered either, because none exists (pickers.rs:152).

### F7. Zeron: motion

- The hero→dock glide lasts 0.420 s when docking and 0.470 s when returning, times `speed_scale` (Z crates/ui/src/composer_dock.rs:468-470). It uses a critically damped spring (the `Glide` struct at L19). `hero_limit` is at L572.
- The target and git chrome fade with `new_thread_chrome_opacity` (composer.rs:9644-9760).

### F8. MonoCode: new-session surface

- **New session = new tab**
  - `onNew` opens a new blank session in a new tab, takes the cwd from the active session, inherits its `runtimeMode` and focuses the composer (M src/app/App.tsx:2250-2262).
  - Defaults come from `active ?? sessions[0]` (L1422).
  - Workspace mode and base are editable only while the session is blank (L5014-5060).
- **Empty canvas** (M src/features/sessions/ui/EmptySession.tsx:43-56)
  - Headline: "What should we work on in ${project}?" or "What should we work on?".
  - Layout: `max-w-4xl`, `px-1.5 py-12`, justify-center. The h1 is `truncate text-lg mb-4 px-2.5`.
  - Arcade `TerminalGridBackground` (L52).
- **Dock motion**: 480 ms, `cubic-bezier(0.22, 1, 0.36, 1)`, 1500 ms launch window. Reduced motion skips it. Centres are aligned so the composer drops straight down (M src/features/sessions/ui/useComposerDockMotion.ts:4-6,34-54).
- **Composer top bar** (M src/features/sessions/ui/Composer.tsx:1955-2023)
  - Inside the box, `px-3 pt-2.5 gap-2.5`.
  - Holds CwdPicker, WorkspacePicker (mode + base), and BranchPicker (only in "current" mode), with a ContextMeter on the right.
- **Composer bottom toolbar**
  - ModelPicker (+ ModelControlPills) and AccessPicker. The AccessPicker is hidden in fx or compact layouts (Composer.tsx:2377-2410).
  - Box style: `rounded-lg border-content/10 bg-content/3 backdrop-blur-sm`.
- **Plan**
  - The "+" menu has "Plan mode" / "Review a plan before building" (Composer.tsx:2179-2201).
  - While on, a yellow "Plan ×" pill shows, titled "Turn off Plan mode" (L2329-2343).
  - The intent `plan` is sent per turn (L1503-1508).
- **WorkspacePicker** (M src/features/workspace/ui/WorkspacePicker.tsx)
  - `WorkspaceMode = "current" | "worktree"` (M src/features/sessions/model/session.ts:390).
  - Labels: "Worktree" / "Current checkout" (L137) and "New worktree" / "Current checkout" (L256).
  - Menu: w240, header "Workspace", rows Current checkout (Folder icon) and New worktree (FolderTree icon), then "Existing worktree…" and "Worktree settings" (L296-386).
  - Existing-worktrees submenu: w300, "Loading worktrees…" / "No existing worktrees" (L417, L449).
  - Base picker: w280, 12 px search "Search base branches…", "No matching branches", label "From ${selected}" (L540-631).
  - Shortcut ⌘⇧G, "Composer: Toggle Workspace" (L35, L258-263).
- **CwdPicker** (M src/features/projects/ui/CwdPicker.tsx:45-50,265-370)
  - w288, max_h 360, preview of 5.
  - Sections: "Current project" (13 px name + mono 11 px parent) and "Recent projects" (rows `px-2.5 py-2`, mono 11 px parent `max-w-28` at content/45).
  - Then "More Projects ›" and a footer "New terminal ⌘`".
- **ModelPicker** (M src/features/sessions/ui/ModelPicker.tsx)
  - Widths 250/310/210; 32 px provider tabs with gap 4 (L83-96).
  - Trigger: `h-6.5 max-w-40`, name and effort at 11 px, effort at content/50. Title "… · Recent models: right-click or ⌘." (L631-660).
  - ⌘. is "App: Switch Model" (L448-458).
  - Right-click opens "Recently used models": h-10 rows with a harness icon, 13 px name and 11 px "Harness · Provider"; unavailable rows at content/30 (L870-948).
  - Search placeholder "Search models". Empty states "No favorite models" / "Loading Codex models…" / "No matching models" (L1361-1384).

### F9. MonoCode: access modes

| RuntimeMode | Label | Hint | Claude `--permission-mode` | Codex approvalPolicy / sandbox / reviewer (app-server) |
|---|---|---|---|---|
| `supervised` (default) | Supervised | "Ask before commands and file changes." | `default` (sent explicitly) | `untrusted` / `read-only` / user |
| `auto-accept-edits` | Auto-accept edits | "Auto-approve edits, ask before other actions." | `acceptEdits` | `on-request` / `workspace-write` / user |
| `auto` | Auto | "An AI reviewer can approve or deny actions." | `auto` | `on-request` / `workspace-write` / `auto_review` |
| `full-access` | Full access | "Allow commands, edits, and supported MCP confirmations in non-plan turns without prompts." | `bypassPermissions` + `--allow-dangerously-skip-permissions` | `on-request` / `danger-full-access` / user. `never` would reject escalations before the client handler sees them |

Sources: M src/features/sessions/model/session.ts:355-388 (default at L373); M src/integrations/harness/providers/claude/claudeProtocol.ts:105-126,281-287; M src/integrations/harness/providers/codex/codexProtocol.ts:62-95. Claude `manual` needs CLI 2.1.200 (claudeProtocol.ts:105-126).

- **AccessPicker** (M src/features/sessions/ui/AccessPicker.tsx)
  - Menu w288 (L29). Icons Lock / Pencil / Sparkles / Shield (L31-36).
  - Trigger: `h-6.5 max-w-52 gap-1 rounded-md px-1.5`, size-3.5 icon, 11 px label. Only the Full access icon is `text-amber-400/90`; the label keeps the normal colour (L103-115, L154).
  - Popover opens on top with p-1 (L122-133).
  - Rows: `items-start gap-2.5 rounded-lg px-2 py-2`, 13 px medium label (leading-5), 11 px hint (leading-4) at content/50 (L148-164).
  - Footnote while busy: "Access changes apply to the next turn. Stop and resend to apply them now." (L169-173).
  - Keys: ArrowUp/Down clamp without wrapping; Enter picks (L68-84).

### F10. MonoCode: states and memory

- **Availability** (M src/integrations/harness/core/availability.ts)
  - CLI names and install commands are listed at L36-53. Probe TTL is 30 s (`PROBE_TTL_MS = 30_000`, L64, L77).
  - Copy: "${name} not found${how}. Install it, or restart MonoCode if it is already installed." (L66-70).
- **Sign-in** (M src/features/sessions/ui/ProviderSignInDialog.tsx)
  - Dialog: "Authentication required" / "Sign in to continue using ${HARNESS_TITLE}.", action "Continue", size sm (L43-54).
  - Failure: "Could not sign in to X." (L34).
- **Memory** (M src/features/sessions/model/models.ts)
  - `saveDefaultModel` writes to localStorage (L772-779).
  - `preferredModelId` picks the saved model, then the last choice, then the catalog default (L782-788).
  - `firstEnabledHarness` respects per-project hidden providers (L795-809).
  - `defaultSessionChoice` is `project.defaultHarness ?? last?.harness ?? "cursor"` (L812-823).
  - Access mode is inherited from the active session rather than persisted (App.tsx:2250-2262).

### F11. Installed CLI surface

- **claude 2.1.285** ($ `claude --help`)
  - `--permission-mode` choices: "acceptEdits", "auto", "bypassPermissions", "manual", "dontAsk", "plan". `default` isn't listed but is accepted ($ `claude --permission-mode default --version` → `2.1.285`). An unknown value errors with "Allowed choices are …".
  - Other flags:
    - `--allow-dangerously-skip-permissions`
    - `--effort <level>` (low, medium, high, xhigh, max)
    - `--model <model>`, with aliases 'fable', 'opus' and 'sonnet' or a full name
    - `-n, --name <name>` ("shown in the prompt box, /resume picker, and terminal title")
    - `-w, --worktree [name]`
- **codex-cli 0.159.0** ($ `codex --help`)
  - Flags:
    - `-s, --sandbox` read-only | workspace-write | danger-full-access
    - `-a, --ask-for-approval` on-request | never. `-a untrusted` fails with "invalid value 'untrusted'"
    - `--approve-for-me` ("Route approval requests through automatic review using the workspace-write sandbox")
    - `--dangerously-bypass-approvals-and-sandbox`
    - `--worktree`, `-m`, `-p`, `-c key=value`
  - `--full-auto` is gone.
  - The binary contains the config keys `model_reasoning_effort` and `plan_mode_reasoning_effort` ($ `grep -a -o model_reasoning_effort …/0.159.0-aarch64-apple-darwin/bin/codex`). `-c model_reasoning_effort=<level>` is therefore very likely valid, but its behaviour is untested.

### F12. Code vs docs disagreements

- **Zeron projectless screenshot vs code.** The screenshot shows device and project chips left-aligned, with the pill at the bottom (Z docs/screenshots/projectless/restored-selection.png). The code right-aligns the target row above a centred hero (Z crates/ui/src/composer.rs:9644-9660; Z crates/ui/src/composer_dock.rs:341). Code wins.
- **Zeron harness screenshots vs code.** Older screenshots show a two-column AGENTS/MODELS picker and a settings title "Agents". The code has a tabbed card (Z crates/ui/src/pickers.rs:3582-3918) and the page title "Providers" (Z crates/ui/src/settings/harnesses.rs:1221).
- **Report 14 line numbers.** Report 14 cites Pocket keybindings at `main.rs:1046-1057`. At b9d14a1 they are at P packages/desktop/crates/pocket/src/main.rs:1062-1071.
- **Report 10-4 vs the Codex CLI.** 10-4's Codex Supervised row sends `approvalPolicy: untrusted` over app-server (M …/codexProtocol.ts:62-95), but the 0.159 CLI flag rejects `untrusted` (F11). This is not a contradiction, since they are different surfaces, but a terminal-mode port can't copy the value.

### F13. Mapping onto Pocket: terminal now, headless later

| Pick | Stage A: desktop argv (now) | Stage B: `LaunchSpec`, pocketd builds argv | Stage C: headless (03-2/10-1, 10-2) |
|---|---|---|---|
| Provider | `argv[0]` = `claude` / `codex` (forms.rs:416) | `spec.provider` | Driver choice |
| Model | `--model <id>` / `-m <id>`. The list comes from the model hint until 03-7/10-11 land | `spec.model` | Spawn arg for Claude; per-turn for Codex via `agent.configure` (10-3) |
| Effort | `--effort <l>` / `-c model_reasoning_effort=<l>` | `spec.effort` | Per turn |
| Access | F14 table | `spec.access` | 10-4 matrix (keep `untrusted` for app-server if it still accepts it) |
| Plan | `--permission-mode plan` replaces the access flag at spawn. Disabled for Codex | `spec.plan` | Per-turn intent (10-16); access applies after the plan |
| Checkout | Pocket `git::add_worktree` (forms.rs:430-449), not CLI `-w` / `--worktree` | `spec.checkout {kind, base, name}` in pocketd | Same |
| Setup / copy env | `setup_op` / file copy (forms.rs:431-448) | `spec.copyEnv`, `spec.runSetup` | Same |
| Prompt | Trailing positional arg; empty is allowed (forms.rs:415) | `spec.prompt` | First `agent.prompt` |
| Name | `claude -n <name>` (optional, 17-12) | `spec.name` | Session title |
| Change after start | Only through the CLI's TUI (`/model`, mode cycling). The chips lock once the terminal exists | Same | `agent.configure`; show MonoCode's "applies to the next turn" footnote (AccessPicker.tsx:169-173) |

- Stage B lets the phone start terminal sessions through `agent.create` before any headless driver exists (08 F10 phone canvas: P docs/orchestrators/research/08-zeron-mobile.md:328-341; 10-3: P docs/orchestrators/research/10-monocode-architecture-harness.md:415).
- Stage B also moves argv building from the desktop (forms.rs:402-459) into one pocketd function. That function then serves the phone, the desktop sheet and the tab-menu quick rows (main.rs:595-599).
- Model and effort discovery is 03-7 (P docs/orchestrators/research/03-zeron-harness.md:372) and 10-11 (10-monocode-architecture-harness.md:423). Not redone here. Until then the model chip shows the provider's name alone or the last-seen label, never "Default model" (Z pickers.rs:158).

### F14. One permission picker, reconciled with 10-4

| Access (Pocket label) | Icon / colour | Hint (adapted from M session.ts:382-388) | claude 2.1.285 argv | codex 0.159.0 terminal argv | Codex headless (10-4) |
|---|---|---|---|---|---|
| **Ask** (default) | lock, TEXT_2 | "Ask before commands and file changes." | `--permission-mode default` | `-s read-only -a on-request` | `untrusted` / `read-only` / user |
| **Auto-accept edits** | pencil | "Auto-approve edits, ask before other actions." | `--permission-mode acceptEdits` | `-s workspace-write -a on-request` | `on-request` / `workspace-write` / user |
| **Auto** | sparkle | "A reviewer model approves or denies actions." | `--permission-mode auto` | `--approve-for-me` | `on-request` / `workspace-write` / `auto_review` |
| **Full access** | shield, WAITING `0xffb224` (text WAITING_TEXT `0xad5700`) | "Run commands and edits without prompts." | `--permission-mode bypassPermissions --allow-dangerously-skip-permissions` | `-s danger-full-access -a never` (or `--dangerously-bypass-approvals-and-sandbox`) | `on-request` / `danger-full-access` / user |
| **Plan first** (toggle, separate row) | checklist | "Review a plan before building." (M Composer.tsx:2179-2201) | `--permission-mode plan` instead of the access flag | Disabled: "Codex plan needs a chat session" | `collaborationMode.mode:"plan"` (10-16) |

- **Rules**
  - Every spawn sends both axes explicitly. Ask is never "no flag" (10-4; M claudeProtocol.ts:105-126).
  - The default is Ask.
  - Full access is never persisted. Reopening falls back to the last non-Full value.
  - Full access from the phone needs an explicit per-session opt-in (P docs/orchestrators/research/10-monocode-architecture-harness.md:444).
  - Claude's `dontAsk` and `manual` are not exposed. `manual` stays as the fallback if `default` stops being accepted.
- **Deviations from 10-4**
  - Codex Ask in terminal mode uses `-a on-request`, because the CLI rejects `untrusted` (F11).
  - Full access uses `-a never`. MonoCode's reason for avoiding `never` ("never rejects escalations before the client handler", M codexProtocol.ts:62-95) only applies to app-server, where a client handler exists.
- **Labels**
  - "Ask" keeps Pocket's existing word (forms.rs:507) and its "Needs you" idiom, instead of MonoCode's "Supervised".
  - "Auto-accept edits" replaces "Auto-edit". "Plan only" becomes a toggle, because in MonoCode and 10-16 plan is an intent, not an access level.

### F15. Recommendation: the hero composer replaces the modal

- **Replace it.** Reasons:
  - Both references start work from the empty canvas composer (Z tabs.rs:222-285; M App.tsx:2250-2262).
  - Pocket's blank page already sits in that spot with a button that opens the sheet (view.rs:697-714).
  - P2's headless chat needs the same composer anyway (03-4/10-12).
  - The modal closes before the worktree is created and loses the prompt on git errors (forms.rs:449,459).
- **Keep speed.** P1 has no entrance motion and no glide (overlay.rs:321). ⌘N shows the canvas with the textarea focused. ⌘⇧N does the same with New worktree preselected.
- **P1 (terminal).**
  - The canvas replaces the main pane while it is a draft, like Zeron's `select_chat(None)`.
  - On a successful spawn, the new terminal tab takes over. On failure, the canvas stays with the error inline.
  - Navigating away keeps the draft. Esc discards it and returns to the previous session.
- **P2 (headless).** The same canvas docks into the transcript with Zeron's glide (0.420 s) or MonoCode's (480 ms `cubic-bezier(0.22,1,0.36,1)`), respecting `follow_reduce_motion` (P packages/desktop/crates/pocket/src/main.rs:1041-1044).
- **Minimal alternative** if 17-4 is deferred: keep the sheet, but land 17-1, 17-2, 17-3 and 17-5 in `forms.rs`. Every chip spec below applies to the sheet unchanged.

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 17-1 | Replace Codex `--full-auto` with `-s workspace-write -a on-request` | Codex "Auto-edit" starts instead of printing an error | P …/pocket/src/forms.rs:412; $ `codex --full-auto --version` | desktop `crates/pocket` forms.rs, adapt | S | — |
| 17-2 | Explicit access flags on both axes for every mode; Codex Plan disabled | "Ask" really asks; no silent `auto` from user config | P forms.rs:408-414; M …/claudeProtocol.ts:105-126; $ `claude --permission-mode default --version` | forms.rs, later the pocketd argv builder (17-10), adapt | S | — |
| 17-3 | Access picker: Ask / Auto-accept edits / Auto / Full access (amber) plus a "Plan first" toggle | One predictable safety control, the same on desktop, phone and headless | M …/AccessPicker.tsx:29-173; M …/session.ts:366-388; 10-4 | desktop forms.rs (split out of `agent_picker`) + `theme` WAITING, new | M | 17-2 |
| 17-4 | Draft "New session" canvas replaces the modal sheet | Start work where the session will live; the draft survives navigation and errors | Z …/shell/tabs.rs:222-285; Z …/composer.rs:9644-9760; M src/app/App.tsx:2250-2262; P forms.rs:544-645; P view.rs:697-714 | desktop `crates/pocket` new canvas view reusing `NewForm`; drop `Overlay::NewSession`, adapt | L | — (better after 17-3) |
| 17-5 | Persist picks in `desktop.json` `composer {provider, model_by_provider, effort_by_provider, access}` | The next session starts the way the last one did | Z …/settings/composer.rs:20,46-71; P …/store/src/store.rs:8-28; P forms.rs:234-235,321 | `crates/store` new field; atomic write is also new (`Store::save` is a plain `std::fs::write`, store.rs:38-42), port | S | — |
| 17-6 | Checkout chip (Current checkout / Current worktree / New worktree), base chip "From {base}" with search, inline branch name | See and edit where work lands without reading footer prose | Z …/proto/src/view.rs:540-586; M …/WorkspacePicker.tsx:137-631; P forms.rs:520-541,622-629 | forms.rs `branch_picker` + new chips, adapt | M | 17-4 for placement |
| 17-7 | Agent/model card: provider sections, effort submenu, "Default" badge, selected-only row, dimmed missing CLIs | Pick model and effort at start without typing `/model` | Z …/pickers.rs:158,179-190,1961,4911,5427; M …/ModelPicker.tsx:83-96,1361-1384; P forms.rs:491-517 | forms.rs `agent_picker` rewrite, adapt | M | 03-7 / 10-11 catalog |
| 17-8 | CLI availability probe (proposed: `command -v` in the login shell; MonoCode instead stats ~100 resolver paths) with a 30 s TTL with install hints and "No agents available" | No dead "Codex" row on a Mac without Codex | M …/availability.ts:36-77; Z …/settings/harnesses.rs:64-97; Z pickers.rs:3647-3655 | desktop, or a pocketd `agents` op; `terminal.LookPath` PATH (P packages/pocketd/internal/terminal/terminal.go:87-101), new | S | 10-6 for PATH parity |
| 17-9 | pocketd-offline state on the canvas; surface spawn and worktree failures before closing | No silent no-op when pocketd is down or git fails | P …/daemon/src/daemon.rs:183,190; P forms.rs:436,449,459; Z composer.rs:7600-7610 | `crates/daemon` send result + canvas error row, adapt | S | — |
| 17-10 | `LaunchSpec` in `packages/protocol`, argv built in pocketd; phone `agent.create` starts terminal sessions | Start sessions from the phone now, without headless | P forms.rs:402-459; P packages/pocketd/internal/wsserver/wsserver_test.go:157-160; P packages/app/src/screens/AgentsScreen.tsx:24; 10-3; 08 F10 | `packages/protocol` + pocketd + desktop, new | M | 17-2 |
| 17-11 | Tab-menu Claude Code / Codex quick rows spawn with the remembered picks | Quick tabs stop ignoring access and model | P …/pocket/src/main.rs:595-599; P view.rs:950-980 | main.rs `new_agent_tab`, adapt | S | 17-5 |
| 17-12 | Pass `claude -n <worktree or slug>` | Session name shows in Claude's prompt box, `/resume` picker and terminal title | $ `claude --help` (`-n, --name`) | argv builder, new | S | 17-2 |
| 17-13 | Hero→dock glide when the draft becomes a headless chat | Continuity from draft to transcript | Z …/composer_dock.rs:341,468-470,606; M …/useComposerDockMotion.ts:4-54 | desktop canvas + transcript pane, port | M | 03-2/10-1, 10-12, 17-4 |
| 17-14 | ⌘/ opens the agent/model card; ⌘1-9 pick rows inside open cards | Keyboard-only setup | Z …/settings.rs:1107-1109; Z pickers.rs:2509-2620; P main.rs:1062-1071 (no ⌘/) | `crates/pocket` bindings, port | S | 17-7 |
| 17-15 | Projectless sessions ("Don't work in a project", cwd `~`) | Scratch questions outside any repo | Z pickers.rs:2439; M …/session.ts:505-534 | — | M | wont: Pocket is project/worktree-first |
| 17-16 | Host/device picker ("This device", "You") | Run on another machine | Z pickers.rs:2298-2335,2928 | — | L | wont: Mac + phone model; see 10-19 |
| 17-17 | Headless sign-in dialog "Authentication required" | Recover from a logged-out CLI without a terminal | M …/ProviderSignInDialog.tsx:43-54; Z …/catalog_failure.rs:55-71 | desktop + phone dialog over pocketd auth op, new | M | 10-1 / 10-2 |
| 17-18 | Model favourites/stars tab | Faster picks with big catalogs | Z pickers.rs:3814 | — | S | wont: 2 providers, small catalogs |

## UI/UX spec to copy

### S1. Draft canvas (replaces the sheet; P1 terminal mode)

```
┌ main pane ───────────────────────────────────────────────────────────┐
│                                                                      │
│                                      [▣ pocket ⌄]      ← target row  │
│  ┌ composer r14 ───────────────────────────────────────────────────┐ │
│  │ Describe what the agent should do…                              │ │
│  │                                                                 │ │
│  │ [◐ Claude Code · opus · high ⌄] [🔒 Ask ⌄] [Plan ×]        (↑)  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│   [⎇ New worktree ⌄] [From main ⌄]  branch calm-otter   ⌘↵ start · esc │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

- **Position**
  - Centred horizontally, max_w 640 (Pocket sheet width, forms.rs:643).
  - Vertical Y = `(pane_h − composer_h)·0.5 + 8` (Z composer_dock.rs:341).
  - No headline, following Zeron (Z tabs.rs:283-285). If one is wanted, use "What should we work on in {project}?" (M EmptySession.tsx:44).
- **Composer**
  - r14, white, ring (forms.rs:601). Text 15/23.25.
  - Min 105 px, grows to 260 max (Z composer.rs:56), then scrolls.
  - Placeholder "Describe what the agent should do…".
  - Send is a 32 px round button at the right of the action row, disabled until `session_ready` (forms.rs:393-400).
- **Action row (inside the composer, bottom)**
  - Agent chip and access chip use the Pocket `chip` style (h30, pl10, pr8, gap7, r8, FILL_2/FILL_3).
  - The "Plan ×" pill appears only when the toggle is on: WAITING tint, title "Turn off Plan first" (adapted from M Composer.tsx:2329-2343).
- **Target row (above, right-aligned)**
  - Row h20, 28 px above the composer, inset 26 px from the right (Z composer.rs:81,9644-9660).
  - Holds the project chip: 18 px repo tile + name + chevron.
- **Bottom slot (below)**
  - Slot h24, px10 (Z composer.rs:84,9680-9760).
  - Small chips: h20, px8, gap6, r6, 12 px TEXT_3 → TEXT_2 on hover, 12 px icon, 12 px chevron at 0.5 opacity (Z pickers.rs:2818-2877).
  - Right-aligned hint "⌘↵ to start · esc to cancel" in TEXT_4 (forms.rs:640).
  - **New worktree:** `[⎇ New worktree ⌄] [From {base} ⌄]`, then "branch" in TEXT_4 and an inline editable name in mono 12.5. Name errors show inline in FAILED with the existing copy (forms.rs:111,113).
  - **Current checkout:** `[▢ Current checkout ⌄]`, then "on {branch}", read-only. Unlike Zeron's `switch_draft_ref` (Z pickers.rs:1562), the draft never runs `git checkout`.
  - **Current worktree:** as Current checkout, labelled "Current worktree" when the selected tree is a worktree (Z view.rs:586).

### S2. Agent/model card (from the agent chip)

- **Menu**
  - `picker_menu` (p5, r10, max_h 360), w300.
  - Opens above the chip when there is less than 180 px below (Z pickers.rs:2635-2686).
- **Sections**
  - `pick_head` "Claude Code" / "Codex" (theme `provider_name`, P packages/desktop/crates/theme/src/theme.rs:98-112), each with a provider-colour mark: AGENT_CLAUDE `0xd97757`, AGENT_CODEX `0x0f9d8a`.
  - Rows are `pick_row` (h34): model label 13 medium, meta = effort in TEXT_4, check column 16.
  - The catalog default gets a "Default" meta badge (Z pickers.rs:4911).
- **Effort**
  - A setting row (h30) at the bottom of each section: "Effort", with the value on the right and a › chevron.
  - → opens the submenu (low / medium / high / xhigh / max for Claude; the catalog's ladder for Codex). ← or Esc closes it.
  - Default is High, else Medium (Z pickers.rs:179-190).
- **Missing CLI**: the section head is dimmed at 0.5 opacity with one row, "Install the Codex CLI to enable. Install with `npm install -g @openai/codex`" (Z harnesses.rs:64-97,1378). Not selectable.
- **Selected-only**: a saved model missing from the catalog stays, with meta "Not in the current model list" (adapted from Z pickers.rs:1961).
- **Search**: only when the total rows exceed 12. Placeholder "Search models…", empty "No models found" (Z pickers.rs:1228,3810).
- **Keys**
  - ↑/↓ wrap, ↵ picks and closes, ⌘1-9 jump to rows 1-9, Esc closes.
  - ⌘/ opens the card from the canvas (Z settings.rs:1109).
- **Before 03-7/10-11**: one row per provider with the last-seen label (view.rs:926-929), otherwise the provider name. Never show "Default model" (Z pickers.rs:158).

### S3. Access menu (from the access chip)

- **Menu**: `picker_menu`, w288 (M AccessPicker.tsx:29).
- **Rows**
  - Two lines: `items-start`, gap 10, px8, py8, r6.
  - Icon 14 in TEXT_2; Full access icon and label in WAITING_TEXT.
  - Label 13 medium, line height 20. Hint 12 in TEXT_4, line height 16.
  - Four rows as in the F14 table, then a SEPARATOR `0x11111317` line, then the toggle row "Plan first" / "Review a plan before building." with a check when on.
  - For Codex in terminal mode the toggle row is dimmed, with hint "Codex plan needs a chat session".
- **Chip label**: icon + label. Full access shows the chip text in WAITING_TEXT.
- **Keys**: ↑/↓ clamp without wrapping and ↵ picks (M AccessPicker.tsx:68-84). Space toggles Plan when its row is focused.
- **P2 only**: footnote "Access changes apply to the next turn. Stop and resend to apply them now." while a turn runs (M AccessPicker.tsx:169-173).

### S4. Checkout, base and project menus

- **Checkout** (w240)
  - `pick_head` "Workspace".
  - Rows: "Current checkout" (folder icon, meta = branch) and "New worktree" (tree icon, meta "from {base}") (M WorkspacePicker.tsx:296-386).
- **Base** (w300)
  - The existing `branch_picker` heads "Default" / "Recent" (forms.rs:520-541).
  - Adds a search field "Search branches…" when there are more than 20 branches. Empty result "No matching branches" (M WorkspacePicker.tsx:631). When truncated, "Showing 20 of {n} branches" (Z pickers.rs:3510).
  - Chip label "From {base}" (M WorkspacePicker.tsx:540; Z pickers.rs:2134).
- **Project** (w280, Z pickers.rs:2907-3049)
  - `pick_head` "Projects". Rows: 18 px tile, name, check.
  - Footer row "Add project…". Empty filter result "No projects match." (Z pickers.rs:2382).

### S5. States

| State | Detection | UI | Copy |
|---|---|---|---|
| No project | `self.project.is_none()` (view.rs:698) | Canvas is replaced by the existing blank page | "Add a project to begin." + "Add project" button |
| pocketd offline | daemon error "pocketd disconnected" (daemon.rs:183) | Send disabled; one TEXT_3 line in the bottom slot; the draft is kept | "pocketd disconnected — sessions can't start." (new) |
| Provider CLI missing | `command -v {cli}` in the login shell (new), 30 s TTL (M availability.ts:64) | Section dimmed with install hint (S2); chip falls back to the other provider | Z copy (harnesses.rs:64-97) |
| No CLI at all | Both probes fail | Agent chip reads "No agents available"; send disabled | "No agents available" / "Install Claude Code or Codex, then reopen." (adapted from Z pickers.rs:3647-3655) |
| Not logged in | Terminal mode: the CLI's TUI shows its own login | None in P1 | P2: "Authentication required" / "Sign in to continue using {Provider}." (M ProviderSignInDialog.tsx:43-44) |
| Branches loading | `f.branches.is_empty()` (forms.rs:396) | Base chip shows "Loading branches…" in TEXT_4; send disabled for worktree | new |
| Name taken / invalid | `name_problem` (forms.rs:111-113) | Inline FAILED `0xe5484d` under the name; send disabled | existing copy |
| Worktree / spawn failed | `add_worktree` Err (forms.rs:449) | Canvas stays with the prompt intact; error row above the bottom slot | `{git error}` verbatim |
| Model catalog failed | 03-7/10-11 | Selected-only row keeps the saved model | per 03-7/10-11 |

### S6. Defaults and memory

| Field | Default (first run) | Remembered | Scope | Source |
|---|---|---|---|---|
| provider | claude | yes | global | Z settings/composer.rs:46-71; P forms.rs:234 |
| model | catalog default | yes, per provider | global | Z `model_by_harness`; M models.ts:782-788 |
| effort | High, else Medium | yes, per provider | global | Z pickers.rs:179-190 |
| access | Ask | yes, except Full access | global (per-project is an open question) | M session.ts:373; F14 |
| plan | off | no | per draft | M Composer.tsx:1503-1508 (per turn) |
| project | selected sidebar project | n/a (follows the sidebar) | — | Z tabs.rs:222-275 |
| checkout kind | Current (⌘N) / New worktree (⌘⇧N) | no | per draft | Z view.rs:547; P main.rs:1064,1069 |
| base | `cfg.base` → main → master → current | per repo in `RepoConfig.base` (existing) | repo | P forms.rs:124; P store.rs:8-28 |
| copy env / run setup | on when configured | existing repo config | repo | P forms.rs:339-340 |
| prompt | empty | kept while the draft is open | draft | new |

### S7. Motion, shortcuts, copy

- **Motion**
  - P1 has no canvas motion (overlay.rs:321). Picker menus stay instant, as today: `picker_menu` wraps `ui::pop`, which has no animation (P packages/desktop/crates/pocket/src/forms.rs:179-193; P packages/desktop/crates/ui/src/ui.rs:32-34).
  - P2 adds the glide: 0.420 s dock / 0.470 s return (Z) or 480 ms `cubic-bezier(0.22,1,0.36,1)` (M), skipped under reduced motion (P main.rs:1041-1044).
- **Shortcuts**
  - ⌘N opens the canvas (Current checkout). ⌘⇧N opens it with New worktree preselected.
  - ⌘↵ starts. Esc closes an open menu, otherwise discards the draft.
  - ⌘/ opens the agent/model card. ⌘1-9 work inside an open card.
  - Palette row "New session in {project}" (overlay.rs:126) now focuses the canvas.
- **Copy**

  | Where | Text |
  |---|---|
  | Placeholder | "Describe what the agent should do…" |
  | Hint | "⌘↵ to start · esc to cancel" |
  | Access labels | "Ask", "Auto-accept edits", "Auto", "Full access", toggle "Plan first" |
  | Chips | "Current checkout", "Current worktree", "New worktree", "From {base}", "branch {name}", "on {branch}" |

## Open questions / risks

- **`claude --permission-mode default`** is accepted but hidden in 2.1.285. If a later CLI drops it, switch to `manual` (listed; ≥2.1.200 per M claudeProtocol.ts:105-126). This needs a version gate (03-12).
- **Codex effort.** `-c model_reasoning_effort=<level>`: the key exists in the 0.159 binary (F11), but the accepted values and the behaviour on a model without that level are untested.
- **Codex plan in terminal mode.** The binary has `plan_mode_reasoning_effort` and collaboration-mode config keys, but no CLI flag. It is unverified whether any `-c` key starts the TUI in plan mode. Until then the toggle stays disabled for Codex.
- **Plan + access in Claude terminal mode.** `--permission-mode plan` takes the only mode slot, and after plan approval the TUI asks which mode to continue in. The chosen access is not applied automatically.
- **Full access from the phone** (10 L444 asks for a per-session opt-in). Does it also need a standing Mac-side switch, and if so per project or per device?
- **Draft placement for New worktree.** The tree doesn't exist yet. It is unclear which sidebar row stays selected while drafting, and whether to show a pseudo-row.
- **CLI-native worktrees** (`claude -w`, `codex --worktree`) vs Pocket's `git::add_worktree`. Keep Pocket's: it owns copy-env, setup, naming and sidebar placement. Risk: users expect CLI-managed paths.
- **App-server `untrusted`** (10-4, M codexProtocol.ts:62-95) vs the CLI rejecting it. Re-verify on app-server when 10-2 lands.
- **Empty prompt.** Terminal mode allows starting with no prompt (forms.rs:415). Headless may need one. Decide before 17-10 fixes the `LaunchSpec` schema.
- **Probe PATH.** A desktop login shell and a launchd-started pocketd can disagree (10-6; P packages/pocketd/internal/terminal/terminal.go:87-101). A CLI can show as installed on the Mac and still fail from the phone.
- **Remembering access** globally vs per project. A risky mode picked in one repo carries into another.
- **The model hint is misleading.** It comes from the latest agent's label (view.rs:926-929), which is a history value presented as a choice. Until 03-7/10-11 land, sending `--model` from it could pin a stale model. Omit `--model` unless the user picked explicitly.
- **Silent spawn failure.** `agent_op` hands over to the shell (daemon.rs:98-112), so a bad flag is visible only as terminal text. 17-9 cannot detect CLI-level failures without an exit-code signal from the script.

## Verification

- Date: 2026-09-30
- Claims checked: 40 (Pocket argv/perm match, `agent_op` shell hand-off, silent `writeln!`, sync overlay close, keybindings, store fields, chip/menu tokens, phone `agent.create` rejection; `claude`/`codex` flags re-run locally incl. `--full-auto` and `-a untrusted` rejections and the F14 argv; Zeron hero Y, 0.420/0.470 s glide, fixed `WorkspaceWrite`, composer constants, target row/bottom slot, shortcuts, `composer-defaults.json` fields, `CheckoutKind`, picker copy, keys, reasoning default, harness filtering, auth detection; MonoCode `onNew`, headline, dock motion, access modes and Codex/Claude mappings, AccessPicker, WorkspacePicker, ModelPicker, availability TTL, models.ts memory; prior-report line refs; projectless screenshot).
- Corrected: 6
  - S7: Pocket picker menus have no fade; ui.rs:135 is a glyph-swap animation, not a menu fade. Re-cited to `picker_menu`/`ui::pop`.
  - F4: Zeron footer chip hover brightens to `text` at 0.8, not `text_muted`.
  - F9: MonoCode tints only the Full access icon amber, not the label.
  - 17-8 / S5: MonoCode's probe stats resolver paths; `command -v` in the login shell is a Pocket proposal, now labelled as such.
  - F14 / open questions: report 10 L444 asks for a per-session opt-in, not a "Mac-side" one; wording aligned.
  - 17-5: Pocket's `Store::save` is not atomic today; mapping now says so.
