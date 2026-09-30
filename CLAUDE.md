# CLAUDE.md

## Desktop code layout

Follow `docs/adr/0003-desktop-code-layout.md` (Zed's split); its map says where new code goes.

- **Model crates hold logic, `pocket` holds views.** Model crates (`git`, `project`, `workspace`, `agents`, …) never depend on `ui`, `theme` or `pocket`. Domain logic that doesn't need GPUI goes in one.
- **One module per feature in `pocket`, with its own state struct** (`DiffState`, `Terminals`, …). Its `new` builds it and returns its own subscriptions; `Desktop::new` only composes. Never add a feature's field straight to `Desktop`.
- **`impl Desktop` stays thin:** gather inputs, call the logic, `cx.notify()`. Decisions live in state methods or free functions over plain data, with no `Window`, `Context` or `App` in the signature.
- **State holding an `Entity` can't be built in a test**, so put its logic on a plain value type inside it (`Pick` in `git_ui/diff.rs`) or in free functions.
- **One file per component.** A module with children is `foo.rs` plus `foo/`; no `mod.rs`. A crate's lib is `src/<crate>.rs`.
- **Tests sit beside the logic** in `#[cfg(test)] mod tests` at the bottom, named as sentences (`tree_hides_what_a_folded_folder_holds`). Test behaviour through the public interface; no mocks, no render tests. Test git writes against a temp repo.
- **Refactors change no behaviour.** A bug found while moving code gets pinned by a test and fixed separately.

Before committing, from `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` with no new warnings, `cargo test --workspace`. For view changes, capture screens before and after with `.ui-review/fixture/capture.sh <dir> <name>=<steps>…` and compare.

## Desktop performance

Treat repos with 1000+ changed files as the normal case.

- **Every notify redraws the whole window.** The UI is one `Desktop` entity, and the daemon poll (1s), git refresh (2s), terminal output and hover all call `cx.notify()`. Render must cost what's visible, not what exists.
- **Virtualize lists that grow with the repo** (Changes, Explorer, diff): `uniform_list` for fixed-height rows, `list` otherwise. Never build every row per frame. Rows inside `uniform_list` need `w_full()`; add a scrollbar with `UniformListScrollHandle` + `vertical_scrollbar`.
- **Keep git, file IO and syntax highlighting off the UI thread**, and out of `render`.
- **Clicks must not wait on `refresh_git`.** It reads every worktree sequentially (~130ms each), and a newer run discards the older run's result (`git_run`). Load only what the action needs in its own background task, and apply it only if the state it was for still holds (see `load_diff` / `DiffState::apply`).
- **Keep `[profile.dev.package."*"] opt-level = 2`.** Unoptimized tree-sitter is ~6× slower. Judge speed in release: our own code is ~10× slower in debug.
- **Measure before fixing.** Use capture mode (`cargo run --release -p pocket --features capture -- --capture <dir> <name>=<step>,…`) with temporary timings tagged `[DEBUG-…]`, and grep them out afterwards.

Baselines to beat:

| Measurement | Before | Now |
|---|---|---|
| Changes list, one frame with 1137 files | 67ms | 3ms |
| Click file → diff shown | 490ms | ~100ms |
