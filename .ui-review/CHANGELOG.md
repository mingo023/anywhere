# UI Review Changelog

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
