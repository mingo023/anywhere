# UI Review Changelog

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
