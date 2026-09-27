# UI Review Changelog

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
