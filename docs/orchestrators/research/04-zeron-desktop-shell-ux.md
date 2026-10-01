# Zeron desktop shell UX: sidebar, palette, navigation, chrome

Date: 2026-09-30.

Sources:
- Zeron: zeronsh/comet @ ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (MIT, Rust + GPUI fork). https://github.com/zeronsh/comet, https://zeron.sh. Latest releases: v0.2.99 (2026-09-30), v0.2.98 (2026-09-29), v0.2.97 (2026-09-28), via `gh api repos/zeronsh/comet/releases`.
- MonoCode: hardbeat920/monocode @ cdc1441dc51e3709cd843e5c316608a123f323c6. Not used; this topic covers Zeron's shell only.
- Pocket: worktree `orchestrator-research` @ 86deb13.

Citation legend: `Z path:L` is a file and line in the Zeron clone, relative to its root. `P path:L` is the same in the Pocket worktree. `M` would mark MonoCode; it is unused here. A range `A-B` covers the whole construct. Where the code disagrees with a README or screenshot, the code wins and the finding says so. Quoted strings are exact UI copy. Sizes are GPUI logical px, and times are ms.

## TL;DR

- Zeron's shell has four regions. The sidebar is the session list; the horizontal strip was removed (`Z crates/ui/src/shell/tabs.rs:1-5`). A 38px unified titlebar names the selected session. A right surface column holds Browser/Terminal/Diffs/History, alongside a files column and a bottom terminal drawer. Settings is a full-window route.
- **Status never moves rows.** Order is pure recency with an id tiebreak (`Z crates/proto/src/view.rs:89-103`). The doc comment cites a user report that rows jumped under the pointer when opening a Done session. Status only drives the dot. Pocket does the opposite: it sorts Needs you first (`P packages/desktop/crates/pocket/src/view.rs:661`) and sorts the rail and palette by status (`view.rs:481`, `overlay.rs:91`).
- There is one visible-order function (`Z crates/ui/src/shell/spaces.rs:4208-4298`): pins, then expanded sections, then groups. Cmd+1..9 jump, Ctrl+Tab cycling (wraps) and hint chips all read it, so shortcuts always match what is on screen.
- Holding the jump modifier swaps each of the first 9 rows' time/status corner for a mono chip such as "⌘1". Hints hide when any overlay owns the keyboard (`Z crates/ui/src/shell.rs:5243-5254`, `Z crates/ui/src/settings.rs:1390-1407`).
- The command palette matches every whitespace-split word as a substring. The haystack is title, project, device, branch and PR. The 30-row cap applies after filtering, arrows wrap, and there is a 3-hint footer plus an empty-state hint (`Z crates/ui/src/shell/command_palette.rs`). Pocket matches the title substring only, caps sessions at 5 (after filtering and a status-then-recency sort), and clamps arrows (`P packages/desktop/crates/pocket/src/overlay.rs:79-94,155-156`).
- The session context menu has Rename…, Pin/Unpin, Archive, a Copy submenu (Path, link, harness session ID) and Delete… (`Z crates/ui/src/shell.rs:9273-9500`). Pocket has row menus only for projects and worktrees (`P packages/desktop/crates/pocket/src/view.rs:389-420`).
- Window geometry is persisted per display UUID and restored with fallback. So are every column width, open state and view option, in `ui-settings.json` with a 400ms debounced save. Minimum window size is 900×600 (`Z crates/ui/src/lib.rs:293-319,373`; `Z crates/ui/src/settings.rs:34-65`). Pocket keeps widths in memory only, has no min size, and always opens 1440×900 centered (`P packages/desktop/crates/pocket/src/main.rs:1059-1061`).
- Motion is centralized in named `MotionSpec` tokens and honors GPUI `reduce_motion` (`Z crates/ui/src/motion.rs:17-20,375-426`):
  - hover 150ms
  - menus 140ms in, 100ms out
  - dialogs 180ms
  - collapse 180ms
  - FLIP resort glide 260ms ease-out-quint
- Desktop notifications and sounds fire on transitions to Needs you, Failed and Done. By default they are background-only. Clicking one opens the session (`Z crates/ui/src/sound.rs:338-354`, `Z crates/ui/src/notify.rs:1-25`, `Z crates/ui/src/lib.rs:278-283`). Pocket already shows system notifications on transitions to Needs you, Failed and Done, skips sessions being viewed, dismisses them once seen, and focuses the session on click (`P packages/desktop/crates/pocket/src/main.rs:314-328,1103-1106`, `P packages/desktop/crates/pocket/src/status.rs:29-31,120-124`). It lacks sounds and any notification settings.
- Cheap wins for Pocket:
  - persist geometry
  - notification sounds and settings
  - palette matching and wrap
  - stable order
  - jump and cycle shortcuts
  - session context menu

  Everything else (pins, sections, view options, project actions) is optional polish.

## Findings

### F1. Shell layout and chrome
- Regions:
  - left sidebar (the session list)
  - center transcript
  - right surface column with its own surface strip
  - files column
  - bottom terminal drawer
  - Settings as a separate route
- The titlebar (`render_session_title_bar`, `Z crates/ui/src/shell/tabs.rs:282`) shows the harness brand icon and session title. While a session is selected, a left cluster holds "+" (new session) and a fork button ("New side chat", `tabs.rs:529`).
- Right-edge anchors are the explorer toggle ("Show files panel", `tabs.rs:461`) and the right-column toggle ("Expand panel", `tabs.rs:451`). Each is 28px with a 4px gap (`tabs.rs:49-54`).
- The legacy strip setting `UiSettings.open_tabs` is no longer read (`tabs.rs:1-5`). The marketing screenshot still shows a strip, so code wins.
- Sizes:
  - TITLEBAR_HEIGHT 38, TITLEBAR_TOP_PAD 4, HEADER_HEIGHT 44, STATUS_STRIP_HEIGHT 24
  - PANEL_RADIUS 10, CONTROL_RADIUS 6, SPACE_XS 4, SPACE_SM 8 (`Z crates/ui/src/theme.rs:815-841`)
- Window:
  - default 1320×880 centered (`Z crates/ui/src/lib.rs:294`), min 900×600 (`lib.rs:373`)
  - traffic lights at (14,14) (`lib.rs:393`)
  - the app owns titlebar drag (`lib.rs:398`)
  - geometry restores onto the saved display by UUID and falls back to centered if that display is gone (`lib.rs:293-319`)
  - the close path deliberately skips the display query, working around an X11 panic (`lib.rs:326-330`)
- Column limits (`Z crates/ui/src/settings.rs:34-65`):

  | Column | Min | Max | Default |
  |---|---|---|---|
  | Sidebar | 224 | 400 | 256 |
  | Files | 220 | 440 | 286 |
  | Right column | 360 | — | 520 |
  | Chat | 300 | — | — |
  | Terminal drawer (height) | 160 | min(0.55·vh, 2000) | 280 |

  Settings save to `ui-settings.json` with SAVE_DEBOUNCE_MS 400. Values are clamped on load (`settings.rs:1527-1542`).
- Pocket comparison:
  - window 1440×900 centered, no min size, no restore (`P packages/desktop/crates/pocket/src/main.rs:1059-1068`)
  - `widths: [Option<f32>;2]` lives in memory; Projects clamps to 200-420 and Sessions to 280-600 (`P packages/desktop/crates/pocket/src/view.rs:183-190`)
  - the resize handle is 5px (`view.rs:161-181`)

### F2. Sidebar structure and order
- Render order in `render_chat_sidebar` (`Z crates/ui/src/shell.rs:7684-8110`):
  1. filter row, outside the scroll area
  2. edge-faded scroll list (px 8, pt 4; SIDEBAR_LIST_GAP 2)
  3. connection pill
  4. "Star on GitHub" banner (dismissible, persisted)
  5. update strip
  6. sidebar notice (danger border, 11px, click to dismiss)
  7. footer (p 8, 28px buttons, 16px avatar)
- Filter row (`Z crates/ui/src/shell/spaces.rs:3797-4005`):
  - space trigger: h29, radius 8, FOLDER 16, label "All projects" or the space name, 10px device tag at 0.45 opacity, WIFI_OFF 12 glyph when offline, caret
  - "Sidebar view options" button 29×29 with a 350ms tooltip; Enter, Space or ArrowDown opens it
- Space menu (`spaces.rs:4009-4200`):
  - card width = sidebar width − 16; list max height 224
  - selection is a wash only, no check glyph
  - right-click a row to rename or remove
  - full-bleed divider, then a pinned "New project…" as the last nav stop
- Visible order (`spaces.rs:4208-4298`):
  1. pins, frozen during drag
  2. custom sections, expanded ones only
  3. groups: by device (local device promoted, `spaces.rs:2080`), by project (`space_id`, else `home:<device>`), or flat
- Jump, cycle and hints all use this order (`Z crates/ui/src/shell.rs:5204-5210`).
- Doc/code mismatch: a doc says "By project" normalizes to "In one list". Code has three organize modes, "By device", "By project" and "None" (`spaces.rs:3466-3468`).
- Sort (`Z crates/proto/src/view.rs:89-103`):
  - key `last_message_at` descending, falling back to `created_at`, with an id tiebreak
  - test `active_list_sorts_by_recency_only_status_never_moves_rows` (`Z crates/ui/src/state.rs` ~4249)
  - `attention_rank` (`view.rs:75`) is used only for the aggregate dot on space rows
- When order changes, rows glide via FLIP (RESORT 260ms, EASE_RESORT = cubic-bezier(0.22,1,0.36,1); `Z crates/ui/src/shell.rs:885-894`, `Z crates/ui/src/motion.rs:305-307`). New rows fade in; removed rows vanish.
- Archived section (`spaces.rs:5014-5172`): 10 rows at first, then pages of 25. Copy: "Archived ({total})", "Show {remaining} more".
- Pocket comparison:
  - the aside (`P packages/desktop/crates/pocket/src/view.rs:193-262`) groups by project with worktree rows
  - the session list lives in a second column (`view.rs:636-664`), filtered to the current worktree
  - sessions reorder when status changes (`view.rs:661`)

### F3. Session row
- Height (`Z crates/ui/src/shell.rs:936-956`):
  - 45 base
  - 47 + 14 when the branch shows, or 47 + 16 with a PR (tallest wins)
  - −16 when the project label is hidden
  - compact: 29
- Styling (`shell.rs:7195-7250`):
  - flex column, gap 2, radius 8 (PALETTE_ITEM_RADIUS 10 inside the palette), px 8, py 6
  - rest text 0.8 opacity; archived 0.55; selected or searching 1.0
  - selected is `glass_selected_bg`; hover blends 150ms toward `glass_hover`
  - a selected row never blends toward hover. Code note: in light mode it looked dimmed under the pointer.
  - no selection ring (user request)
- Lines:
  - line 1: "project @ device", "~" when projectless, "?" when the space is missing
  - title: 13px/17
  - meta: 11px/14 (branch, PR badge), shown only when the Show toggles are on (`spaces.rs` `sidebar_chat_data`)
  - harness icon 13px
- Right corner:
  - time-ago, w30 at 11px
  - hovering the row swaps it for an Archive/Unarchive button (`shell.rs:7146-7160`). Only the corner click archives. Code note: "corner-only felt undiscoverable", so row hover reveals it.
- Status dot colors (`status_dot_color`, `Z crates/ui/src/shell/spaces.rs:2245`):

  | State | Color |
  |---|---|
  | Working | busy 0.55 + `mini_glyph_spinner` (`shell.rs:6981`) |
  | AwaitingInput | accent 0.6 |
  | Errored | danger 0.65 |
  | Completed | success 0.9 |
  | Idle | ink 0.14 |

- Status labels (`shell.rs:6964-6972`): "Working", "Input", "Failed", "Done", plus the send states "Failed"/"Queued".
- Status derivation (`Z crates/proto/src/view.rs:41-73`):
  - Working or AwaitingInput older than SESSION_STALE_MS = 45000 reads as none
  - otherwise live state wins, then unseen (`last_message_at > last_seen_at`, `Z crates/proto/src/entities.rs:246`) gives Completed, else Idle
  - seen is marked when the session is selected (`Z crates/ui/src/state.rs:2353-2363,2545`)
- Rows are draggable (`shell.rs:7292`). The drop zone "Drop here to unpin" is 48px. Escape cancels a drag (`shell.rs:9185`).
- Pocket comparison:
  - `session_row` is about 72px tall: px 10, py 10, gap 4; 12px provider + time line; 14px/20 semibold title; branch + status line (`P packages/desktop/crates/ui/src/ui.rs:428-451`)
  - Pocket's `status_label` shows "Running" for Working (`P packages/desktop/crates/ui/src/ui.rs:457`), which breaks CONTEXT vocabulary
  - Pocket marks sessions seen only while the window is active (`P packages/desktop/crates/pocket/src/main.rs:364-372`), which is stricter than Zeron's select-only rule and matches CONTEXT

### F4. Jump and cycle shortcuts
- JUMP_SLOTS 9, defaults mod-1..mod-9 (`Z crates/ui/src/settings.rs:997-1011`). `jump_to_session(slot)` takes the Nth id from the visible order and follows the same path as a click (`Z crates/ui/src/shell.rs:5204-5210`).
- `cycle_target` (`Z crates/ui/src/shell/tabs.rs:16-32`) wraps at both ends. With nothing selected it enters at the first row going forward, or the last going back. A selection that left the list is treated the same way.
- Bindings: ctrl-tab / ctrl-shift-tab on mac, mod-tab / mod-shift-tab elsewhere. Label: "Next session or right pane tab" (`settings.rs:1060-1130`). When the right column has focus, cycling walks its surfaces instead (`Z crates/ui/src/shell/navigation_focus.rs`).
- Hints (`Z crates/ui/src/settings.rs:1390-1407`):
  - they show only when the held modifiers exactly match a jump combo. Adding Shift or Alt hides them, so a chord like Cmd+Shift+4 never flashes them.
  - a combo with no modifier never shows hints
  - route must be Chat with no keyboard-owning overlay: palette, section dialog or menu, add-space, composer pickers (`Z crates/ui/src/shell.rs:5212-5228`)
- The same overlay guard also silences jump, cycle and archive shortcuts. Code note: GPUI runs a matched binding before `on_key_down`, so without the guard sessions would switch under an open popover.
- Chip (`shell.rs:7016-7040`): h16, px 4, radius 4, bg `text_muted` 0.08, text 0.85, mono 10 medium. It reuses the PR badge's styling deliberately ("Any other geometry reads as a second badge system"). In compact rows it takes the w30 time slot, or COMPACT_JUMP_HINT_WIDTH 42 when the label is longer than 3 chars (`shell.rs:7441-7459`). Rows 10 and beyond keep their time-ago (`Z crates/ui/src/shell/spaces.rs:4615-4621`).
- Pocket comparison:
  - no jump or cycle bindings (`P packages/desktop/crates/pocket/src/main.rs:1046-1056`)
  - `cmd-j` is "next Needs you" (`main.rs:934`), which Zeron lacks; Zeron uses mod-j to toggle its terminal drawer

### F5. Command palette
- Zeron (`Z crates/ui/src/shell/command_palette.rs`):
  - Actions (40-86):
    - "New chat"
    - "New project"
    - "Open settings"
    - "Switch to light theme" / "Switch to dark theme", which inverts the resolved appearance
  - The theme action keeps the palette open (199-205). Closing restores the previous focus.
  - Matching `matches_query` (67-70): lowercase, split on whitespace, every word must be a substring.
  - Chat haystack (146-175): `title project device branch #PR prTitle head base`.
    - Child (side) chats are excluded; archived ones are included.
    - The sidebar space filter is ignored.
    - Order follows the sidebar sort setting.
    - HISTORY_RESULT_LIMIT 30 applies after filtering and sorting (5).
  - Rows:
    - Action rows (270-294): min_h 30, py 4, radius 10, icon 16, query highlight on the label, right-aligned kbd hint read from the live keymap
    - Separator (my 8) between actions and history (247-249)
    - Chat rows reuse `render_chat_row` with the highlight
  - Empty state (360-364): "No results" / "Try a command, chat title, project, or device."
  - Keys (369-417):
    - Up/Down wrap by modulo
    - Enter goes through an EnterPress latch (X11 key-repeat guard)
    - Escape or an outside mouse-down closes
  - Geometry (438-550):
    - card width min(560, vw−32), radius 16, border, surface bg
    - header min_h 44, px 16, py 8, gap 10, 14px input, hairline bottom border at 0.06, "⌘K" badge
    - results max height clamp(vh−180, 100, 360), top/bottom fade band 18
    - footer px 16, py 7, gap 12, hairline top border
    - scrim 0.35, frosted blur 16, centered
  - Doc/code mismatches:
    - placeholder: code has "Search commands and chats…" (102); the screenshot shows "Type a command or search chats…"
    - footer: code has "↑ ↓ Navigate", "↵ Select", "Esc Close"; the README says "Open"
- Pocket (`P packages/desktop/crates/pocket/src/overlay.rs:60-230`):
  - sections: Sessions, top 5 by (status, recency), title substring only, Tab toggles all projects; Files (5); Actions
  - arrows clamp (151-156)
  - width 660, radius 22, top 120, header h60, 17px input, results max 460
  - empty copy "No matches."
  - footer "↑↓ navigate", "↵ open", "Tab to search all projects"
  - Pocket is ahead on file search and the Needs you jump. It is behind on multi-word matching and on haystack fields (project, worktree, branch, agent).

### F6. Menus, dialogs, popovers
- Session context menu (`Z crates/ui/src/shell.rs:9273-9500`), width 216, pages Root and Copy (`shell.rs:181`):
  - Root: "Rename…", "Pin"/"Unpin", "Archive", "Copy ▸", separator, "Delete…" (danger)
  - Side chats hide Pin, Archive and Copy.
  - Copy page: "Back", separator, "Path", "Zeron conversation link", the harness link label, "Harness session ID"
  - On right-column surface tabs it adds: "Close tab", "Close other tabs", "Close tabs to the left", "Close tabs to the right"
- Dialogs (`shell.rs:9541-9619`): "Rename session" with a Rename button; "Delete session?" with a Delete button. Dialog motion is DIALOG_IN 180ms.
- Popover tokens (`Z crates/ui/src/popover.rs:307-341,891-962,1127-1140`):
  - card: CARD_RADIUS 12, MENU_GAP 2, CARD_INSET 4, border, shadow_lg (none when frosted), 13px text
  - radii follow from the inset: MENU_ITEM_RADIUS = 12−1−4 = 7; PALETTE_ITEM_RADIUS = 14−4 = 10
  - menu_row: gap 10, px 8, py 6; active bg `card_selected_bg`; hover blend; keyboard highlight kept separate from pointer hover (`menu_row_nav`)
  - menu_heading: 10px medium uppercase, tracked, px 8, pt 6, pb 4
  - kbd_hint: px 5, py 1, radius 5, bg ink 0.05, mono 10
  - search input frame: mb 4, px 10, py 6
  - scrollbar thumb grows 3→5px, lingers 1400ms, fades 260ms
- Submenus use HoverIntent with a 300ms grace (`Z crates/ui/src/popover/hover_intent.rs:144`). A child menu flips left when `bounds.right + 244 > viewport` (`Z crates/ui/src/shell/spaces.rs:3552`).
- Pocket comparison:
  - row menu for projects and worktrees only: "Settings…", "Keep in Pocket", "Remove from Pocket", "Delete worktree…" (`P packages/desktop/crates/pocket/src/view.rs:389-420`)
  - the more menu has "New terminal tab", "Close session", "Open in editor", "Reveal in Finder" (`P packages/desktop/crates/pocket/src/overlay.rs`)
  - there is no right-click menu on sessions

### F7. Pins, sections, view options
- Pins:
  - persisted per profile in `sidebar_pinned_session_ids_by_profile` (`Z crates/ui/src/settings.rs:781`) and account-synced via the engine
  - writes are optimistic, serialized per engine connection, and marked unconfirmed after a 20s timeout (`Z crates/ui/src/shell/sidebar_pins.rs`)
  - notices: "Engine not connected. Sidebar was not changed.", "Waiting for the engine to confirm the previous sidebar change."
- Sections (`Z crates/ui/src/shell/sidebar_sections.rs`):
  - account-synced; local-only spaces store them per profile (1-32)
  - assigning a session to a section unpins it (58-80)
  - header (247-335): h28, px 8, gap 8, 12px `text_muted` 0.5, chevron; on hover a 20px "Section options" button
  - empty body: h40 "Drop sessions here" (354)
  - menu: "Edit section", "Archive all", "Delete" (463)
  - dialog: "New section"/"Edit section", "Group sessions however you like", placeholder "Section name", buttons "Cancel" and "Create section"/"Save"
- View options menu (`Z crates/ui/src/shell/spaces.rs:2051,3460-3767`):
  - Organize: "By device", "By project", "None"
  - Sort: "Last updated", "Created"
  - Show: "Branch", "Pull request", "Harness", "Project icon", "Location"
  - switch: "Compact" (3721)
  - action: "Create Section" (3767)
- Pocket has no pins or sections. Its closest control is `collapsed` project blocks in `desktop.json` (`P packages/desktop/crates/store/src/store.rs:17-27`).

### F8. Resize seams
- `resize_handle` (`Z crates/ui/src/shell.rs:9703-9790`; hitbox const `shell.rs:242`):
  - 20px hitbox with a 1px visible seam
  - hover shows a gradient highlight, suppressed while the width is clamped at a limit
  - double-click resets to the default and saves
- Edge feedback: RESIZE_EDGE_NUDGE 5px, RESIZE_EDGE_BOUNCE_MS 220, out fraction 0.32 (`Z crates/ui/src/motion.rs:424-426`).
- RESIZE 200ms ease-out drives sidebar, right-column and terminal-drawer open/close via `WidthTween` (`Z crates/ui/src/motion.rs:389`; `Z crates/ui/src/shell.rs:4235,5924-5937`), and is also used in `history.rs:1473,1817`.
- Pocket comparison: the 5px handle has no hover feedback, no reset and no persistence (`P packages/desktop/crates/pocket/src/view.rs:161-190`).

### F9. Notifications and sounds
- Settings (`Z crates/ui/src/settings/notifications.rs:190-389`):
  - Sounds: "Session sounds" master switch, then "Task completed", "Input required", "Errors and disconnections"
  - Desktop: "Desktop notifications", "Agent updates", "Only when in the background" ("Only notify when Zeron is in the background")
  - Defaults: all on, background-only true (`Z crates/ui/src/settings.rs:932-937`)
  - Env kill-switches `ZERON_DISABLE_SOUND` / `ZERON_DISABLE_NOTIFICATIONS`
- Triggers (`Z crates/ui/src/sound.rs:338-354`):
  - entering Errored plays Attention
  - entering AwaitingInput plays Request
  - a new completed turn on a fresh (≤45s) session with no pending send plays Done
  - suppressed pings are never replayed
  - connectivity degrading plays Attention once (`sound.rs:361-368`)
- Delivery (`Z crates/ui/src/notify.rs:1-25`):
  - macOS uses NSUserNotification through the ObjC runtime, with a delegate that forces presentation; the focus check happens at the call site
  - Linux uses `notify-send`; Windows is a no-op
- A click opens the target session (`Z crates/ui/src/lib.rs:278-283`).
- Pocket comparison: desktop banners already exist via GPUI `show_system_notification` for Needs you / Failed / Done transitions, excluding viewed sessions, with click-to-focus and auto-dismiss (`P packages/desktop/crates/pocket/src/main.rs:314-328,1103-1106`; `P packages/desktop/crates/pocket/src/status.rs:120-124`). No sounds, no settings toggles, no background-only option (rg finds no sound code in `P packages/desktop/crates`).

### F10. Other shell pieces (lower value for Pocket)
- Project icon (`Z crates/ui/src/shell/project_icon.rs`):
  - lookup order: `public/apple-touch-icon.png`, `apple-touch-icon.png`, `public/favicon.svg`, `favicon.svg`, `public/favicon.png`, `public/icon.png`, `public/logo.png`, … `assets/icon.png`
  - resolves the git root, including a linked `.git` file (worktrees)
  - fallback monogram from MONOGRAM_PALETTE: 8 (dark, light) pairs picked by FNV-1a hash of the seed
- Pocket's `repo_color` falls back to `PALETTE[position]` (`P packages/desktop/crates/pocket/src/view.rs:124-127`), so a project's color changes when projects reorder.
- Project actions (`Z crates/ui/src/shell/actions_ui.rs`, `Z crates/ui/src/project_actions.rs:242`):
  - named commands: "Run {name}", "Run automatically on worktree creation", "Import from zeron.json"
  - menu w280; control h24, radius 7
  - limits: name ≤80 chars, command ≤16384 bytes
  - the titlebar label hides below 420px titlebar width
  - Pocket already stores one `setup` command and a `copy` list per project (`P packages/desktop/crates/store/src/store.rs:8-15`)
- Transcript rail (`Z crates/ui/src/rail.rs`): prompt minimap, hidden below 768px, tick slot 10, at most 12 ticks.
- Side chats (`Z crates/ui/src/shell/side_chats.rs`): a fork through the latest completed response, opened in the right column. Error: "Start a conversation before creating a side chat." This is agent-transcript specific and has no Pocket equivalent; Pocket sessions are terminals.
- Keymap (`Z crates/ui/src/settings.rs:1019-1130`, groups at `Z crates/ui/src/settings/shortcuts.rs:482-510`): user-rebindable with a recorder. mod-k is fixed (`Z crates/ui/src/shell.rs:488`). Escape stopping the active agent is opt-in, default false (`settings.rs:945`).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 04-1 | Stable recency order in session lists; status only drives the mark | Rows don't jump under the pointer; spatial memory holds | `Z crates/proto/src/view.rs:89-103`; `P packages/desktop/crates/pocket/src/view.rs:661,481`, `overlay.rs:91` | pocket crate `view.rs`/`overlay.rs`: adapt (drop status keys from sort; keep `cards()` recency `main.rs:471`) | S | Product call vs CONTEXT urgency order (see open questions) |
| 04-2 | Cmd+1..9 jump to the Nth visible session, with hint chips while Cmd is held | Two keystrokes to any session; the chips teach it | `Z crates/ui/src/shell.rs:5204-5254,7016-7040`; `Z crates/ui/src/settings.rs:997-1011,1390-1407` | pocket `main.rs` keys + `on_modifiers_changed`; ui crate `session_row` corner slot: port | M | One `visible_order()` fn shared by render and keys |
| 04-3 | Ctrl+Tab / Ctrl+Shift+Tab cycles sessions with wrap | Fast back-and-forth without the mouse | `Z crates/ui/src/shell/tabs.rs:16-32` | pocket `main.rs`: port `cycle_target` verbatim | S | 04-2's visible order; terminal Ctrl+Tab passthrough decision |
| 04-4 | Palette matching and nav upgrade | Find a session by project, worktree, branch or agent, not only title | `Z crates/ui/src/shell/command_palette.rs:5,67-70,146-175,360-417` | pocket `overlay.rs:78-156`: adapt. Every-word match over title+project+worktree+branch+agent; cap after filtering; wrap arrows; highlight matches; empty hint copy | S | none |
| 04-5 | Session right-click menu: Rename…, Pin, Copy ▸ (Path, agent session ID, resume command), Close session… | Common per-session actions without opening the session | `Z crates/ui/src/shell.rs:181,9273-9619` | pocket `view.rs` RowMenu + `overlay.rs` confirm: new `RowMenu::Session` | M | Rename needs a title override in store or pocketd |
| 04-6 | Pinned sessions group at the top, drag to reorder | Long-running sessions stay reachable regardless of recency | `Z crates/ui/src/shell/sidebar_pins.rs`; `Z crates/ui/src/shell/spaces.rs:4208-4298` | store crate `Store.pinned: Vec<String>` + pocket `view.rs`: new | M | 04-1; decide desktop.json vs pocketd for phone parity |
| 04-7 | Persist window geometry (per display), column widths, layout mode; min window 900×600; debounced save | App reopens as left | `Z crates/ui/src/lib.rs:293-330,373`; `Z crates/ui/src/settings.rs:34-65,1527-1542` | store crate: add `window`, `widths`, `layout`; pocket `main.rs:1059`, `view.rs:183-190`: port | S | none |
| 04-8 | Resize seam feedback: 20px hitbox, 1px hover seam, clamp cue, double-click resets | Discoverable, forgiving resizing | `Z crates/ui/src/shell.rs:242,9703-9790`; `Z crates/ui/src/motion.rs:424-426` | pocket `view.rs:161-181` `resizable`: adapt | S | 04-7 for reset persistence |
| 04-9 | Sounds on Needs you / Failed / Done plus notification settings (master switches, per-event toggles). Banners with click-to-open already exist in Pocket | Hear state changes without watching; let users mute | `Z crates/ui/src/sound.rs:338-368`; `Z crates/ui/src/settings/notifications.rs:190-389`; `P packages/desktop/crates/pocket/src/main.rs:314-328` | pocket `main.rs` `sync_alerts` + `status::alerts`: extend with sound; store crate: toggles | S | Dedupe with phone push |
| 04-10 | Motion tokens + FLIP glide on reorder, 150ms hover fade, honor reduce-motion | Reorders read as movement, not teleporting | `Z crates/ui/src/motion.rs:17-20,299-426`; `Z crates/ui/src/shell.rs:885-894` | theme crate `motion` consts; ui crate rows: port | M | none: Pocket already uses `with_animation`, ease-out-quint and mirrors macOS reduce-motion (`P packages/desktop/crates/pocket/src/main.rs:1027-1029`, `overlay.rs:311-328`, `mermaid.rs:238`) |
| 04-11 | Titlebar names the selected session (agent icon + title + muted project · worktree) | Constant "where am I" without scanning the list | `Z crates/ui/src/shell/tabs.rs:1-5,282` | pocket `view.rs` `page_bar`: adapt | S | none |
| 04-12 | Stable hashed monogram color + project icon lookup (favicon/apple-touch-icon) | Projects recognizable at a glance; colors don't shift on reorder | `Z crates/ui/src/shell/project_icon.rs`; `P packages/desktop/crates/pocket/src/view.rs:124-127` | pocket `view.rs` `repo_color`; ui crate `repo_tile`: port | S | none |
| 04-13 | Sidebar view options: organize (by project / flat), sort (updated / created), show toggles (branch, agent, worktree), compact 29px rows | Density control for many sessions | `Z crates/ui/src/shell/spaces.rs:2051,3460-3767`; `Z crates/ui/src/shell.rs:936-956` | pocket `view.rs`, store crate settings: new | M | 04-1 |
| 04-14 | Project actions: named commands, run in a new terminal, optional run on worktree creation | Dev server, tests or lint one click away per project | `Z crates/ui/src/shell/actions_ui.rs`; `Z crates/ui/src/project_actions.rs:242` | store `RepoConfig.setup` → `actions: Vec<Action>`; pocket `main.rs:156` setup runner: adapt | L | Terminal spawn API in the workspace crate |
| 04-15 | Custom sidebar sections (drag sessions into named groups) | User-defined grouping | `Z crates/ui/src/shell/sidebar_sections.rs` | store + pocket `view.rs`: new | L | 04-6; unclear demand given project/worktree grouping |

## UI/UX spec to copy

### Layout
- Window: min 900×600. Restore saved bounds on the saved display (by UUID), else center 1320×880 (Pocket may keep 1440×900). Traffic lights at (14,14).
- Titlebar 38, top pad 4, header 44, status strip 24.
- Sidebar: 224-400, default 256. The filter row sits outside the scroll area: gap 4, px 8, pt 8, pb 4. List px 8, pt 4, gap 2. Footer p 8, buttons 28, avatar 16.
- Resize: 20px hitbox, 1px seam. Hover shows a gradient highlight unless clamped. Double-click resets and saves. Hitting a limit nudges 5px and bounces 220ms.

### Session row
- Height:
  - 45 (no meta)
  - 61 (branch) or 63 (PR)
  - −16 when the project line is hidden
  - compact 29
- Box: radius 8, px 8, py 6, gap 2.
- Text:
  - title 13/17; meta 11/14; agent icon 13; right corner w30 at 11px
  - rest text 0.8; archived 0.55; selected 1.0
- States:
  - selected: wash only, no ring, never blends toward hover
  - hover: 150ms blend toward the hover wash
  - hover on a row swaps the corner for a quick action (Pocket: close/menu)
- Status marks, in Zeron's scale:

  | Status | Mark |
  |---|---|
  | Working | busy 0.55 + mini spinner |
  | Needs you | accent 0.6 |
  | Failed | danger 0.65 |
  | Done | success 0.9 |
  | Idle | ink 0.14 |

  Pocket should map this onto its own tokens.
- Jump chip: h16, px 4, radius 4, bg `text_muted` 0.08, text 0.85, mono 10 medium, label "⌘1".."⌘9". Rows 10 and beyond keep the time.

### Command palette
- Card: width min(560, vw−32), radius 16, 1px border, scrim 0.35, blur 16 if available, centered.
- Header: min_h 44, px 16, py 8, gap 10, 14px input, bottom hairline 0.06, right-side "⌘K" badge.
- Results: max height clamp(vh−180, 100, 360), 18px edge fade band.
- Rows:
  - action rows: min_h 30, py 4, radius 10, icon 16, kbd hint right
  - my 8 separator between groups
- Footer: px 16, py 7, gap 12, top hairline. Hints: "↑ ↓ Navigate", "↵ Select", "Esc Close". Hint label 10px muted, gap 5.
- Behavior:
  - every whitespace word must be a case-insensitive substring of the haystack
  - cap (30) applies after filtering
  - Up/Down wrap
  - Enter latch against key-repeat
  - Escape or outside click closes and restores previous focus
  - a toggle action (theme) keeps it open
- Empty: "No results" / "Try a command, chat title, project, or device." For Pocket: "Try a session title, project, worktree, or branch."

### Menus and popovers
- Card: radius 12, inset 4, item gap 2, border, shadow_lg, 13px. Item radius 7 = 12−1−4.
- Row: gap 10, px 8, py 6. Keyboard highlight is distinct from pointer hover.
- Heading: 10px medium uppercase, tracked, px 8, pt 6, pb 4.
- kbd hint: px 5, py 1, radius 5, bg ink 0.05, mono 10.
- Submenu: hover-intent grace 300ms. Flip left if `right + 244 > viewport`. Paged submenus start with "Back" + separator.
- Session menu width 216. Order: Rename…, Pin/Unpin, Copy ▸, separator, destructive item last in danger color, with a confirm dialog.

### Motion (all honor reduce-motion)

| Token | Duration | Curve |
|---|---|---|
| HOVER_FADE | 150 | (0.4,0,0.2,1) |
| FADE_QUICK | 150 | ease |
| MENU_IN | 140 (scale ≈0.96, translateY −2) | ease |
| MENU_OUT | 100 | ease |
| DIALOG_IN | 180 (scale ≈0.96→1) | ease |
| COLLAPSE | 180 | ease-out |
| CHEVRON | 200 | ease |
| RESIZE | 200 (sidebar/right column/terminal toggles, history) | ease-out |
| TAB_SLIDE | 150 | ease-out |
| RESORT (FLIP) | 260 | (0.22,1,0.36,1) |
| NEW_THREAD_TRANSITION | 420 | (0.22,1,0.36,1) |
| Tooltip delay | 350 | — |
| Badge hover delay | 280 | — |
| Scrollbar | thumb 3→5px, linger 1400, fade 260 | — |

Refs: `Z crates/ui/src/motion.rs:299-426`, `Z crates/ui/src/badges.rs`.

### Shortcuts (Zeron defaults; "mod" = Cmd on mac)

| Combo | Action | Pocket today |
|---|---|---|
| mod-k | Palette (fixed) | cmd-k same |
| mod-, | Settings | cmd-, = project settings |
| mod-n | New session | cmd-n same |
| mod-shift-n | New project | cmd-shift-n = new worktree |
| mod-1..9 | Jump to session N | none |
| ctrl-tab / ctrl-shift-tab | Next / previous session | none |
| mod-shift-a | Archive session | n/a |
| mod-b / mod-r / mod-e | Toggle left sidebar / right column / files | cmd-\ rail, cmd-. layout cycle |
| mod-j | Toggle terminal drawer | cmd-j = next Needs you (keep) |
| mod-/ | Model picker | n/a |

Refs: `Z crates/ui/src/settings.rs:1060-1130`, `Z crates/ui/src/shell.rs:488` (mod-k), `Z crates/ui/src/app_menus.rs:147` (mod-,), `P packages/desktop/crates/pocket/src/main.rs:1046-1057`.

### Copy to reuse (Pocket wording in brackets)
- "Rename…", "Pin" / "Unpin", "Copy", "Back", "Path", "Delete…" ["Close session…"], "Rename session", "Drop here to unpin".
- "No results" / "Try a command, chat title, project, or device." ["Try a session title, project, worktree, or branch."]
- "Sidebar view options", "Organize", "Sort", "Show", "Last updated", "Created", "Compact".
- Notification settings: "Task completed" ["Done"], "Input required" ["Needs you"], "Errors and disconnections", "Only when in the background".

## Open questions / risks

- **Recency vs urgency order.** CONTEXT defines the status order Needs you > Done > Working > Idle. Is that a list sort or only a roll-up precedence? Zeron's user report says status sorting moved rows on open (Done → seen → Idle). Pocket's `view.rs:661` has the same failure mode. Needs a product call: keep urgency in the rail and roll-ups, recency in lists?
- **Device dimension.** Zeron groups by device and tags rows "@ device". Pocket is single-host, so drop the device parts.
- **Where pins and sections live.** `desktop.json` is desktop-only; pocketd would give phone parity. Zeron needed an optimistic queue with a 20s unconfirmed timeout for synced writes.
- **Rename.** Pocket session titles come from the agent or terminal. A user title override needs a store field or pocketd support, plus a rule for which wins.
- **Archive.** Zeron archives persistent chats. Pocket sessions end with their terminal, so Archive maps to Close session, and the archived list has no equivalent.
- **Key conflicts inside the terminal.** Ctrl+Tab and Cmd+1..9 may be wanted by TUI agents or the terminal itself. Holding Cmd for chips flashes hints on every Cmd chord unless the exact-modifier-match rule is copied.
- **Frosted blur.** Pocket's GPUI rev may lack backdrop blur; the palette scrim then needs an opaque fallback.
- **Notification duplication.** Pocket's existing desktop banners plus phone push could double-ping; adding sounds widens this. The desktop should probably skip when the phone timeline is open, and vice versa.
- **Vocabulary drift in Pocket today.** `P packages/desktop/crates/ui/src/ui.rs:457` renders "Running" for Working, and the column header reads "Workspace" (`P packages/desktop/crates/pocket/src/view.rs` `column_view`). Both break CONTEXT; this report changes neither.
- **Doc vs code in Zeron** (code wins in each):
  - the screenshot shows a session strip, but tabs.rs removed it
  - palette placeholder: "Type a command…" (screenshot) vs "Search commands and chats…" (code)
  - palette footer: "Open" (README) vs "Select" (code)
  - "By project" is described as normalizing to a flat list, but code has 3 organize modes
- **Zeron's seen rule is weaker than Pocket's.** Zeron marks seen on select; Pocket requires an active window. Keep Pocket's rule when porting 04-9 and 04-1.

## Verification

Date: 2026-09-30. Claims checked: 22. Corrected: 7.

Confirmed against source: tab strip removal and `cycle_target` wrap (`tabs.rs:1-32`); recency-only sort (`proto/src/view.rs:89-103`); jump slots, defaults, exact-modifier hint rule and overlay guard (`settings.rs:997-1011,1096-1128,1390-1407`, `shell.rs:5204-5254`); palette every-word match, haystack, cap-after-filter, wrap, copy, 560 width, 0.35 scrim (`command_palette.rs`); session menu items and width 216; geometry restore, 1320×880, 900×600 min, traffic lights, X11 note (`lib.rs:293-398`); column limits and 400ms debounce (`settings.rs:34-60`); motion tokens incl. RESORT 260 (`shell.rs:888`); sound triggers and notification defaults; status dot colors; row heights; theme sizes; popover radii; hover-intent 300ms; Pocket palette, keybindings, 1440×900 window, 5px handle and in-memory widths, row menus, `session_row`/"Running", store fields, `repo_color` fallback, no pins/jump/min size.

Corrections:
- Pocket already has desktop notifications (Needs you/Failed/Done, skip viewed, click focuses, auto-dismiss; `P main.rs:314-328,1103-1106`, `status.rs:120-124`). Removed "Pocket desktop has none"; 04-9 rescoped to sounds + settings.
- Pocket palette caps at 5 after filtering and sorting, not "before any ranking"; citation fixed to `overlay.rs:79-94,155-156`.
- Zeron RESIZE 200ms does animate sidebar/right column/terminal toggles via `WidthTween` (`shell.rs:4235,5924-5937`); removed "only history.rs uses it / toggles don't animate" (F8 and motion table).
- Resize handle citation `shell.rs:324-339` pointed at titlebar cluster code; now `shell.rs:9703-9790`.
- 04-10 prerequisite "GPUI animation support in Pocket's GPUI rev" dropped: Pocket already uses `with_animation` and mirrors reduce-motion (`P main.rs:1027-1029`, `overlay.rs:311-328`).
- Notification click citation `lib.rs:286-288` → `lib.rs:278-283`.
- Shortcut refs: added `shell.rs:488` (mod-k) and `app_menus.rs:147` (mod-, is not in `settings.rs` defaults); Pocket bindings range `main.rs:1046-1057`.
