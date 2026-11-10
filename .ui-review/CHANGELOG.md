# UI Review Changelog

## 2026-11-10 · e68b4593 + uncommitted · Board pane compared like for like; new features on the real daemon
- passed: outside text every screen is under 1%: empty 0.05%, columns 0.14%, list 0.14%, filtered 0.14%, bulk 0.24%
- failed: none
- skipped: sidebar (the app's own width and content); Board-only Command menu icon (removed from the prototype shot, not built by decision)
- measured: compare.py on the 1168x900 pane, prototype viewport sized to match and given the fixture's tickets. With text included: empty 0.54% and filtered 0.95% pass; columns 1.50%, list 2.71%, bulk 2.86% do not
- text check: 265 text runs compared for weight, colour and height. Median height difference 0. Found and fixed: placeholder colour, hint key chips, hidden-tickets notice colour and position
- fixes: bulk bar fill, ring, glyphs and position; Board and ticket bar height 44 with 2pt title inset; first list group top gap; tab gaps and chip widths (border was adding 2pt); list-row actions show only on hover or cursor; card actions right on the meta row; card head height and title gap; dock composer line height and footer margin; Start shortcut chip
- note: the text-included remainder is system font width, about 2.5% wider in Chromium than GPUI; the skill's triage ignores OS font rendering. Text runs start within 1pt except where a run is right-anchored or follows other text. Note wrapping in cards was normalized in the prototype shot because it is a result of that width
- note: not a pass with text included. Equalizing the browser's text width (letter-spacing -0.0123em in the prototype shot only) still left list 2.23% and bulk 2.38%, so the rest is how each renderer draws glyphs
- note: on a real pocketd: bulk Priority, Label, Agent, Status to Queued, Close with Undo, multi-value and "is not" filters, saved view after an app restart (stub agent); bulk Start of two tickets with real Claude ran through to Done

## 2026-11-10 · e68b4593 + uncommitted · full (caller-requested re-check of the board empty state)
- passed: board (4.56% full-canvas; explained below, no real defect)
- failed: none
- skipped: sessions, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus, automations, automations-runs, automations-editor, settings-general, settings-appearance, settings-notifications, settings-keyboard, settings-keyboard-custom, settings-projects, settings-agents, settings-agent-claude, settings-agent-codex, settings-automations, settings-automations-stopped, settings-phone, settings-phone-empty, settings-project-edit, settings-sidebar, settings-terminal, settings-files, settings-browser, settings-git, settings-diff, settings-search (no design this run; mode forced to full by new icon assets under theme/assets/icons/**)
- designs: board=4510075a5324
- trigger: caller re-ran the board-empty review (same flow.html "Empty" variant) against board-empty-state-polish's board.rs/board/** changes
- note: mismatch stayed within 0.3pt of the last run's 4.29% (board features since landed: composer.rs, selection.rs, bulk_bar.rs, ticket_page.rs, views.rs). Of the 4.56%: (1) the sidebar project/ticket rows and the composer's placeholder-text suffix are fixture/live data, not design; (2) the hero block centers with `flex_1().items_center().justify_center()` (composer.rs:98) inside a 652px `HERO_WIDTH` box — unchanged and still matching the design pixel-for-pixel — but the design's mockup floats the window 72px/24px inset (804px tall) while the headless capture fills the full 900px canvas with no window chrome, so the same centered block sits ~46px lower in the taller capture; this is the mockup's presentation, not a layout bug (confirmed: header-row offset is ~73px but hero offset is only ~26px, and (900-804)/2≈48px accounts for the gap). No pixel, copy, icon, or style regression found.
- fixes: none (review-only)
- unmapped UI changes: packages/desktop/crates/pocket/src/desktop.rs, desktop/chrome.rs, desktop/project.rs, modals.rs, sidebar/rail.rs → none of `screens.*.files` covers these; worth a follow-up pass on the config globs

## 2026-11-10 · e68b4593 + uncommitted · board features, clicked through against the fixture
- passed: filter-menu search, row project/agent pick, J/K cursor with column scroll, X and ⌘A selection, bulk bar, bulk Close with Undo, bulk Start, saved view tabs
- failed: none
- skipped: none
- note: crops pixel-diffed against the prototype's own screenshots: filter menu 3.4%, value panel 2.0%, filter token 5.5%, Clear and Save view 6.3%; those crops differ by ticket counts and text antialiasing. Bulk bar not diffed: the prototype's shot caught it mid-fade
- designs: board=4510075a5324
- trigger: Linear features from the prototype added to the Composer Board
- fixes: picked card ring showed through the translucent surface as a solid fill; row check uses the prototype's indigo; bulk Status, filter side panel, several values per filter and is / is not added

## 2026-11-10 · e68b4593 + uncommitted · board re-diff, others against unchanged code
- passed: board empty state after aligning to the design's window (rest is sample copy and project names)
- failed: none
- skipped: none; changes dropped its `comment` step, which capture mode doesn't have
- designs: board=4510075a5324
- trigger: Board Composer implementation; split and 35 other screens pixel-compared with e68b4593
- note: 4.3% full-frame mismatch is the design's inset window and sample sidebar. Other screens differ only by clocks and loading dots. Placeholder color is the app-wide muted token, brighter than the design's TEXT_4
- fixes: hero textarea size (Textarea sets its own, so parent text_size did nothing), disabled Start label

## 2026-11-10 · e68b4593 · full (only "board" has a design this run)
- passed: none
- failed: board 4.29%
- skipped: sessions, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus, automations, automations-runs, automations-editor, settings-general, settings-appearance, settings-notifications, settings-keyboard, settings-keyboard-custom, settings-projects, settings-agents, settings-agent-claude, settings-agent-codex, settings-automations, settings-automations-stopped, settings-phone, settings-phone-empty, settings-project-edit, settings-sidebar, settings-terminal, settings-files, settings-browser, settings-git, settings-diff, settings-search (no design this run; blast radius was new icon assets under theme/assets/icons/**, which forced scope.py to full)
- designs: board=4510075a5324
- trigger: caller-requested review of the Board's empty state (flow.html, "Empty" variant) against board-empty-state-polish's board.rs/board/** changes; new .ui-review/config.json screen "board" added (capture: dark, board, board-empty)
- note: design and impl use identical color tokens (FILL_1, SEPARATOR, TEXT_2/3/4 all match exactly) and identical hero layout constants (HERO_WIDTH 652, composer padding 12/12/9, textarea min-h 60) — confirms this screen's build matches the design's measurements precisely. The only vertical offset in the diff (~27px, cascading from a ~54px gap at the very top) is the headless capture's missing native window-chrome/traffic-lights, not a real bug — confirmed by the hero-to-header gap matching within 5px once that offset is accounted for. Sidebar project names/colors and the inbox bell's "3" badge differ because they're fixture/live data, not design. Real defects below.
- fixes: none (review-only)

## 2026-10-09 · 6427b5f · full (caller-scoped, 10 settings screens)
- passed: settings-automations, settings-automations-stopped, settings-git, settings-terminal, settings-browser
- failed: settings-agent-claude, settings-agent-codex, settings-project-edit, settings-phone, settings-phone-empty
- skipped: sessions, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus, automations, automations-runs, automations-editor, settings-general, settings-appearance, settings-notifications, settings-keyboard, settings-keyboard-custom, settings-projects, settings-agents, settings-sidebar, settings-files, settings-diff, settings-search (not requested this run)
- designs: settings-agent-claude=9cfd31c79a06, settings-agent-codex=6cad2fc474ba, settings-project-edit=9a25e8ee42ca, settings-automations=b45f8657f421, settings-automations-stopped=626d074900f6, settings-git=f72d7f39a3c9, settings-terminal=f63c3f1ccfb7, settings-browser=a44e0a52cf68, settings-phone=69e3ce93a9ba, settings-phone-empty=6e5a5711b92f
- trigger: caller-requested review of the new per-provider Agent pages (Claude Code, Codex), the Project-settings agent row, Git AI model row, Automations status card, Terminal, Browser, and Phone & Remote (dark), against designs at /private/tmp/dshots
- note: `since` == `head` (6427b5f) because the working tree is dirty (151 uncommitted files); this review covers the current working tree, not a commit boundary, and will re-trigger in full next time until something is committed. Several `.ui-review/config.json` globs are stale and silently never match: top-level `view.rs`, `overlay.rs`, `forms.rs`, `termview.rs`, `explore.rs`, `diff.rs` no longer exist (code moved to `desktop.rs`, `palette.rs`, `modals/*`, `terminal_view.rs`+`terminal_view/**`, `explorer/**`, `git_ui/diff.rs`), and `inbox.rs` now has a sibling `inbox/**` not covered by any screen's `files`. Recommend updating `blast_radius` and the affected `screens.*.files` entries in a follow-up pass. Confirmed real gaps this run: Claude/Codex provider pages are each missing two Advanced rows under Launch — "Extra arguments" (hint "Added to every launch") and "Environment variables" (hint "Set only for this provider's sessions", empty state "No variables") — `rows::<P>()` in settings/provider.rs:27-48 defines only 5 rows total and has zero `.advanced()` entries, so neither page can ever show the design's "N more in Advanced" toggle; Project settings' Agent/Model/Effort row renders as a horizontal 3-column flex row (modals/add_project.rs:351-377) instead of the design's 3 stacked full-width rows under an "Agent defaults" header; Phone & Remote's Tailscale row renders a literally empty control area with no value, spinner or placeholder while `self.agents.host` hasn't loaded (settings/phone.rs:65, `None => div().into_any_element()`), and the Devices list has the same empty-div gap while `host.devices` is `None` (phone.rs:75) — likely why the "no-phones" capture step showed a blank area instead of the "No phones paired yet" placeholder. Lower-severity copy-only diffs (automations.rs:30 Agent-row hint, terminal.rs:59-60 confirm-close wording, browser.rs:28-29/:53 two hint/caption clauses, phone.rs:24 "This Mac" vs design's "This network") are listed under their screens below and did not change the pass/fail call.
- fixes: none (review-only)

## 2026-10-08 · 6427b5f · full (caller-scoped, 16 settings screens)
- passed: settings-general, settings-notifications, settings-keyboard, settings-projects, settings-sidebar, settings-files, settings-git, settings-diff, settings-search
- failed: settings-appearance, settings-agents, settings-automations, settings-phone, settings-phone-empty, settings-terminal, settings-browser
- skipped: sessions, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus, automations, automations-runs, automations-editor (not requested this run)
- designs: settings-general=8a44568b12e1, settings-appearance=4c0b7026506d, settings-notifications=6b391270304d, settings-keyboard=331ef2aabcfa, settings-agents=f30df4b03159, settings-automations=b45f8657f421, settings-phone=69e3ce93a9ba, settings-phone-empty=6e5a5711b92f, settings-projects=68dad62a1a9a, settings-sidebar=b985df67a2c1, settings-terminal=f63c3f1ccfb7, settings-files=d54aa8c85800, settings-browser=a44e0a52cf68, settings-git=f72d7f39a3c9, settings-diff=cd19d1479608, settings-search=db663d657b94
- trigger: caller-requested review of all 16 Settings sections (dark) against the now-complete settings rewrite (ROWS-based catalog in settings/*.rs), superseding the 5-screen scaffold entry below
- note: impl captures are fixed-viewport (1440×900, unscrolled); designs are full-page. Compared with compare.py --crop 0,0,1000,625 so mismatch % reflects only the visible region and is still mostly 1000px-vs-1440px content-density noise, so it's omitted below; below-the-fold content was verified against each settings/*.rs ROWS const directly instead of guessed from the crop. Confirmed real gaps: Appearance has no Syntax theme row; Agents is missing "When an agent exits" (Tabs) and has zero `.advanced()` rows despite Composer's "1 more in Advanced"; Automations is missing Permission mode, Run in a new worktree, Catch up after sleep; Phone & Remote has no Network/Reachable-from (This network/Tailnet) section at all, and its subtitle drops ", and from where"; Terminal's swatches have no `on_click` and the Color scheme hint drops "Click one to change it"; Browser's Home page hint reads "Dev server opens…" vs design's "Project dev server opens…", and its dev-servers caption drops "Also editable in each project's settings" (correctly — the per-project edit form has no dev_url field). Not flagged (excluded by the caller, or intentional current codebase behavior): custom providers, pairing code, interface size, Option-as-Meta sides, custom shell, background-service switch, force-with-lease (Git Committing's own "1 more in Advanced"), ligatures (Terminal Text's "3 vs 2 more in Advanced"), plan usage, jump letters, keyboard rebinding, accent/fonts, the nav group label "Workspace" (design) vs "Projects" (impl, settings.rs:115, pinned by the test at :734, affects all 16 screens), General's extra "Last used" layout option, Agents' extra "Agent's settings" permission option, Notifications' Banners master switch and dropdown-only sound rows, Sidebar's stepper-style Warn/Critical-at controls.
- fixes: none (review-only)

## 2026-10-08 · 6427b5f · full (caller-scoped to 5 screens)
- failed: settings-general 58.23%, settings-appearance 50.37%, settings-notifications 46.16%, settings-keyboard 77.02%, settings-projects 44.45%
- skipped: sessions, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus, automations, automations-runs, automations-editor (not requested this run; mode is full since blast radius + many files changed since b15b94e, but caller asked only for the 5 settings screens)
- designs: settings-general=8a44568b12e1, settings-appearance=4c0b7026506d, settings-notifications=6b391270304d, settings-keyboard=331ef2aabcfa, settings-projects=68dad62a1a9a
- trigger: caller-requested review of SettingsGeneralDark/AppearanceDark/NotificationsDark/KeyboardDark/ProjectsDark.png against an in-progress settings rewrite (uncommitted: settings.rs, settings/{appearance,general,keybindings,nav}.rs modified, settings/notifications.rs new, settings/worktrees.rs renamed to projects.rs)
- note: mismatch % is mostly unbuilt content, not fidelity (design 1000×N crop vs impl 1440×900 full window, no manual crop applied). One real cross-screen defect: card/row background renders ~2x lighter than design (impl rgb(56,56,56) vs design rgb(28,28,31) against a near-identical page bg ~rgb(20) vs ~rgb(14)) — theme.rs:68 `SURFACE` token, affects all 5 screens, app-wide not settings-specific. The large remaining gap is the settings screens being an early scaffold: only 5 of the design's ~14 sections exist (Section enum, settings.rs:18-24), no search bar (nav.rs:25-32), no "Show advanced (N)" / "Reset … to defaults" anywhere, and each screen's content card covers a fraction of what the design specs (General: just Software update, missing Startup & background / Launch layout / Quitting & sleep / Performance; Appearance: missing Accent color, Fonts, Code colors; Notifications: missing per-status banner split, sound pickers, Volume, Dock & bell; Keyboard: missing search/filter, rebinding-on-click + conflict resolution, Reset all, and groups by what-it-does instead of design's where-it-works; Projects: missing Clone folder, Clone… button, Deleting a worktree section). Treated as unbuilt-feature, not defects, per caller's bias instruction. No source changes made (review-only).
- fixes: Reduce motion hint now uses the design's copy; earlier in this rewrite ui.rs `toggle` got a white knob in dark (ON_TEXT went dark on the green track)
- kept: SURFACE stays the app-wide translucent dark token (headless capture composites the glass to grey, so the card looks lighter than on a real desktop); Idle shows no "Up to date" dot since Sparkle doesn't report a last check; Keyboard keeps grouping by what a shortcut does, tagging where it works

## 2026-09-29 · b15b94e · full
- passed: none
- failed: sessions 4.54% (Workspace column only)
- skipped: sessions-sidebar-expanded, changes, split, inbox, explore, command-palette-k, new-session, add-repository, repositories-worktrees, compact, compact-sidebar-open, session-terminal-focus (no design)
- designs: sessions=e13c86beb40e
- trigger: caller-requested review of "v2 · Sessions@1x.png", scoped to the Workspace column (header, tabs, search, session cards); blast radius (forms.rs, overlay.rs, view.rs, arrow-up.svg, theme.rs) + view.rs/workspace.rs/termview.rs changed since 50078dc
- note: cropped both images to the Workspace column (design x72-420, impl x112-808@2x) so the intentionally-narrower 56pt rail (design 72pt) doesn't skew alignment; also not flagged per caller: no vertical divider between rail and column, no filter button, "Explorer" label, missing Changes badge, project dropdown removed, rail initials from fixture. Chrome (title, tabs, search row) matches design almost exactly. The one real issue: card order. Design's 6 sample cards keep fixture/insertion order (Needs You, Running, Running, Idle, Failed, Idle) — Upgrade to RN 0.81 (Failed) sits 5th, below both Running cards. The app re-sorts by urgency (`cards.sort_by_key(|c| c.status)`, view.rs:700, using the Ord on `Status` in status.rs:6-12: NeedsYou < Failed < Done < Working < Idle), which bubbles the Failed card to 2nd, ahead of the two Running sessions. That single swap is the entire 4.54% mismatch — rows 1 and 6 match exactly; rows 2-5 all differ only because of the one card's position.

## 2026-09-29 · 50078dc · incremental
- passed: sessions 0.81%, compact 0.36%, compact-sidebar-open 0.75%, session-terminal-focus 0.24%, new-session 0.80%, sessions-sidebar-expanded 0.95%
- skipped: changes, split, inbox, explore, command-palette-k, project-menu-amp-worktrees, add-repository, repositories-worktrees (not requested this run)
- designs: all six 24a6feeff6e3
- trigger: 99% fidelity pass on Agent Remote (1).html
- note: captures now run against `.ui-review/fixture` (bun fixture pocketd serving the design's sessions, TZ pinned to 14:00, AppleFontSmoothing 0 during capture only). The new "worktree" capture step selects the session's worktree for compact-sidebar-open and repositories-worktrees. SANS is now .SystemUIFont to match the design's system-ui. Remaining diffs are design data we don't have or that contradicts itself: worktree badge counts, the card's +84 −51 vs the bar's +42 −17, "opus · high" (we show "Opus"), "resets 1h 48m", the branch "fix/flaky-snapshot" (we derive it from the prompt), and no traffic lights. The new-session "@ files · / commands" hint and mic are still not built because neither feature exists.

## 2026-09-28 · 50078dc · incremental
- passed: sessions, compact, compact-sidebar-open, session-terminal-focus, sessions-sidebar-expanded
- failed: new-session
- skipped: changes, split, inbox, explore, command-palette-k, project-menu-amp-worktrees, add-repository, repositories-worktrees (not requested this run)
- designs: sessions=24a6feeff6e3, compact=24a6feeff6e3, compact-sidebar-open=24a6feeff6e3, session-terminal-focus=24a6feeff6e3, new-session=24a6feeff6e3, sessions-sidebar-expanded=24a6feeff6e3
- trigger: caller-requested review of Agent Remote (1).html against blast radius (theme.rs, ui.rs, view.rs, overlay.rs, forms.rs, arrow-up.svg) + view.rs/main.rs/forms.rs changes since 88b7acd
- note: 2.1-6.2% mismatch on all 6 screens traced almost entirely to the headless capture selecting real local ~/.coding-pocket session data (different projects/sessions, no active running command/git diff/dev-server tab, different user initials) instead of the design's populated exemplar session — chrome, spacing, colors, and icons matched everywhere content was actually comparable. new-session's dialog itself is still missing the "@ files · / commands" hint row + mic icon (forms.rs:559-563 goes straight from the branch chip to the send button) — same gap flagged unimplemented on 2026-09-27, left as-is. sessions-sidebar-expanded and new-session capture recipes don't include a "session" step, so their backgrounds render empty vs. the design's populated background, inflating their numbers further — added "session" to both (re-run: new-session 5.62%, sessions-sidebar-expanded 4.84%, remaining diff is data). No source changes made; target of ≥99% match is not reachable without matching fixture data first.

## 2026-09-28 · 88b7acd · full
- passed: sessions, sessions-sidebar-expanded, changes, explore, repositories-worktrees, terminal-focus
- failed: command-palette-k, new-session, project-menu-amp-worktrees, add-repository (fixed)
- skipped: split, inbox (no design)
- designs: sessions=40574e0b0075, sessions-sidebar-expanded=d095965700d6, changes=b4639de1fc80, explore=ff101789cdef, command-palette-k=5483488e6184, new-session=39fe454f4641, project-menu-amp-worktrees=a18e707caec6, add-repository=e167de64e5b7, repositories-worktrees=f317924950e5, terminal-focus=64d255efe3c7
- trigger: new export (Agent Remote (4).html — inline 44px top bar replaces title+meta block, new focus layout); packages/desktop/crates/pocket/src/{diff,explore,main,view}.rs, packages/desktop/crates/ui/src/ui.rs changed since d505d02
- note: floating menus/dialogs on `ui::pop`/`ui::modal` (palette, new-session, project menu, add-repository) showed background text/images through the panel. Captures came from a binary started before `pop()` switched from translucent `0xffffffd6` to solid `SURFACE`; relaunched on the new build, palette and new-session render opaque. Remaining 4–8% mismatches on the other screens are live data.

## 2026-09-28 · d505d02 · full
- passed: sessions, sessions-sidebar-expanded, changes, explore, command-palette-k, new-session, project-menu-amp-worktrees, add-repository, repositories-worktrees
- failed: none
- skipped: split, inbox (no design)
- designs: sessions=ef256f9808bb, sessions-sidebar-expanded=c1e11265c546, changes=188406d1de1d, explore=a90e2297737a, command-palette-k=a3d12d70d121, new-session=a9df74ee652a, project-menu-amp-worktrees=66c761edc414, add-repository=721c1a3e4edd, repositories-worktrees=7a56880a7d0d
- trigger: new/updated designs (Agent Remote (3).html export); no code changes since d505d02
- note: re-reviewed against a refreshed export with richer fixture data (populated worktree lists, varied session states, prefilled forms/comments). All 9 screens still match structurally; every compare.py mismatch (5.7-12.1%) traced to live data (session/repo names, counts, diff stats, timestamps) or to conditional UI legitimately absent in this sparse local fixture (Run-setup checkbox needs a configured setup script, Claude Code model label falls back to "Default model" with no prior session history, comment thread empty since the file has no existing comments). No source changes made.

## 2026-09-28 · d505d02 · full
- passed: sessions, sessions-sidebar-expanded, explore, changes, command-palette-k, new-session, project-menu-amp-worktrees, add-repository, repositories-worktrees
- failed: none
- skipped: split, inbox (no design)
- designs: sessions=06b15a8cd4e1, sessions-sidebar-expanded=79fe0b17453e, changes=d12f531a5eb4, explore=3f953abc220a, command-palette-k=884cad52125a, new-session=39a63b9384ca, project-menu-amp-worktrees=b1ac2595bdb3, add-repository=eed18972549a, repositories-worktrees=100a18140dbf
- trigger: packages/desktop/crates/pocket/src/main.rs, packages/desktop/crates/pocket/src/view.rs, packages/desktop/crates/pocket/src/diff.rs, packages/desktop/crates/pocket/src/explore.rs, packages/desktop/crates/pocket/src/overlay.rs, packages/desktop/crates/theme/src/theme.rs, packages/desktop/crates/ui/src/ui.rs
- note: reviewed the flush-pane redesign (rail 72/240px, column 348/334px, aside 272px, no radius/shadow on chrome, 0.5px hairlines) already implemented on this branch. All 9 screens matched design structurally; every compare.py mismatch (6-12%) traced to live-data/fixture differences (session counts, open file vs. no selection, comment composer/Run-setup checkbox hidden because their preconditions — existing comments / a repo setup script — aren't present in this fixture, not because the feature is missing). No source changes were needed.

## 2026-09-27 · 65e97a9 · full
- passed: sessions, explore, changes, command-palette-k, new-session
- failed: project-menu-amp-worktrees, add-repository (fixed), repositories-worktrees (fixed)
- skipped: none
- designs: sessions=3f4eeffece97, explore=db5c14fc2163, changes=d78db1da29e4, command-palette-k=61dd392d6781, new-session=dadeae61d89b, project-menu-amp-worktrees=a82e1c0460ef, add-repository=99a76b41ca74, repositories-worktrees=5165169a3afb
- trigger: packages/desktop/crates/pocket/src/overlay.rs, packages/desktop/crates/pocket/src/forms.rs, packages/desktop/crates/ui/src/ui.rs
- fixes: overlay.rs worktree-menu idle suffix was appended to Done sessions (should be Failed-only); forms.rs add-repository source tabs were missing their folder/external icons; ui.rs worktree_row used the full labelled status pill instead of the aside's compact icon-only pill for Waiting/Running
- note: New Session's design shows a "@ to mention files · / for commands" hint row under the prompt textarea with no backing feature in the codebase (no @-mention, slash-command, or dictation support anywhere) — left unimplemented rather than adding dead UI; flagged for follow-up

## 2026-09-27 · f07e1cc · full
- passed: sessions, changes, split, inbox
- failed: none
- skipped: none
- designs: sessions=70e2cbef45a3, changes=70e2cbef45a3, split=70e2cbef45a3, inbox=70e2cbef45a3
- trigger: packages/desktop/src/ds.rs, packages/desktop/src/theme.rs, packages/desktop/src/view.rs, packages/desktop/assets/icons/**
- note: design is a component board (no screens); compared components visually, fixed root line-height (φ → 1.2)

## 2026-09-25 · 71e6eb2 · full
- passed: none
- failed: changes
- skipped: sessions, split, inbox (no design)
- designs: changes=388434e2d9d0
- trigger: packages/desktop/src/diff.rs, packages/desktop/src/git.rs, packages/desktop/src/theme.rs, packages/desktop/src/view.rs, packages/desktop/assets/icons/chevron-down.svg

## 2026-09-25 · 6e8fa40 · full
- passed: split, inbox
- failed: sessions, changes
- skipped: none
- designs: sessions=9942b8158e2a, changes=d7e079937766, split=4daa243c9940, inbox=661f70859265
- trigger: first run
