# 16. Terminal surface: selection, scrollback, tabs, dock (Zeron GPUI terminal, MonoCode xterm)

Date: 2026-09-30

Sources:
- Zeron: zeronsh/comet@ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (MIT), https://zeron.sh. Releases: https://github.com/zeronsh/comet/releases/tag/v0.2.99 ("Fix slow trackpad scrolling in terminals", PR 615; "Prevent terminal overlap with the composer", PR 620) and v0.2.91 (projectless-terminal regression coverage, PR 457).
- MonoCode: hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6 (MIT), https://usemono.dev. No release note mentions the terminal.
- Pocket: worktree `orchestrator-research`@b9d14a19cefd1805656e0aecb9446f3349c57346.
- libghostty-vt: ghostty-org/ghostty@4ae9f1a2de5484de3d6a13fe03676b8853b9c41c (https://github.com/ghostty-org/ghostty/tree/4ae9f1a2de5484de3d6a13fe03676b8853b9c41c). This is the commit pinned by `P scripts/build-ghostty.sh:4`, read from the main checkout's `third_party/ghostty`. Its `include/ghostty` is identical to `zig-out/include/ghostty`.

Legend. `Z path:L` = Zeron clone, `M path:L` = MonoCode clone, `P path:L` = Pocket worktree, `G path:L` = ghostty repo at the pin; all paths are repo-relative. Shorthands:
- Z `t/` = `crates/ui/src/terminal/`.
- M `ft/` = `src/features/terminal/`.
- P `d/` = `packages/desktop/crates/`, `pd/` = `packages/pocketd/internal/`, `app/` = `packages/app/src/`.
- G `h/` = `include/ghostty/vt/`, `zt/` = `src/terminal/`.

`NN-k` is idea k of report NN. "code≠doc" marks code and docs disagreeing; the code wins. "Ghostty gives" means the pinned C API already exposes the behaviour. "Rust builds" means Pocket must write it.

## TL;DR

- Pocket's terminal is a bottom-anchored grid of StyledText rows and nothing else. It has no scrollback view, selection, copy, paste, links, mouse reporting, scrollbar or search (`P d/pocket/src/termview.rs:41-89`, `P d/pocket/src/view.rs:1070`).
- Most of the missing work is already done inside libghostty-vt at the pin. The shim just doesn't wrap it:
  - viewport scroll and scrollbar
  - a selection gesture state machine with per-cell `SELECTED`
  - OSC 8 URIs
  - mouse, paste and focus encoders
  - search
  - title, bell, OSC 52 and notification callbacks
  - cursor style and blink

  Rust builds the event plumbing, timers and widgets.
- The history already reaches the desktop. pocketd's attach snapshot formats the whole screen from the top of scrollback (`G zt/formatter.zig:888`, `P pd/vt/vt.go:96`). What's missing is scrolling the local viewport. Two catches:
  - Both VTs keep libghostty's default of about one page of scrollback (`G zt/Terminal.zig:277-284`).
  - The snapshot carries only the active screen, so primary history is lost if you re-attach while a TUI holds the alt screen (`G zt/formatter.zig:486`).
- The Zeron numbers to copy:
  - Font and grid: 13px text on an 18px line, 12px padding, ligatures off.
  - Clicks: 1/2/3 select char/word/line; Shift+click extends; a 2px drag threshold.
  - Edge autoscroll: a 24ms tick moving 1-3 lines.
  - Wheel: fractional remainder reset on TouchPhase::Started.
  - Scrollbar: 3→5px thumb, 24px minimum, 1400ms linger, 260ms fade.
  - Tabs: 118×28 in a 40px bar, drag reorder with a 150ms slide, middle-click close, exited tabs at 0.55 opacity.
  - Dock: 160px minimum, 55% of the viewport maximum, 280px default.
- The Zeron and MonoCode terminals are unequal: xterm.js (MonoCode) gets mouse modes, IME and bracketed paste for free. Zeron's alacritty port has no mouse reporting, IME, links or search.
- Pocket is ahead of Zeron on IME wiring (it has `EntityInputHandler`; xterm.js handles IME for MonoCode) but paints no preedit and returns no candidate bounds (`P d/pocket/src/main.rs:996-1043`).
- Likely live bug: Pocket paints Geist Mono with ligatures on, the exact cause of Zeron's `codex --yolo → codex--yolo` render bug (`Z t/view.rs:430-442`).
- Key clashes: Pocket's ⌘J is NextWaiting and ⌘K is the palette (`P d/pocket/src/main.rs:1062-1071`), where Zeron uses ⌘J for the terminal toggle and MonoCode uses ⌘K to clear. Keep Pocket's bindings.
- Phone: ship a read-only terminal first. pocketd renders styled rows from its own VT render state and pushes them over a new WS `terminal.watch`. Don't put a VT on the phone, and never resize the PTY from the phone. Raw input (a key bar) comes second. This refines 14-17 and wires the dead "Open raw terminal" button (`P app/screens/ChatScreen.tsx:155-159`).
- Must-dos, all S except 16-4 (M):
  - 16-1: scrollback wrap plus a 10k-line cap
  - 16-2: wheel handling
  - 16-3: ligatures off
  - 16-4: selection
  - 16-5: copy
  - 16-6: paste

  Report 06 called the Zeron terminal "little to copy" (`docs/orchestrators/research/06-zeron-git-files-ux.md:243`). This report contradicts that.

## Findings

### F1. Pocket baseline (the numbers to change)

- **Metrics.**
  - `MAIN` is 13px text on a 22px line; `SMALL` is 12/19 (`P d/pocket/src/termview.rs:11-12`).
  - Cell width is `advance('m')` (`:21`). cols = ⌊w/cell⌋ and rows = ⌊h/line⌋, both at least 1 (`:22-23`).
  - Body padding is top 10, sides 20, bottom 14 (`P d/pocket/src/view.rs:1077-1079`).
  - Split rows after the first are a fixed 250px (`P d/pocket/src/view.rs:999`).
- **Paint.**
  - Rows are StyledText in a `justify_end` column.
  - The cursor is an inverted cell; bold becomes SEMIBOLD; underline is 1px; wide spacer tails are skipped (`P d/pocket/src/termview.rs:41-89`, `:54-55`).
  - No font features are set, so ligatures are on.
- **Shim.**
  - `pt_new` sets no options or callbacks (`P d/term/src/shim.c:32-43`).
  - `pt_frame` reads cols, rows, colours, cursor and per-cell cp/fg/bg/bold/italic/underline/inverse/wide (`:56-110`).
  - It ignores selection, cursor style, blink and dirty flags.
  - The Rust `Term` exposes only new/write/resize/app_cursor/frame (`P d/term/src/term.rs:41-72`).
- **Input.**
  - `key_bytes` covers enter, bs, esc, tab, arrows/home/end per DECCKM, del, pgup/pgdn, F1-F12, ctrl-letters and alt-ESC (`P d/keys/src/keys.rs`).
  - ⌘C returns None and is asserted by a test (`:113`).
  - Text goes in via `replace_text_in_range`; marked text is stored as a length only; `bounds_for_range` returns None (`P d/pocket/src/main.rs:996-1043`).
  - Nothing implements terminal copy or paste. `clipboard` appears only in `explore.rs`.
- **Mouse.** Mouse-down only focuses the pane (`P d/pocket/src/view.rs:1070`). There is no wheel handler.
- **Resize.** `fit` sends a `resize` op on every size change, with no debounce (`P d/pocket/src/main.rs:742-751`). The local Term resizes only when the server echoes it (`P d/pocket/src/sessions.rs:44-57`).
- **Tab bar.**
  - Bar h40, gap 2. Tab h28, pl10 pr6, r7, 12.5px text.
  - Selected tab: WHITE background, row shadow, SEMIBOLD. Inactive: TEXT_2 at MEDIUM weight, FILL_2 on hover.
  - Label max width 150 (`:800`).
  - Close button: 20px, r5, hidden until group hover.
  - Plus: 28px r7. Chevron: 20×28, opens the tab menu (`P d/pocket/src/view.rs:826-923`).
  - No drag reorder and no middle-click. `on_drag` exists only for columns and projects (`:172`, `:325`).
- **Pipeline.**
  - pocketd owns the PTY and an authoritative VT. `WRITE_PTY` is set, and it answers terminal queries unless a real TTY is attached (`P pd/vt/vt.go:15-20`, `P pd/terminal/terminal.go:180-189`).
  - The pump reads 32KiB at a time and broadcasts `output` (`P pd/terminal/terminal.go:191-216`).
  - Attach returns `vt.Snapshot()` (`:251-266`).
  - There is one size per PTY (`:289-299`).
  - The desktop rebuilds its local `Term` on every `snapshot` (`P d/pocket/src/sessions.rs:44-57`).
- **Snapshot contents.** The VT snapshot enables `modes`, `keyboard`, `screen.cursor` and `screen.style`. It leaves out `palette`, `scrolling_region`, `tabstops`, `pwd`, `screen.hyperlink`, `kitty_keyboard` and `charsets` (`P pd/vt/vt.go:22-40` vs `G h/formatter.h:41-94`).
- code≠doc: `Plain()` is documented as "visible text" but formats the whole screen, including all scrollback (`P pd/vt/vt.go:93-94`, `G zt/formatter.zig:888`, `G zt/point.zig:38-44`).

### F2. libghostty-vt capability matrix (pin 4ae9f1a)

| Behaviour | Ghostty gives | Rust builds |
|---|---|---|
| Scrollback retention | Defaults are `max_scrollback_bytes = 10_000` and `max_scrollback_lines = null` (`G zt/Terminal.zig:277-284`), and the C `new` keeps those defaults (`G src/terminal/c/terminal.zig:841-848`). Pruning is page-granular. A page is about 400KB, and at least one standard page is kept (`G h/terminal.h:1480-1523`); the standard page is 215×215 (`G zt/page.zig:1896-1901`). Options `OPT_SCROLLBACK_MAX_BYTES`/`MAX_LINES` exist. | Set `MAX_LINES = 10_000` (Zeron parity, `Z t/emulator.rs:44`) and `MAX_BYTES = NULL` in both `shim.c` and `vt.go`. |
| Viewport scroll | `ghostty_terminal_scroll_viewport` with TOP/BOTTOM/DELTA (negative is up)/ROW (`G h/terminal.h:219-265`, `:2384`). `DATA_VIEWPORT_ACTIVE` (`:2016`), `DATA_TOTAL_ROWS` (`:1842`), `DATA_SCROLLBACK_ROWS` (`:1849`). | Wheel mapping, remainder, snap-to-bottom on input. |
| Scrollbar model | `GhosttyTerminalScrollbar{total, offset, len}` via `DATA_SCROLLBAR` (`G h/terminal.h:313-329`, `:1793`). There is no change callback, so poll it each frame (`:1786-1789`). | The rail widget, fade and drag. |
| Selection | Gesture state machine: PRESS/RELEASE/DRAG/AUTOSCROLL_TICK/DEEP_PRESS (`G h/selection.h:22-50`, `:453-469`). Behaviours CELL/WORD/LINE/OUTPUT, default cell/word/line by click count (`:345-358`). Options include time, repeat interval, word boundaries, rectangle, geometry and viewport (`:480-536`). Autoscroll NONE/UP/DOWN (`:401-411`). Helpers select word/line/all/output, adjust, contains (`:550-1074`). Install with `OPT_SELECTION` (`G h/terminal.h:1415-1426`). Render reports `ROW_DATA_SELECTION` and `ROW_CELLS_DATA_SELECTED` (`G h/render.h:270`, `:302-320`, `:791`). Cell threshold is 60% of cell width (`G zt/SelectionGesture.zig:728-732`). | GPUI mouse→event feed; the autoscroll timer (Ghostty says about 15ms, `G zt/SelectionGesture.zig:120-122`; the edge zone is only 1px, `:155-157`, `:370-375`; 1 row per tick, `:447-480`); Shift+click extend (not in the gesture code); highlight paint. |
| Copy text | `ghostty_terminal_selection_format_buf`/`_alloc` (`G h/selection.h:893`, `:928`). For copy parity use PLAIN with unwrap+trim (`G h/selection.h:229-231`). | ⌘C binding and clipboard write. |
| Paste | `ghostty_paste_encode(data, len, bracketed, …)` replaces NUL/ESC/DEL etc. with spaces, wraps in 200~/201~ when bracketed, and turns `\n`→`\r` otherwise (`G h/paste.h:241`). `ghostty_paste_is_safe` flags newlines or `\x1b[201~` (`:209`). `ghostty_terminal_paste` (`:187`) needs `WRITE_PTY`, so don't use it on the desktop VT. Mode 2004 (`G h/modes.h:91`). | ⌘V, a mode lookup through a new `pt_mode()`, an unsafe-paste confirm, and sending bytes over `daemon.input`. |
| Links | OSC 8: `CELL_DATA_HAS_HYPERLINK` (`G h/screen.h:196`), `ROW_DATA_HYPERLINK` (`:295`), `ghostty_grid_ref_hyperlink_uri` (`G h/grid_ref.h:167-191`). The VT formatter re-emits per-cell OSC 8 (`G zt/formatter.zig:1470-1490`), so links survive the snapshot. | Plain-URL/path detection (none in the headers), ⌘-hover underline, ⌘-click to open. |
| Mouse reporting | Tracking NONE/X10/NORMAL/BUTTON/ANY and formats X10/UTF8/SGR/URXVT/SGR_PIXELS (`G h/mouse/encoder.h:35-62`). Encoder options (`:112-125`), `setopt_from_terminal` (`:175`), `encode` (`:208`), buttons LEFT…ELEVEN (`G h/mouse/event.h:48-60`), `DATA_MOUSE_TRACKING` (`G h/terminal.h:1812`). Mode 1007 alt-scroll defaults on (`G zt/modes.zig:366`). | Routing: mouse tracking on → encode; otherwise alt screen with 1007 → arrows; otherwise scroll the viewport. Shift bypasses to selection. |
| Focus reports | `ghostty_focus_encode` (`G h/focus.h:64`) and mode 1004 (`G h/modes.h:78`). | Send on pane focus/blur when 1004 is set. |
| Keys | Full key encoder, kitty-aware (`G h/key/encoder.h:132-253`). | Optional: replace `key_bytes`. Out of scope here; 13-12 covers Mac keys. |
| Title / bell / OSC 52 / notify / pwd | Callbacks `BELL`, `TITLE_CHANGED`, `PWD_CHANGED`, `CLIPBOARD_WRITE` (OSC 52/1337/5522), `DESKTOP_NOTIFICATION` (OSC 9/777) (`G h/terminal.h:1238`, `:1263`, `:1465`, `:1477`, `:1532`). Data `DATA_TITLE`, `DATA_PWD` (`:1823`, `:1835`). | Wire them in pocketd (authoritative) rather than the desktop. |
| Cursor style | `DATA_CURSOR_VISUAL_STYLE` BAR/BLOCK/UNDERLINE/BLOCK_HOLLOW, `BLINKING`, `PASSWORD_INPUT` (`G h/render.h:144-153`, `:200-209`). | Paint the shape and a blink timer; hollow when unfocused. |
| Dirty tracking | Global and row dirty (`G h/render.h:127-133`, `:173`). | Optional repaint skip. |
| Search | Covers the active area plus scrollback of both screens. Matches come back as selections; VIEWPORT_MATCHES, SELECT_NEXT/PREV, SELECT_SCROLL; ASCII is case-insensitive (`G h/search.h:20-70`, `:177-298`). | ⌘F bar UI. |
| Full-state snapshot | Binary "GHOSTSNP" snapshot: CRC-protected, a READY marker, then incremental history, one entry per screen (`G h/snapshot.h:24-30`, `:76-90`). | Optional swap for the VT-stream snapshot (16-20). |
| IME | Nothing. | Preedit paint at the cursor; `bounds_for_range` returns the cursor cell. |
| Tabs, dock, resize debounce | Nothing. | All UI. |

### F3. Zeron terminal behaviour (alacritty_terminal 0.26 on a GPUI fork)

- **Stack.**
  - A custom GPUI Element over `alacritty_terminal` with 10_000 lines of scrollback (`Z t/emulator.rs:44`, `:181`).
  - code≠doc: Zeron's inventory says "xterm.js equivalent needed" (`Z docs/research/feature-inventory.md:99-104`); the code uses alacritty.
  - GPUI comes from the zeronsh/zui fork (`Z Cargo.toml:79`). Pocket is on gpui-kit 0.6.6 (`P packages/desktop/Cargo.toml:25`), so a port needs API adaptation.
- **Metrics.**
  - 13px text, 18px line, 12px padding (`Z t/view.rs:25-31`).
  - Input coalescing 12ms, resize debounce 80ms (`:34-36`).
  - cols = ⌊(w−24)/cell_w⌋, clamped to 2..500. `cell_w` is the em advance, falling back to 0.6×size (`Z t/view.rs:447-457`).
  - The user can set font size 8..32, default 13 (`Z crates/ui/src/typography.rs:125-133`).
- **Font.** `liga`, `calt` and `dlig` are all 0. The comment records the Geist Mono `--`/`->` ligature bug: the row paints short while the cursor stays on the true column (`Z t/view.rs:430-442`). Non-ASCII and wide glyphs are pinned per cell (`Z t/view.rs:633-744`).
- **Cursor.** A block the size of one cell: filled when focused, outlined otherwise (`Z t/view.rs:547-559`). Colour is text at alpha 0.40 (dark) / 0.55 (light) (`Z crates/theme/src/builtins.rs:139`). No blink, no DECSCUSR shapes.
- **Selection.**
  - Click count 1/2/≥3 → Simple/Semantic/Lines (`Z t/panel.rs:1105-1109`); Shift+click extends a Simple selection (`:1111-1116`); drag threshold 2.0px (`Z t/view.rs:126`).
  - Cell side comes from the half-cell split (`Z t/view.rs:150-191`).
  - Colour is border tone (white dark / black light) at alpha 0.22 dark / 0.16 light (`Z crates/theme/src/builtins.rs:110`, `:157`).
- **Edge autoscroll.**
  - 24ms tick (`Z t/panel.rs:45`, `:1236-1273`).
  - Edge zone = min(line_h, grid_h/3). Speed = round(1+2t²) lines, so 1-3 per tick, where t is penetration/edge clamped to 0..1 (`Z t/panel.rs:237-257`).
- **Wheel.**
  - `Lines` deltas are used as-is. `Pixels` deltas are divided by the measured line_h.
  - A fractional `scroll_remainder` is kept per tab and reset on `TouchPhase::Started`; the integer part is scrolled and the event stops propagating (`Z t/panel.rs:266`, `:1785-1812`).
  - It always scrolls scrollback: no mouse reporting, no alt-screen arrows. v0.2.99 fixed "slow trackpad scrolling".
- **Input.** A keypress while scrolled back snaps to the bottom (`Z t/panel.rs:893-895`). Positive scroll is up (`Z t/emulator.rs:254`).
- **Copy.** ⌘C or Ctrl+Shift+C copies the selection. The key is swallowed only if something was copied; otherwise ⌘C falls through (`Z t/panel.rs:946-985`, `:1210`).
- **Paste.** ⌘V or Ctrl+Shift+V. `paste_bytes` strips only `\x1b[201~` and wraps when DECSET 2004 is on (`Z t/view.rs:312-322`, `Z t/emulator.rs:211-268`). There is no control-char scrub and no multi-line confirm.
- **Title and bell.** The OSC title shows when set, otherwise "Terminal N" (`Z t/panel.rs:445`, `:670`). The bell is captured but only read in tests (`Z t/emulator.rs:229`, `:585-588`).
- **Exit.** `"\r\n\x1b[90m[process exited {code}]\x1b[0m\r\n"` is written into the grid, and the tab dims to 0.55 (`Z t/panel.rs:143-144`, `:1592`). Reconnect backoff is 500→8000ms (`:74-76`).
- **Tab bar.**
  - Bar h40 with a bottom hairline at 0.07, pl8 pr6 (`Z t/panel.rs:44`, `:1470-1483`).
  - Tab w118 h28 r8 (`:43`, `:1549-1558`).
  - Active tab: text colour on ink 0.08. Inactive: text_muted at 0.6 (`:1518-1521`).
  - Close button: 20px r6, shown only on the selected tab, hover ink 0.09, tooltip "Close terminal" (`:1536-1545`).
  - "+" tooltip "New terminal" (`:1650`). The chevron tooltip "Hide terminal" collapses the drawer (`:1657-1682`).
  - Middle-click closes (`:1573-1578`).
  - Drag reorder: a `TabGhost`, a drop index from `rel_x/TAB_WIDTH`, and siblings sliding by `slide_offset × 118` (`:297-319`, `:1492`, `:1605-1606`, reorder helpers `:79-140`). The slide is 150ms (`Z crates/ui/src/motion.rs:391`).
  - Closing the last tab closes the drawer.
- **Dock (bottom drawer).**
  - Height: minimum 160, maximum 0.55×viewport, absolute maximum 2000, default 280 (`Z crates/ui/src/settings.rs:53-56`, `Z t/panel.rs:64`, test `:1919`).
  - Toggle `mod-j` (`Z crates/ui/src/settings.rs:1106`, `Z t/panel.rs:50-57`) with a 200ms tween. Focus moves to the terminal on open and back to the composer on close (`Z crates/ui/src/shell.rs:4208-4244`, `Z crates/ui/src/motion.rs:389`).
  - Resize handle: a 1px line with a 10px hitbox; double-click resets to 280 (`Z crates/ui/src/shell.rs:249`, `:10270-10390`).
  - Same-frame geometry `{reserved_height, height, content_height, limit}` keeps the dock from overlapping the composer (`Z t/dock.rs`; PR 620).
- **Scrollbar (shared menu rail).**
  - Track inset 4, hit strip 10, thumb 3 resting / 5 hovered, minimum thumb 24.
  - Visible for 1400ms after the last scroll, then a 260ms fade repainted every 16ms (`Z crates/ui/src/popover.rs:1369-1384`).
  - Thumb height = track×viewport/content, at least 24 (`:1413-1419`).
  - Colour: text_faint at 0.5, 0.68 when active, 0.85 on hover (`:1770-1771`).
  - Shown only while dragging, hovering the rail, or inside the linger window.
- **Engine limits.**
  - At most 32 terminals, 64KiB input, 1MiB replay, 30min TTL.
  - Env `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Zeron` (limits `Z crates/engine/src/terminals.rs:33-36`, env `:280-282`).
  - 12ms output batching (`Z crates/doc/src/constants.rs:23`).
- **Gaps, confirmed with rg:**
  - no InputHandler/IME
  - no OSC 8 or URL links
  - no mouse modes
  - no search
  - no cursor blink

### F4. MonoCode terminal behaviour (xterm.js 6)

- **Options.**
  - `cursorBlink: true`, `cursorStyle: "bar"`, `fontSize: 13`, `lineHeight: 1`, `scrollback: 5000`, `smoothScrollDuration: 0`, `macOptionIsMeta` on Mac (`M ft/ui/TerminalView.tsx:156-168`).
  - Font stack at `:116-121`.
  - `@xterm/addon-fit` is listed but never imported; fitting is custom (`M package.json:58-59`, `M ft/ui/TerminalView.tsx:296-347`, `M ft/model/terminalLayout.ts`: 14px scrollbar reserve, TUI ceil-fit).
- **Colours.** Background transparent. Cursor is the accent colour, falling back to #4da3f5 dark / #4078f2 light. Selection is rgba(255,255,255,0.18) dark / rgba(0,0,0,0.18) light, 0.08 when inactive (`M ft/ui/TerminalView.tsx:102-114`).
- **Padding.** 8px 10px, 0 on the alt screen; the scrollbar is hidden on the alt screen (`M src/styles/index.css:2068-2106`).
- **Keys** (`M ft/ui/TerminalView.tsx:188-217`).
  - ⌘C with a selection copies; ⌘C without one is dropped, never sent as ^C. Ctrl+C is SIGINT.
  - ⌘V pastes through `term.paste`, which is xterm's own bracketed paste (`:173-186`).
  - ⌘K clears.
  - Mac editing keys are 13-12.
- **Wheel.** Passed to xterm only when mouse events are enabled or the buffer isn't the alternate one. So on an alt screen without mouse mode, wheel→arrows is suppressed (`M ft/ui/TerminalView.tsx:291-294`).
- **Title.** OSC 7 cwd (`:219-244`) plus a 1000ms `ps` poll for the tab title (`:378-412`, `M ft/model/terminalTab.ts`).
- **Colour queries.** Replies to OSC 10/11/12 (`M ft/ui/TerminalView.tsx:267-284`).
- **Not implemented** (rg finds no `linkHandler`, web-links, search, `onBell` or `onTitleChange`): link handling, search, bell.
- **PTY.** 32KiB reads, 8ms coalescing, `TERM_PROGRAM=MonoCode` (`M src-tauri/src/pty.rs:19`, `:22`, `:267-270`).
- **Dock.**
  - Four sides via "Move Terminal" → "Dock Bottom"/Top/Left/Right (`M ft/ui/ProjectTerminalDock.tsx:54`, `:228`).
  - Sash `h-1.5`/`w-1.5` (6px): content/10 on hover, /15 while dragging; double-click resets (`:170-203`).
  - Menu items `New Terminal (⌘\`)` and `Hide Terminal (⌘J)` (`:221`, `:239`).
  - Default sizes 220 (top/bottom) / 360 (left/right); minimum 88 / 180; maximum ⌊span×0.7⌋ (`M src/features/projects/model/projectTerminal.ts:31-38`, `:68`).
- **Tabs.**
  - Bar h36 (`h-9`). Tab w224 (`w-56`), minimum 112 (`min-w-28`). Inner h30, 13px text.
  - Middle-click via `onAuxClick` (`M src/features/workspace/ui/SurfaceTabs.tsx:268`, `:316-323`, `:364`).
  - Close button `size-5` (`:414`). Menu "Close"/"Close Others" (`:90`).
  - Reorder: drag activates at 5px, and the click is suppressed for duration+400ms (`M src/shared/hooks/useAnimatedReorder.ts:186`, `:226`).
  - Motion tokens: reorder 160ms, tab-close 200ms, ease-out cubic-bezier(0.22,1,0.36,1), with a reduced-motion path (`M src/styles/index.css:75-79`, `:172-230`).
- **Close confirm** is 13-13 (`M ft/model/terminalClose.ts`).

### F5. Behaviour spec for Pocket (decisions)

- **Grid.**
  - Keep 13px Geist Mono. Consider an 18px line (Zeron) instead of 22 so box-drawing TUIs (Claude Code, Codex) join up; this is an open question.
  - Turn `liga`/`calt`/`dlig` off.
  - Measure cells with the em advance, as today.
- **Scrollback.**
  - Set 10_000 lines and remove the byte cap, on both VTs.
  - Viewport follows the bottom unless the user has scrolled.
  - Any input (key, IME commit, paste) snaps to the bottom.
  - Output while scrolled back leaves the view where it is. Ghostty only follows when the viewport is active (`DATA_VIEWPORT_ACTIVE`).
- **Wheel and trackpad.**
  - Lines: Δy. Pixels: Δy/line_h.
  - Accumulate the remainder and reset it on `TouchPhase::Started`.
  - Route per event:
    1. Shift held → always scroll the viewport.
    2. Mouse tracking ≠ NONE → mouse encoder, wheel buttons, one event per whole line.
    3. Alt screen with mode 1007 → `key_bytes("up"/"down")` × n.
    4. Otherwise scroll the viewport.
- **Selection.**
  - Feed GPUI mouse events to the ghostty gesture: PRESS with click count timing from Ghostty, DRAG, RELEASE.
  - Geometry `{cols, cell_w, padding_left, screen_height}`.
  - Keep Zeron's 2px drag threshold before arming.
  - Shift+click extends via `ghostty_terminal_selection_adjust` (`G h/selection.h:960`), driven from Rust.
  - Edge autoscroll: Zeron's 24ms timer and 1-3 lines per tick. Ghostty's 1px zone is too thin for a pane with 20px padding, so Rust computes the edge and sends AUTOSCROLL_TICK or `scroll_viewport` DELTA.
  - Highlight: the per-cell `SELECTED` flag becomes a run background of text at 0.16 alpha (the light theme; `P d/theme/src/theme.rs` defines no dark palette).
  - A selection survives output. It is cleared on input and on a snapshot rebuild.
- **Copy.** ⌘C with a selection copies PLAIN with unwrap+trim and swallows the key. Without a selection ⌘C does nothing and is not sent (as in MonoCode). Ctrl+C stays ^C.
- **Paste.**
  - ⌘V: read the clipboard, then `ghostty_paste_encode(bracketed = mode 2004)`, then `daemon.input`.
  - If `!ghostty_paste_is_safe` and 2004 is off, confirm with "Paste N lines into {title}?" and buttons "Paste" / "Cancel".
  - Right-click → Copy/Paste menu is optional.
- **Links.**
  - With ⌘ held, hovering a cell that has an OSC 8 URI, or matches `https?://…`, underlines the run in ACCENT and shows a pointer cursor.
  - ⌘-click opens it with the system handler; a `file:line` path opens in explore (report 06).
  - A plain click stays a selection.
- **Mouse reporting.** Press, release, drag and motion go through the ghostty mouse encoder when tracking is on and Shift is not held. Encoder options come from the terminal (`setopt_from_terminal`).
- **IME.**
  - Paint the marked text underlined at the cursor cell, in cursor-cell bounds.
  - `bounds_for_range` returns the cursor cell rect so the candidate window anchors.
  - Commit clears the preedit.
- **Cursor.** Read the visual style. Block, bar (2px) and underline (2px) are filled when focused. Unfocused, show BLOCK_HOLLOW with a 1px outline. Blink 530ms on/off when `BLINKING` is set; my choice, since Ghostty doesn't specify the interval. Honour reduced motion by not blinking.
- **Resize.** Debounce `fit` 80ms trailing (Zeron). Suspend while a split drag is live.
- **Bell and title.** Title: OSC title when set, otherwise the foreground command, otherwise "Terminal". The bell adds a tab dot until the tab is seen; there is no sound.

### F6. Phone terminal view (recommendation)

- **Today.**
  - The phone WS has no terminal methods (`P pd/wsserver/wsserver.go:154-212`).
  - The agent summary carries `terminalId` (`P pd/proto/proto.go:170`, filled at `P pd/agent/agent.go:168`).
  - The "Open raw terminal" button has no `onPress` (`P app/screens/ChatScreen.tsx:155-159`).
  - The app has no webview and no VT; it has Geist Mono (`P packages/app/package.json:14-31`, `P app/design.ts:32`).
- **Pick: server-rendered read-only grid.** pocketd already holds the authoritative VT.
  - Add `vt.Frame()` in `pd/vt/vt.go`, a cgo port of `pt_frame` (`P d/term/src/shim.c:56-110`) that emits run-merged rows: `{t, fg?, bg?, b, i, u, inv}` plus cursor, cols and rows.
  - Add WS `terminal.watch {terminalId}` → `terminal.frame` push. Throttle it to at most 4Hz and only when dirty (my choice; Ghostty has dirty state, `G h/render.h:127-133`). Add `terminal.unwatch`.
  - History, as a "log" mode: `terminal.history {before, lines}`. Use a PLAIN formatter with unwrap, restricted by a selection range (`G h/formatter.h:107-118`), so text reflows to the phone width and stays selectable with native text selection.
  - Don't use `Plain()` as-is. It returns the whole history, up to 10k lines after 16-1.
- **Rendering.**
  - Grid mode renders at the PTY's cols. The font size fits cols to width with a 7pt floor; below that the view pans horizontally. Pinch zooms.
  - Default mode: log for shells, grid when the alt screen is active or the program uses DECSTBM.
- **Never resize from the phone.** The PTY has one size, and resizing would reflow the desktop session (`P pd/terminal/terminal.go:289-299`).
- **Phase 2: input.**
  - Key bar: Esc, Tab, Ctrl (sticky), ←↑↓→, Enter, ^C.
  - A text field sends committed text.
  - WS `terminal.input` reuses `Terminal.Write` (`P pd/terminal/terminal.go:268-274`) and inherits the OnInput hook.
  - Needs a paired-device trust check before enabling, because raw input can type into any shell.
- **Rejected.**
  - libghostty-vt on the phone: native RN modules for iOS and Android plus a wasm path (`G h/wasm.h`) make it XL, and Hermes wasm support is unverified.
  - An xterm.js webview adds a dependency the app avoids.
- **Relation to 14-17.** 14-17 planned a raw attach over WS. Shipping VT bytes would force a VT onto the phone, so 16-13 replaces the transport with frames and keeps 14-17's goal.

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prereqs |
|---|---|---|---|---|---|---|
| 16-1 | Scrollback: 10k-line cap on both VTs; shim wraps `scroll_viewport` and `DATA_SCROLLBAR`; `Frame` gains `scrollbar{total,offset,len}` | Read what an agent printed before the last screen | `G h/terminal.h:1502-1523`, `:2384`, `:313-329`; `G zt/Terminal.zig:277-284`; `Z t/emulator.rs:44` | `P d/term/src/shim.c`, `P d/term/src/term.rs` (adapt); `P pd/vt/vt.go` `vt_new` (adapt) | S | None |
| 16-2 | Wheel/trackpad: remainder accumulation, TouchPhase reset, snap-to-bottom on input, Shift/mouse-mode/alt-1007 routing | Smooth trackpad; TUIs still get arrows | `Z t/panel.rs:1785-1812`, `:893-895`; `M ft/ui/TerminalView.tsx:291-294`; `G zt/modes.zig:366` | `P d/pocket/src/termview.rs` `on_scroll_wheel` (new); `P d/pocket/src/main.rs` input paths (adapt) | S | 16-1; `pt_mode()` |
| 16-3 | Ligatures off (`liga`/`calt`/`dlig` = 0) | Commands render with their real spacing (`--yolo`) | `Z t/view.rs:430-442` | `P d/pocket/src/termview.rs` font in `surface`/`screen` (adapt) | S | Confirm the bug first by typing `a --b` |
| 16-4 | Selection: ghostty gesture (1/2/3 clicks), 2px arm, Shift+click extend, 24ms edge autoscroll of 1-3 lines, highlight via `SELECTED` | Select agent output like any Mac terminal | `G h/selection.h:22-536`, `G h/render.h:791`; `Z t/panel.rs:237-257`, `:1105-1116`, `Z t/view.rs:126` | `P d/term/src/shim.c` + `term.rs` (selected flag, gesture wrap); `P d/pocket/src/termview.rs` mouse handlers; `P d/pocket/src/main.rs` state (new) | M | 16-1 |
| 16-5 | ⌘C copies PLAIN unwrap+trim; swallowed only with a selection, never sent as ^C | Copy without killing the running agent | `G h/selection.h:229-231`; `Z t/panel.rs:946-985`; `M ft/ui/TerminalView.tsx:188-217` | `P d/pocket/src/main.rs` action; `P d/keys/src/keys.rs` stays None (adapt) | S | 16-4 |
| 16-6 | ⌘V paste via `ghostty_paste_encode` using mode 2004; unsafe multi-line confirm | Safe pastes; no accidental command runs | `G h/paste.h:209`, `:241`; `Z t/view.rs:312-322` (weaker) | `P d/term/src/shim.c` (`pt_mode`, encode); `P d/pocket/src/main.rs`; `P d/pocket/src/overlay.rs` confirm (new) | S | None |
| 16-7 | Scrollbar rail: 3→5px thumb, 24 min, 4 inset, 10 hit strip, 1400ms linger, 260ms fade, drag to scroll | See your position in the history; jump quickly | `Z crates/ui/src/popover.rs:1369-1419`, `:1770-1771` | New widget in `P d/ui` or `termview.rs` (port) | M | 16-1 |
| 16-8 | Links: OSC 8 URI plus URL/`path:line` detection; ⌘-hover underline, ⌘-click open | Open the PR and localhost URLs agents print | `G h/screen.h:196`, `G h/grid_ref.h:167-191`, `G zt/formatter.zig:1470-1490` | `P d/term/src/shim.c` (URI per cell); `P d/pocket/src/termview.rs` (new) | M | 16-4 hit-testing |
| 16-9 | Mouse reporting through the ghostty mouse encoder; Shift bypass | Mouse works in vim, htop and agent TUIs | `G h/mouse/encoder.h:35-208`, `G h/terminal.h:1812` | `P d/term` (encoder wrap); `P d/pocket/src/termview.rs` (new) | M | 16-2 |
| 16-10 | IME preedit paint plus `bounds_for_range` returning the cursor cell | CJK/Vietnamese input shows what you're typing; candidate window anchors | `P d/pocket/src/main.rs:996-1043` | `P d/pocket/src/main.rs`, `P d/pocket/src/termview.rs` (adapt) | S | None |
| 16-11 | 80ms trailing debounce on `fit` resize | No reflow storms or TUI flicker while dragging a window | `Z t/view.rs:36`, `Z t/panel.rs:991-1048` | `P d/pocket/src/main.rs:742-751` (adapt) | S | None |
| 16-12 | Tabs: drag reorder (5px arm, 150ms slide, ghost), middle-click close, exited tab at 0.55, OSC title | Organise many shells; close quickly | `Z t/panel.rs:79-140`, `:297-319`, `:1573-1606`; `M src/shared/hooks/useAnimatedReorder.ts:186-226` | `P d/pocket/src/view.rs:826-923`; tab order in `P d/workspace/src/workspace.rs` `tabs` (adapt) | M | None |
| 16-13 | Phone read-only terminal: pocketd `vt.Frame` + WS `terminal.watch`/`history`; grid and log modes; no resize | Check what an agent is really doing from the phone | F6; `P pd/wsserver/wsserver.go:154-212`; `P app/screens/ChatScreen.tsx:155-159` | `P pd/vt/vt.go`, `P pd/wsserver`, `P pd/proto` (new); new `app/screens/TerminalScreen.tsx` (new) | M | 16-1; refines 14-17 |
| 16-14 | Find in terminal (⌘F): ghostty search, next/prev, viewport highlights | Find an error in 10k lines of history | `G h/search.h:20-298` | `P d/term` (search wrap); `P d/pocket/src/view.rs` find bar (new) | M | 16-1, 16-4 |
| 16-15 | OSC side channels in pocketd: title, bell (tab dot), OSC 52 clipboard (desktop write, with a confirm), OSC 9/777 notification feeding Status | Agent titles and notifications reach the UI | `G h/terminal.h:1238-1532`; `Z t/panel.rs:445` | `P pd/vt/vt.go` callbacks; `P pd/terminal`; the desktop reads the events (new) | S | None |
| 16-16 | Cursor: DECSCUSR shape, blink (off under reduced motion), hollow when unfocused | You can tell which pane has focus; vim modes show | `G h/render.h:144-209`; `Z t/view.rs:547-559`; `M ft/ui/TerminalView.tsx:157-158` | `P d/term/src/shim.c`, `P d/pocket/src/termview.rs:54-55` (adapt) | S | None |
| 16-17 | Resizable split rows: drag sash, minimum 160, maximum 55%, double-click resets to 280 | Size a log pane to your liking | `Z crates/ui/src/settings.rs:53-56`, `Z crates/ui/src/shell.rs:249`, `:10270-10390`; `M ft/ui/ProjectTerminalDock.tsx:170-203` | `P d/pocket/src/view.rs:999` (adapt) | S | 16-11 |
| 16-18 | Phone key bar and raw input over WS `terminal.input` | Answer a y/n prompt or send ^C from the phone | F6; `P pd/terminal/terminal.go:268-274` | `P pd/wsserver`; `app/screens/TerminalScreen.tsx` (new) | M | 16-13; trust model |
| 16-19 | Port Zeron's per-cell glyph pinning for non-ASCII/wide glyphs if StyledText drifts | CJK, emoji and box-drawing stay on the grid | `Z t/view.rs:633-744` | `P d/pocket/src/termview.rs` (port) | M | Observed drift |
| 16-20 | Snapshot fidelity: add `scrolling_region`/`tabstops`/`charsets`/`kitty_keyboard`/`hyperlink`/`pwd`/`palette`, or switch to the GHOSTSNP binary snapshot (both screens) | Re-attach mid-TUI restores exactly; primary history kept under the alt screen | `P pd/vt/vt.go:22-40`; `G h/formatter.h:41-94`; `G zt/formatter.zig:486`; `G h/snapshot.h:24-90` | `P pd/vt/vt.go`; `P d/pocket/src/sessions.rs:44-57` (adapt) | S (flags) / M (binary) | None |

Related, not redone: 13-12 (Mac editing keys ⌥←/→, ⌘←/→, ⌘⌫; `docs/orchestrators/research/13-monocode-session-git-ux.md:365`) and 13-13 (close-tab confirm when busy; `:366`, `:490`). 14-16 is split here into 16-1/2/4/8/9/14; its prerequisite, "libghostty scrollback API check", is resolved by F2.

## UI/UX spec to copy

- **Layout.**
  - Pane body padding: keep Pocket's top 10 / sides 20 / bottom 14 (`P d/pocket/src/view.rs:1077-1079`). Selection geometry uses `padding_left = 20`.
  - The scrollbar rail sits inside the right padding: 10px hit strip, thumb inset 4 from top and bottom.
- **Grid.**
  - 13px Geist Mono, ligatures off.
  - Line 22 today; 18 as an option, pending the box-drawing check.
  - Cols = ⌊(w−40)/advance⌋.
- **Tokens (Pocket light theme).**
  - Text is TEXT #111113 on WHITE (`P d/theme/src/theme.rs:9`, `:16`).
  - Selection: TEXT at α0.16 (Zeron light), inactive pane α0.08 (MonoCode).
  - Cursor: TEXT at α0.55, filled; hollow 1px when unfocused.
  - Links: ACCENT #5b5bd6 underline on ⌘-hover (`:30`).
  - Scrollbar thumb: TEXT_4 at α0.5, 0.68 when dragging, 0.85 on hover.
- **Tab bar** (keep Pocket's sizes, `P d/pocket/src/view.rs:826-923`).
  - Add a drag ghost at the tab's size, sibling slide 150ms ease-out (0,0,0.58,1) (`Z crates/ui/src/motion.rs:301`, `:391`), 5px arm.
  - Middle-click closes. Exited tabs at opacity 0.55.
  - A bell dot on unseen tabs: 6px ACCENT, my choice.
- **Split sash.** 1px line, 10px hitbox, `row-resize` cursor. Minimum 160, maximum 55% of the pane column, double-click resets to 280.
- **States.**
  - Scrolled back: a "Jump to bottom ↓" pill, 28px r14, bottom-right, inside the padding. My addition; neither competitor has one.
  - Selection active. Link hover (⌘). Mouse mode on, with Shift to select (no chrome).
  - Exited: keep "Process exited with code {c}" (`P d/pocket/src/view.rs:1086-1088`).
  - "Connecting…" / "This session is not running." unchanged.
- **Motion.**
  - Scrollbar linger 1400ms, then fade 260ms.
  - Tab slide 150ms.
  - Sash snap-back on double-click 200ms (`Z crates/ui/src/motion.rs:389`).
  - No smooth scroll (MonoCode `smoothScrollDuration: 0`).
  - Reduced motion: no fades or slides, cursor steady.
- **Shortcuts.**
  - ⌘C copy (only with a selection), ⌘V paste, ⌘A select all (`ghostty_terminal_select_all`), ⌘F find.
  - ⌘-click opens a link. Shift+click extends. Shift+wheel always scrolls history.
  - Mac editing keys per 13-12.
  - Keep ⌘K = palette and ⌘J = NextWaiting. There is no clear shortcut; the shell's ^L works.
- **Copy text.**
  - "Paste {n} lines into {title}?" with "Paste" / "Cancel".
  - "Jump to bottom".
  - Tooltips "Close terminal" and "New terminal".
  - Phone: "Read-only", "Log" / "Grid" toggle, "Showing {cols}-column terminal".

## Open questions / risks

- Do the Claude Code and Codex TUIs use the alt screen, DECSTBM or mouse tracking? The answer decides wheel routing and the phone's default mode. Test with `printf` probes or read `DATA_MOUSE_TRACKING` live.
- Does a 22px line leave gaps in box-drawing (╭─╮) at 13px? Zeron uses 18; MonoCode uses 1.0. Compare screenshots before changing.
- Do Pocket's 22px line and the default CoreText ligatures actually reproduce Zeron's `--` bug in StyledText rows? Pocket paints the cursor as an inverted cell inside the run, so the symptom may differ.
- GPUI drift: Zeron's element code targets the zui fork. `TouchPhase`, `ScrollDelta` and `click_count` need checking in gpui-kit 0.6.6.
- Snapshot rebuilds the local Term (`P d/pocket/src/sessions.rs:44-57`). Scroll position and selection reset on every re-attach. Re-attaching mid-TUI loses primary history (`G zt/formatter.zig:486`); 16-20 fixes it.
- The effective scrollback is page-granular. `MAX_LINES = 10_000` keeps "almost always more" (`G h/terminal.h:1502-1523`). Memory is about 400KB per page; measure per terminal before shipping.
- Phone payload: `Plain()` returns all history (10k lines after 16-1). `terminal.history` must page. The frame push rate (≤4Hz) is a guess; measure it on cellular.
- Phone raw input (16-18) can type into any shell on the Mac. It needs pairing trust, per-session opt-in, and maybe the Needs-you path only.
- OSC 52 clipboard writes from remote programs are a known exfiltration and injection vector. Default to confirm or off.
- Two VTs (pocketd and desktop) must agree on scrollback options, or scrollbar totals diverge after re-attach.
- Ghostty's gesture autoscroll zone is 1px from the edge (`G zt/SelectionGesture.zig:155-157`). Pocket needs its own edge zone because of the 20px padding. Does feeding AUTOSCROLL_TICK with Rust-computed direction work, or must Rust drive `scroll_viewport` plus DRAG?

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 6.

Confirmed against source: Pocket metrics, paint path, no font features, shim `pt_new` without options, `Term` API, ⌘C test, IME stubs, ⌘J/⌘K bindings, no clipboard outside `explore.rs`, snapshot rebuild, pocketd VT options and `Plain()` doc mismatch, dead "Open raw terminal" button; libghostty defaults (10_000 bytes, null lines), `scroll_viewport`, `DATA_SCROLLBAR`, selection gesture events and 15ms/1px/60% values, `paste_encode`/`is_safe`, mode 1007 default on; Zeron 13/18/12 metrics, ligature comment, 2px threshold, 24ms tick and 1-3 line speed, wheel remainder with `TouchPhase::Started`, copy/paste keys and `paste_bytes`, tab 118×28 / bar 40 / 0.55 exited / middle-click / 150ms slide, dock 160/0.55/2000/280, `mod-j`, scrollbar 3→5/24/1400/260; MonoCode xterm options, selection colours, ⌘C/⌘K handling, wheel handler, 8ms/32KiB PTY, dock sizes, tab sizes, 5px reorder arm.

- "Pocket is ahead of both on IME" contradicted the xterm.js IME claim; narrowed to Zeron.
- Zeron cols formula citation `view.rs:447-452` → `:447-457`.
- Zeron env vars cited to `terminals.rs:33-37`; they are at `:280-282` (limits at `:33-36`).
- Pocket tab label max width 150 is at `view.rs:800`, outside the cited range; citation added.
- "Pocket is light-only" cited `theme.rs:16` (the WHITE constant); reworded to "defines no dark palette".
- Pocket mouse-down citation `view.rs:1069` → `:1070`. Idea 16-2 mapping marked `on_scroll_wheel` (new); 16-12 mapping pointed at `P d/workspace/src/workspace.rs`.
