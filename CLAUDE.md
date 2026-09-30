# CLAUDE.md

## Desktop performance

Treat repos with 1000+ changed files as the normal case.

- **Every notify redraws the whole window.** The UI is one `Desktop` entity, and the daemon poll (1s), git refresh (2s), terminal output and hover all call `cx.notify()`. Render must cost what's visible, not what exists.
- **Virtualize lists that grow with the repo** (Changes, Explorer, diff): `uniform_list` for fixed-height rows, `list` otherwise. Never build every row per frame. Rows inside `uniform_list` need `w_full()`; add a scrollbar with `UniformListScrollHandle` + `vertical_scrollbar`.
- **Keep git, file IO and syntax highlighting off the UI thread**, and out of `render`.
- **Clicks must not wait on `refresh_git`.** It reads every worktree sequentially (~130ms each), and a newer run discards the older run's result (`git_run`). Load only what the action needs in its own background task, and apply it only if the state it was for still holds (see `load_diff` / `apply_diff`).
- **Keep `[profile.dev.package."*"] opt-level = 2`.** Unoptimized tree-sitter is ~6× slower. Judge speed in release: our own code is ~10× slower in debug.
- **Measure before fixing.** Use capture mode (`cargo run --release -p pocket --features capture -- --capture <dir> <name>=<step>,…`) with temporary timings tagged `[DEBUG-…]`, and grep them out afterwards.

Baselines to beat:

| Measurement | Before | Now |
|---|---|---|
| Changes list, one frame with 1137 files | 67ms | 3ms |
| Click file → diff shown | 490ms | ~100ms |
