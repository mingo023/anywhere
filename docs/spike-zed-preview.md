# Spike: Zed-style file preview, markdown and diff on gpui-kit

Date: 2026-09-28. Code in `docs/spike-zed-preview/` (standalone crate, not in the desktop workspace). Background: `docs/research-zed-diff-rendering.md`.
Stack: gpui-kit 0.6.6 (`tree-sitter-rust`, `tree-sitter-markdown`), imara-diff 0.2, Rust 1.98.1, release build, M-series Mac, 60 Hz display.

## Question

Can gpui-kit's own pieces give Explore and Changes Zed-level rendering (syntax colours, word diff, markdown) and smooth scrolling on big inputs, styled with our tokens?

## What was built

One window with four tabs, all on `theme` tokens (`SURFACE_SUNKEN`, `MONO` 13 px, 22 px rows, `SYN_*`):

| Tab | Renders with |
|---|---|
| gpui-kit Editor | `EditorState` read-only, `language("rust")`, line numbers, search; `HighlightTheme` rebuilt from `SYN_*` |
| Own rows | `uniform_list` + `StyledText::with_highlights`, spans from `SyntaxHighlighter` |
| Markdown | `TextViewState::markdown` + `TextView` with `TextViewStyle.highlight_theme` |
| Diff | imara-diff Histogram + `postprocess_lines`, 3 lines of context, "N unchanged lines" folds, word diff per paired line, syntax spans on both sides merged with `combine_highlights`, rendered in `list(ListState)` like `diff.rs` |

Run: `./bench.sh <editor|rows|md|diff> <px> [lines]`, `./shot.sh <tab>` (from `docs/spike-zed-preview/`).

`SPIKE_BENCH=<px>` scrolls the active tab by that many px each frame via `request_animation_frame` for 4 s, then prints frame intervals and per-frame CPU (render → paint, measured with a trailing `canvas`).

## Results

### Open cost (release)

| Step | Input | Time | Thread |
|---|---|---|---|
| `EditorState::set_value` | 20k lines / 731 KB | 4.6 ms | UI |
| `EditorState::set_value` | 100k lines / 3.6 MB | 23.5 ms | UI |
| tree-sitter parse + styles (whole file) | 20k lines | 237 ms | must be background |
| tree-sitter parse + styles (whole file) | 100k lines | 980 ms | must be background |
| tree-sitter parse + styles | 5.6k lines (one diff side) | 83–91 ms | background |
| imara-diff line diff | 5.6k vs 5.6k lines, 200 hunks | 0.6 ms | any |
| word diff + row build | 3.2k rows | 4.1 ms | any |
| `TextViewState::markdown` | 1.7 MB | 0.4 ms (parse is async) | UI |

The Editor highlights incrementally in the background, so opening a 100k-line file costs one 23 ms frame.

### Scrolling (4 s, 20k-line source unless noted)

Frame interval: p50 16.67 ms and 0 frames over 1.5× vsync in every run below, except the one markdown parse hitch (see below). The ">16.7 ms" count in the raw log is vsync jitter (16.6–17.8 ms), not jank.

| View | px/frame | CPU per frame p50 / p95 / p99 |
|---|---|---|
| Editor | 40 | 3.3 / 5.2 / 6.0 ms |
| Editor | 400 | 4.8 / 7.0 / 7.8 ms |
| Editor, 100k lines | 400 | 5.1 / 7.4 / 7.9 ms |
| Own rows | 40 | 2.8 / 4.2 / 4.7 ms |
| Own rows | 400 | 4.7 / 6.6 / 8.0 ms |
| Own rows, 100k lines | 400 | 4.8 / 6.6 / 7.2 ms |
| Diff (`list`, 3.2k rows) | 40 | 3.0 / 4.4 / 5.1 ms |
| Diff | 400 | 2.9 / 4.2 / 4.9 ms |
| Markdown 57 KB | 400 | 5.5 / 7.5 / 9.7 ms |
| Markdown 1.7 MB | 400 | 4.7 / 8.5 / 12.0 ms |

- **All views hold 60 fps**, even flinging 400 px per frame through 100k lines.
- **120 Hz (8.3 ms budget):** Editor, rows and diff p99 stay under 8 ms. Markdown p99 goes to 10–12 ms, so expect an occasional dropped frame on ProMotion.
- **Markdown hitch:** one frame when the background parse lands. It scales with size: 59 ms at 57 KB, 94 ms at 287 KB, 342 ms at 1.7 MB. Our 512 KB `MAX_BYTES` cap keeps it under ~150 ms.

### Look

Screenshots via `shot.sh` (window only, no focus steal):
- **Editor** matches `code_box`: same fonts, 22 px rows (`line_height` override works), our syntax colours, plus active-line tint and indent guides.
- **Diff** reads like Zed: red/green row tints, stronger word-level tints (`0xe5484d38` / `0x30a46c40`), syntax colours on deleted lines too, folds between hunks.
- **Markdown** renders headings, lists, inline code and tables with our tokens.

## Gotchas

- **Unfocused windows render at 30 fps.** `WindowOptions::inactive_frame_interval` defaults to 33 ms, so measure with it set to `None`. Focused windows are unaffected.
- **`list()` does not cascade text style to its rows.** `.font_family(MONO)` on the `list` element is ignored, so set it on a wrapping `div` (as `diff_box` does) or on each row. Rows also need `.w_full()` or their background stops at the text.
- **Unknown languages still return ranges.** `SyntaxHighlighter::new(lang)` needs no `gpui_kit::init` and falls back to plain text, but `styles()` also returns the unstyled gaps (`color: None`). Filter them out.
- **`EditorState::set_value` resets scroll and selection to the top.** To refresh a file that changed on disk, save `scroll_offset()` first and restore it with `set_scroll_offset()`.
- **`TextViewStyle` is a struct with public fields, not a builder.** Code block and table styles are `StyleRefinement`s, so style markdown locally instead of changing global `Theme` colours that inputs share.
- **Test modules clash with the gpui glob.** A test module that does `use super::*` also pulls in `gpui_kit::*`, whose `test` macro shadows `#[test]`. Import names explicitly.
- **The Editor has no public gutter-marker API.** Show changed lines as `TextDecoration` background tints instead of 3 px gutter bars.

## Decision

| Surface | Choice | Why |
|---|---|---|
| Explore code preview | **gpui-kit `Editor`, read-only** (research Option A) | Scrolls as well as own rows. Opening costs one frame. Highlighting, Cmd+F, selection, copy and 100k-line files come free. Own rows would need a 0.2–1 s background parse plus hand-built selection and search. |
| Markdown preview | **gpui-kit `TextView`** with a Preview/Source toggle; Source uses the Editor | Renders well with local `TextViewStyle`. The one-time hitch is bounded by `MAX_BYTES`. |
| Changes diff | **Keep own `list` rows** in `diff.rs` and add syntax + word diff | Rows must interleave comments and the composer, and support line picking and split view, which the Editor can't. `list` scrolling costs ~3 ms/frame. |
| Diff engine | **imara-diff in-process** on old/new full text | We need full text anyway for correct tree-sitter colours. A line diff takes 0.6 ms, and full text is what makes "N unchanged lines" expandable. |
