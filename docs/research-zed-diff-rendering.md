# Research: how Zed renders diffs and file previews

Date: 2026-09-28. Zed at `1a28cff4b409169bac058bca40dfbfeb7621d19b`. Our stack: gpui-kit 0.6.6.

## TL;DR

- **Diffs:** Zed computes them in-process with **imara-diff 0.2** (`Algorithm::Histogram` + `postprocess_lines`). It has no git2; base text comes from the `git` CLI (`cat-file --batch`).
- **Word diff:** imara-diff over word/punctuation tokens. It only runs on hunks where the old and new line counts are equal and ≤ 5 lines.
- **Rendering:**
  - Only visible rows are laid out.
  - Each row is shaped once from syntax-colored `TextRun`s. GPUI caches shaped lines across frames.
  - Row backgrounds, word-diff backgrounds, and gutter bars are painted as quads.
  - Deleted lines are real rows from a base-text buffer that has its language set, so they are syntax-highlighted too.
- **Highlighting:** tree-sitter `highlights.scm` queries run over the visible byte range, and capture names map to theme styles with prefix fallback.
  - gpui-kit 0.6.6 already ships an **Apache-2.0 tree-sitter `SyntaxHighlighter`** behind cargo features.
- **File preview:** a Zed text preview is a read-only `Editor`. Of its display-map stack only tab expansion matters (soft wrap is off by default). Guards: 1024-char line truncation, NUL-sniff binary refusal, 6 GB cap. Plain text shows first; highlights arrive after the background parse.
  - gpui-kit 0.6.6 also ships an **Apache-2.0 readonly-capable code `Editor`** (virtualized, line numbers, tree-sitter, Cmd+F, selection/copy). Spike it before building our own (§4.9, Phase 3).
- **Markdown:**
  - Zed parses with pulldown-cmark 0.13 into a flat list of source-ranged events, reparsing fully in the background (highlighting included) on every change.
  - It rebuilds one non-virtualized element tree per frame, with cross-block selection mapped back to source offsets (§4.10).
  - gpui-kit's Apache `TextView` (markdown-rs, already compiled in) does the same job with incremental append and `gpui::list` virtualization. Use it for `.md` preview (Phase 3b).
- **Licensing:** every Zed crate that matters here (`buffer_diff`, `editor`, `multi_buffer`, `language`, `grammars` queries, `git_ui`, `image_viewer`) is **GPL-3.0-or-later**.
  - Reimplement the ideas; don't copy code.
  - Safe to use: `gpui` (Apache-2.0), imara-diff (Apache-2.0), tree-sitter and grammar crates (MIT).
- **Plan:**
  1. Word diff with imara-diff.
  2. Syntax highlighting via gpui-kit's tree-sitter features, run on the background executor.
  3. Virtualize explore: gpui-kit `Editor` if its look fits, else `uniform_list` with our row primitives.
     - 3b: markdown preview for `.md` files with gpui-kit `TextView`, themed from our tokens.
  4. Optional: diff full texts in-process and add expandable context.
- **Skip:** Zed's multibuffer, display map (except tab expansion), and split editor machinery.

### Citation legend

- `Z path:L` = `https://github.com/zed-industries/zed/blob/1a28cff4b409169bac058bca40dfbfeb7621d19b/path#LL`
- `R crate/path:L` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/crate/path` (the published source). Mirrors: https://docs.rs/gpui-kit/0.6.6, https://docs.rs/gpui-component/0.6.6.
- `P path:L` = this repo, relative to `packages/desktop/`.

## 0. What we have today

- **`P crates/git/src/git.rs`:**
  - Everything goes through the `git` CLI.
  - `file_diff` runs `git diff HEAD -- path`, with `--no-index /dev/null` for untracked files (L190-197). `parse` produces `Line { kind, old, new, text }` (L200-256).
  - `split()` pairs deletion runs with the addition runs that follow them for side-by-side view (L258-276).
- **`P crates/pocket/src/diff.rs`:**
  - A gpui `list` (variable height) of `Row::{Unified, Split, Composer, Comment}` (L36-62, L313-328).
  - Rows are plain `SharedString`s with a whole-line background (L16-23, L131-144). **No syntax or intra-line highlighting.**
  - Hunk headers are static text (L95-118).
- **`P crates/pocket/src/explore.rs`:**
  - A hand-rolled keyword heuristic highlighter (L67-108).
  - Renders **all** lines (up to 3000) eagerly as divs (L313-343). Not virtualized.
- **`P crates/pocket/src/main.rs:475-532`:** `refresh_git` already loads the diff and the file on `cx.background_executor()`. This is the right hook for all heavy work below.

## 1. Diff pipeline end to end

1. **Base text:**
   - `git` has no libgit2. `load_index_text` (`:path`) and `load_committed_text` (`HEAD:path`) are in `Z crates/git/src/repository.rs:793-803`.
   - Blobs are batched through `git cat-file --batch` (`:1817-1850`).
   - Diffs are opened per buffer in `Z crates/project/src/git_store.rs:1275` (unstaged) and `:1790` (uncommitted).
2. **Base buffer:**
   - `BufferDiff::new` creates a buffer for the base text and **sets its language**, so deleted text is syntax-highlighted like live text (`Z crates/buffer_diff/src/buffer_diff.rs:1618-1630`).
   - Base text updates: `set_base_text` `:2234-2304`.
3. **Line diff:**
   - `compute_hunks`, `Z crates/buffer_diff/src/buffer_diff.rs:1224-1281`. It runs on the background executor (`update_diff`, `:1978-2035`).

   ```rust
   let input = InternedInput::new(lines(diff_base.as_ref()), lines(buffer_text.as_str()));
   let mut diff = Diff::compute(Algorithm::Histogram, &input);
   diff.postprocess_lines(&input); // git slider/indent heuristic
   for hunk in diff.hunks() { sink.process_change(hunk.before, hunk.after); }
   ```

   - Dependency: `imara-diff` (`Z crates/buffer_diff/Cargo.toml:20`, workspace `Z Cargo.toml:672` = 0.2.0).
   - Upstream: https://github.com/pascalkuthe/imara-diff.
4. **Hunk model:**
   - `DiffHunk { range, buffer_range, diff_base_byte_range, secondary_status, buffer_word_diffs, base_word_diffs }`.
   - Status kind is `Added | Modified | Deleted`. The secondary status tracks staged vs unstaged. See `Z crates/buffer_diff/src/buffer_diff.rs:127-170`.
   - Hunks live in a `SumTree` keyed by anchors, so they survive edits.
5. **Word diff:**
   - Gated in `HunkSink::process_change` (`Z crates/buffer_diff/src/buffer_diff.rs:1337-1341`):

   ```rust
   && base_line_count == buffer_line_count
   && diff_options.max_word_diff_line_count >= base_line_count   // MAX_WORD_DIFF_LINE_COUNT = 5 (:20)
   ```

   - Options come from `build_diff_options` (`:1199-1222`) and the `word_diff_enabled` setting (`Z assets/settings/default.json:1673`).
   - The algorithm (`word_diff_ranges`) is in `Z crates/language/src/text_diff.rs:181-219`:
     - `tokenize` splits text into runs of the same `CharKind` (Word / Whitespace / Punctuation). Each punctuation char is its own token (`:383-411`).
     - `Word` = alphanumeric or `_` (`Z crates/language/src/buffer.rs:604-611`, `:6184-6250`).
     - Then Histogram runs over the token lists (`diff_internal`, `:348-373`).
   - Standalone defaults: `MAX_WORD_DIFF_LEN = 512` bytes, `MAX_WORD_DIFF_LINE_COUNT = 8` (`text_diff.rs:6-7`, gate `:334-346`).
6. **Other diff libs:**
   - `diffy` 0.4.2 applies patches only (`Z crates/language/Cargo.toml:82`).
   - `streaming_diff` is Zed's own crate for AI edits. Not relevant.
7. **Multibuffer:**
   - The project diff view is a `MultiBuffer` of excerpts, one per hunk, plus `excerpt_context_lines` (default 2) of context.
     - `Z crates/git_ui/src/diff_multibuffer.rs:77-95`, `:512-541`.
     - `Z assets/settings/default.json:494-496`: `expand_excerpt_lines` 5, `excerpt_context_lines` 2.
   - Deleted rows are spliced inline by `DiffTransform::{BufferContent, DeletedHunk}` (`Z crates/multi_buffer/src/multi_buffer.rs:715-727`).
   - The project diff calls `set_all_diff_hunks_expanded` (`:2334`).

## 2. Rendering

- **Visible rows only:**
  - `start_row`/`end_row` come from the scroll position (`Z crates/editor/src/element.rs:8779-8794`).
  - `layout_lines` shapes only those rows (`:3258-3336`, called at `:9180`).
- **Line shaping:**
  - `highlighted_chunks` → `LineWithInvisibles::from_chunks` → `TextRun`s → `window.text_system().shape_line` (`element.rs:7472-7720`, `:7627-7632`; `Z crates/gpui/src/text_system.rs:638-660`).
  - Lines are truncated at `MAX_LINE_LEN = 1024` (`Z crates/editor/src/editor.rs:302`, `element.rs:7518-7519`).
- **Shaping cache:**
  - `LineLayoutCache` keeps `previous_frame`/`current_frame` maps keyed by text, font size, and runs.
  - Unchanged lines are reused without reshaping (`Z crates/gpui/src/text_system/line_layout.rs:458-470`, `:523-560`, `:668-700`). We get this for free with `StyledText`.
- **Row backgrounds:**
  - Each diff row pushes a `LineHighlight` in filled or hollow style. Hollow means staged or unstaged, depending on the `hunk_style` setting (`element.rs:8870-8921`, `:6994-7006`).
  - Colors: `editor_diff_hunk_added_background` = added color at 0.16 opacity (light) or 0.12 (dark); hollow background 0.08, hollow border 0.48 (`Z crates/theme/src/default_colors.rs:11-35`, `:133-138`).
- **Word diff:** background quads over `version_control_word_added`/`version_control_word_deleted` ranges, filtered to the viewport (`element.rs:4781-4840`; colors `default_colors.rs:169`, `:175`).
- **Gutter:** `paint_gutter_diff_hunks` (`element.rs:5414-5475`):
  - Added: green bar.
  - Modified: yellow bar.
  - Deleted: small rounded wedge straddling the row boundary.
- **Split view:**
  - `SplittableEditor` runs two editors. The left one gets `add_inverted_diff`.
  - `Block::Spacer` blocks pad rows so both sides align (`Z crates/editor/src/split.rs:534-555`, `:1360`; `Z crates/editor/src/display_map/block_map.rs:391-395`).
  - It falls back to unified below `minimum_split_diff_width: 100` columns (`split.rs:1396`; `default.json:409`, `:416`).
- **GPUI primitives we can use directly** (all in gpui-pre 0.3.6, which gpui-kit wraps):
  - `uniform_list`: lazy fixed-height rows (`R gpui-pre-0.3.6/src/elements/uniform_list.rs:22`).
  - `list`: lazy variable-height rows.
  - `StyledText::with_highlights` / `with_default_highlights` (`R gpui-pre-0.3.6/src/elements/text.rs:418-470`).
  - `HighlightStyle { color, background_color, font_weight, .. }` (`R gpui-pre-0.3.6/src/style.rs:580-600`).
  - `combine_highlights(a, b)` merges syntax and word-diff layers (`style.rs:990`).

## 3. Syntax highlighting

- **Zed:**
  - Queries: tree-sitter `highlights.scm` per language, embedded in the `grammars` crate (`Z crates/grammars/src/grammars.rs:9-46`, e.g. `crates/grammars/src/rust/highlights.scm`).
  - Execution: a `QueryCursor` limited to the requested range with `set_byte_range` (`Z crates/language/src/syntax_map.rs:1229-1240`).
  - Chunks keep a capture stack, so the innermost capture wins (`Z crates/language/src/buffer.rs:525-563`).
  - Capture → theme mapping: `HighlightMap` is built once per theme (`Z crates/language/src/language.rs:1223-1267`, `crates/language_core/src/highlight_map.rs`).
  - `SyntaxTheme::highlight_id` does prefix fallback (`function.method` → `function`) (`Z crates/syntax_theme/src/syntax_theme.rs:87-101`).
  - Parsing: incremental. A sync parse is tried with a 1 ms timeout, otherwise it moves to the background (`Z crates/language/src/buffer.rs:1170-1174`, `:1930-2000`).
  - tree-sitter is pinned to a git rev. Grammars include tree-sitter-rust 0.24.2, python/go 0.25, json 0.24, bash 0.25.1, and a TypeScript fork (`Z Cargo.toml:880-903`).
- **gpui-kit 0.6.6 already has this, under Apache-2.0:**
  - Type: `gpui_kit::component::highlighter::SyntaxHighlighter` (gpui-component).
  - Feature gates:
    - `tree-sitter`, per-language `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-tsx`, `tree-sitter-javascript`, `tree-sitter-python`, `tree-sitter-go`, … (JSON comes with base `tree-sitter`, `R gpui-component-0.6.6/Cargo.toml:54`)
    - Or `tree-sitter-languages` for all of them (`R gpui-kit-0.6.6/Cargo.toml:62-65`, `:138`, `:174`, `:202`).
    - Without the feature, a wasm stub is compiled (`R gpui-component-0.6.6/src/highlighter/mod.rs`).
  - API (`R gpui-component-0.6.6/src/highlighter/highlighter.rs`):
    - `SyntaxHighlighter::new(lang)` (`:334`).
    - `update(edit: Option<InputEdit>, &Rope, timeout: Option<Duration>) -> bool` (`:529`).
    - `styles(&Range<usize>, &dyn HighlightStyleResolver) -> Vec<(Range<usize>, HighlightStyle)>` (`:1036-1061`).
  - Resolver trait: `fn style(&self, name: &str) -> Option<HighlightStyle>` (`R gpui-base-0.6.6/src/input/editor/highlighting.rs:13-15`). This is where we map capture names to our `SYN_*` theme constants.
  - `Rope` is re-exported from ropey 2.0.0-beta.1, which is already in our lockfile (`R gpui-base-0.6.6/src/input/mod.rs:108`).
  - Supports injections. Language names are resolved by `Language::from_name` (`R gpui-component-0.6.6/src/highlighter/languages.rs:174-250`).
  - Dependency weight: tree-sitter 0.26.13 plus the chosen grammar crates (`R gpui-component-0.6.6/Cargo.toml:379-381`, `:483`, `:507`).
  - Query sources: some come from the grammar crates' `HIGHLIGHTS_QUERY` (e.g. TOML/YAML); others are its own `.scm` files, which its READMEs trace to upstream grammar repos (`languages.rs:395-420`; `languages/rust/README.md`).
- **Fallback if we avoid gpui-component:**
  - Use tree-sitter 0.26 (0.27.0 is latest, MIT, MSRV 1.90) and `tree-sitter-highlight` 0.27 (MIT). Grammar crates export `LANGUAGE` + `HIGHLIGHTS_QUERY` (e.g. tree-sitter-rust `bindings/rust/lib.rs:35`, `:43`).
  - Versions/licenses: https://crates.io/crates/tree-sitter, https://crates.io/crates/tree-sitter-rust (0.24.2 MIT), tree-sitter-typescript 0.23.2, -javascript 0.25.0, -python 0.25.0, -go 0.25.0, -json 0.24.8.
  - Stay on 0.26 if gpui-component is also compiled, to avoid two tree-sitter versions.

## 4. File preview (single read-only file)

A Zed text preview is a full `Editor` on a `Buffer` (GPL). Only the parts below matter for a viewer. License tag per item: **[GPL]** = reference only, **[Apache]** = reusable.

### 4.1 Render path [GPL: editor; Apache: gpui]

- **Phases:** `request_layout` (`Z crates/editor/src/element.rs:8511`) → `prepaint` (`:8586+`) → `paint` (`:10112+`).
- **Metrics** (`element.rs:8620-8632`):
  - `line_height = style.text.line_height_in_pixels(rem_size)`.
  - `em_width`, `em_advance`, `em_layout_width` from the text system; `glyph_grid_cell = size(em_advance, line_height)`.
  - `text_width = bounds.width - gutter_dimensions.width`.
- **Visible rows** (`:8774-8790`):
  - Scroll position is fractional rows; the integer part is the top row. It is pixel-snapped first (`:8767-8772`).
  - `start_row = floor(scroll_y + clipped_top)`, `end_row = ceil(scroll_y + clipped_top + height / line_height)`, both clamped to `max_row`.
  - `row_infos`, highlights and background ranges are queried for that range only (`:8792-8795`).
  - Pending autoscroll is applied before rows are computed (`:8740-8765`).
- **Line layout:** `layout_lines` (`:3258-3336`):
  - `snapshot.highlighted_chunks(rows, language_aware, style)` → `LineWithInvisibles::from_chunks(chunks, style, MAX_LINE_LEN, rows.len(), ..)`.
  - `from_chunks` (`:7472-7720`) splits chunks on `\n`, pushes one `TextRun { len, font, color, background_color, underline, strikethrough }` per chunk (style = `text_style.highlight(chunk_style)`), then calls `window.text_system().shape_line(line, font_size, runs, None)` per line (`:7627-7632`).
  - Result: `LineFragment::Text(ShapedLine)` or inline `Element` (`:7449-7457`). Tabs/whitespace positions are kept for invisibles (`:7697-7720`).
  - Hit-testing on `ShapedLine`: `index_for_x`, `closest_index_for_x`, `x_for_index` (`Z crates/gpui/src/text_system/line_layout.rs:61`, `:78`, `:108`; same in `R gpui-pre-0.3.6/src/text_system/line_layout.rs:61`, `:108`).
- **No reshaping:** `LineLayoutCache` swaps `previous_frame`/`current_frame`; a line with the same text, font size and runs is moved, not reshaped (`line_layout.rs:458-470`, `:523-560`, `:668-700`). `StyledText` goes through the same cache.
- **Paint order** (`:10112+`): mouse listeners (`:10147`) → `paint_background` (`:10172`) → `paint_line_numbers` (`:10178`) → `paint_text` (`:10181`). `paint_text` (`:5914`) = line backgrounds → highlighted ranges (selections, search) → lines → cursors (`:5969-5976`).

### 4.2 DisplayMap stack [GPL: editor]

`DisplayMap` (`Z crates/editor/src/display_map.rs:1-15`, struct `:215-244`) chains buffer → inlay → fold → tab → wrap → block.

| Layer | Does | Viewer needs? |
|---|---|---|
| InlayMap | LSP inlay hints, inline completions | No |
| FoldMap | folded ranges → placeholder | No |
| TabMap | hard tabs → spaces to next stop; chunks tagged `is_tab` (`tab_map.rs:323`) | **Yes** |
| WrapMap | soft wrap via gpui `LineWrapper` (`Z crates/gpui/src/text_system/line_wrapper.rs:17-27`) | No (default `"soft_wrap": "none"`, `Z assets/settings/default.json:1650`) |
| BlockMap | custom blocks (diagnostics, diff spacers) | No |

- **TabMap details** (`Z crates/editor/src/display_map/tab_map.rs`):
  - `tab_len = tab_size - ((col - 1) % tab_size)` (`expand_tabs`, `:417-442`). Default `tab_size` 4, `hard_tabs` false (`default.json:1658`, `:1656`).
  - Guard: `MAX_EXPANSION_COLUMN = 256` (`:11`). A tab past column 256 renders as one space (`:84-90`).
- **WrapMap guard:** rewrap is sync below `WRAP_YIELD_ROW_INTERVAL = 100` rows, otherwise background with a 5 ms `block_with_timeout`, then async (`wrap_map.rs:34`, `:267-330`).
- **Minimal equivalent:** one function that expands tabs to 4-column stops (with a column cap) and remaps highlight byte ranges. Row index = line index.

### 4.3 Syntax highlight → text runs [GPL: language, editor]

- **Chunks:** `Buffer::chunks` (`Z crates/language/src/buffer.rs:4149-4178`) yields `Chunk { text, syntax_highlight_id, .. }`. The capture stack keeps the innermost capture (`:5863-5907`).
- **Run cache:** highlight runs are cached per 50-row chunk (`MAX_ROWS_IN_A_CHUNK`, `:158`) in `TreeSitterData.highlights_by_chunks` (`:151-175`, `cached_highlight_runs` `:4180-4236`, `compute_chunk_highlights` `:4239+`). Chunks over 64 KB (`MAX_BYTES_TO_HIGHLIGHT_IN_A_CHUNK = 4 * MAX_BYTES_TO_QUERY`, `:159`; `MAX_BYTES_TO_QUERY = 16 KB`, `Z crates/language/src/syntax_map.rs:30`) skip the cache.
- **Id → style:** `DisplaySnapshot::highlighted_chunks` (`display_map.rs:1847-1935`) maps `syntax_highlight_id` via `editor_style.syntax.get(id)`, then merges text/diagnostic highlights with `.reduce(|a, h| a.highlight(h))` (`combined_highlights`, `:1946+`). Output is `HighlightedChunk { text, style, is_tab }`, which feeds §4.1's `TextRun`s.
- **Off-thread parse** (`reparse`, `buffer.rs:1930-2000`):
  1. Interpolate the old tree over edits, so stale highlights shift instead of vanishing.
  2. If allowed to block, try a sync parse with `sync_parse_timeout` (1 ms).
  3. Otherwise `background_spawn` the parse; re-parse if the version moved meanwhile.
  4. `did_finish_parsing` (`:2007-2029`) swaps the tree, emits `BufferEvent::Reparsed`, calls `cx.notify()`.
- **Before the first parse:** no tree, so chunks carry no highlight id and the file renders in the plain foreground color until the notify (inferred from the no-tree path + `did_finish_parsing`).

### 4.4 Gutter [GPL: editor]

- **Width** (`Editor::gutter_dimensions`, `Z crates/editor/src/editor.rs:12190-12275`):
  - Numbers column = digits of the widest line number × digit advance (`max_line_number_width`, `:12185-12188`), floored at `min_line_number_digits` (4, `default.json:753`) so width doesn't jump.
  - Left padding 1–4 × `ch_width` (git gutter/runnables); right padding 4 × `ch_width` when folds show.
- **Layout** (`layout_line_numbers`, `element.rs:2934-3050`): one shaped line per visible row, right-aligned: `x = gutter.width - shaped.width - right_padding`, `y = ix * line_height - (scroll_top % line_height)`.
- **Colors** (`LineNumberStyle`, `element.rs:108-139`): active row → `editor_active_line_number`; diff rows → version-control colors; else `editor_line_number`.
- **Paint** (`paint_line_numbers`, `:5359-5412`): IBeam hover; click selects the line.
- **Current line** (`paint_background`, `:5102-5165`): gutter bg, text bg, then `editor_active_line_background` over the active rows, scoped by `current_line_highlight` (`all` default, `default.json:349`).
- **Folds:** `layout_crease_toggles` (`:3051`). Skip.

### 4.5 Scrolling [GPL: editor; Apache: gpui, gpui-base]

- **Model** (`Z crates/editor/src/scroll.rs`):
  - `ScrollAnchor { anchor, offset: Point<f64> }` (`:35-57`); scroll position = anchor row + offset, in rows.
  - `ScrollManager` (`:139-160`): anchor, `vertical_scroll_margin` (3, `default.json:782`), autoscroll request, trackpad axis lock, scrollbar auto-hide (`SCROLLBAR_SHOW_INTERVAL = 1s`, `:28`).
  - `scroll_beyond_last_line: one_page` (`default.json:780`).
- **Autoscroll** (`scroll/autoscroll.rs:16-30`, `:100-110`): `Fit`, `Newest`, `Center` (default), `Focused`, `Top`, `Bottom`, relative variants.
- **Wheel** (`paint_scroll_wheel_listener`, `Z crates/editor/src/element/mouse.rs:478-570`): line deltas × `line_height`, pixel deltas as-is; `new_y = (y * line_height - delta * sensitivity) / line_height`, clamped to max. Cmd+wheel zooms font.
- **Pixel → row/col** (`PositionMap::point_for_position`, `element.rs:10825-10870`): `row = y / line_height + scroll_y`; `col = line.index_for_x(x + scroll_x * em_layout_width)`.
- **Scrollbars:** `layout_scrollbars` (`:1483-1530`), `paint_scrollbars` (`:6169`, thumb `:6218`); markers (search hits, hunks) collected fast (`:6392`) or async (`:6414-6598`).
- **For us:** rows are fixed height and there is no wrap, so gpui `uniform_list` gives the same visible-range math for free: it lays out only `range` rows from its scroll offset (`R gpui-pre-0.3.6/src/elements/uniform_list.rs:22`). `UniformListScrollHandle::scroll_to_item(ix, ScrollStrategy::Center)` (`:80`, `:84`, `:150`) replaces autoscroll. A custom element only pays off for char-level selection hit-testing across rows (§4.6) or overlays spanning rows.

### 4.6 Selection, copy, search [GPL: editor, search, workspace, project]

- **Read-only:** a flag on `Editor` (`set_read_only`, `editor.rs:3417`); selection, copy and search stay live.
- **Mouse:** `mouse_left_down` / `mouse_dragged` / `mouse_up` (`element/mouse.rs:575`, `:1009`, `:831`); dragging past the edge autoscrolls (`:1029`).
- **Selection paint** (`paint_highlighted_range`, `element.rs:6607-6660`): builds `HighlightedRange { start_y, line_height, lines: [HighlightedRangeLine { start_x, end_x }], color, corner_radius }` (`:11161-11175`) from `x_for_index` per visible row.
- **Copy** (`do_copy`, `Z crates/editor/src/clipboard.rs:581-660`): an empty selection copies the whole line plus `\n`.
- **Search:**
  - `SearchableItem` trait (`Z crates/workspace/src/searchable.rs:79`), implemented for `Editor` at `Z crates/editor/src/items.rs:1717`. Bar UI in `crates/search` (`buffer_search.rs`, GPL, `Z crates/search/Cargo.toml:6`).
  - `find_matches` (`items.rs:2010-2080`): background task, range split into `num_cpus` chunks, each searched with `SearchQuery` = aho-corasick for text, fancy-regex for regex (`Z crates/project/src/search.rs:1`, `:4`, `:78`; workspace deps `Z Cargo.toml:526`, `:631`; Unlicense-or-MIT and MIT).
  - Matches are stored as background highlights (`search_match_background`, active `search_active_match_background`, `items.rs:1750-1760`) and painted like selections.
  - `activate_match` unfolds, autoscrolls `center` or `fit`, selects (`items.rs:1865-1872`).
- **Minimal:** line-granular selection first; matches = `(line, byte_range)` from a background `match_indices`; paint as `background_color` spans; Enter/Shift+Enter → `scroll_to_item(.., Center)`.

### 4.7 Large files, long lines, binary, images [GPL: worktree, language, editor, image_viewer, svg_preview, workspace; Apache: gpui]

- **Long lines:** truncated at `MAX_LINE_LEN = 1024` bytes on a char boundary (`Z crates/editor/src/editor.rs:302`, `element.rs:7666-7674`). Soft wrap is off by default; with it on, wrap is async (§4.2).
- **Size:** `FILE_SIZE_MAX` 6 GB, commented as a workaround for memory use (#27283); error "File is too large to load" (`Z crates/worktree/src/worktree.rs:1712-1726`). No other size gate. Highlighting is bounded by the 16 KB query / 64 KB chunk limits (§4.3).
- **Binary:** first 1024 bytes sniffed (`FILE_ANALYSIS_BYTES`, NUL + known headers, `Z crates/language/src/file_content.rs:5`, `:106-160`); `ensure!(.. != Binary, "Binary files are not supported")` (`worktree.rs:7305-7315`).
- **Error view:** `InvalidItemView` shows the message and an "Open in Default App" button (`OpenWithSystem`) (`Z crates/workspace/src/invalid_item_view.rs:13-40`, `:97-104`).
- **Images** (`Z crates/image_viewer/src/image_viewer.rs`, GPL per `Cargo.toml:6`):
  - Formats: `Img::extensions()` = avif, jpg/jpeg, png, gif, webp, tif/tiff, tga, dds, bmp, ico, hdr, exr, pbm/pam/ppm/pgm, ff/farbfeld, qoi, svg (`Z crates/gpui/src/elements/img.rs:212-217`). The image store excludes svg, so svg opens as text (`Z crates/project/src/image_store.rs:247-281`).
  - Zoom: `MIN_ZOOM 0.1`, `MAX_ZOOM 20`, `ZOOM_STEP 1.1` (`:55-59`); actions ZoomIn/ZoomOut/ResetZoom/FitToView (`:43-49`, `:210-230`).
  - Fit: `min(cw / iw, ch / ih, 1.0)`, never upscales (`:232-239`). Zoom keeps the point under the cursor fixed (`:263-285`); Cmd/Ctrl+wheel zooms (`:289-300`).
  - Render: centered, absolutely positioned `img(image)` with pan offset over a checkerboard scaled with zoom (`:440-500`). Status bar shows W×H + size (`image_info.rs:40-55`).
- **gpui `img()` does the heavy lifting** [Apache]: `ImageSource: From<PathBuf>` (`R gpui-pre-0.3.6/src/elements/img.rs:101`); loads via `fs::read` (`:634`), `image::guess_format` (`:669`), animated GIF frames (`:672`), svg fallback via `svg_renderer.render_single_frame` (`:740-741`, resvg/usvg `Z crates/gpui/Cargo.toml:82-83`). Builders `object_fit` (default `Contain`, `:140`, `:159`), `with_fallback` (`:166`), `with_loading` (`:173`). `checkerboard` fill: `R gpui-pre-0.3.6/src/color.rs:841`.

### 4.8 Markdown / SVG previews [GPL: svg_preview]

- **Markdown:** see §4.10.
- **SVG:** `svg_preview` [GPL] rasterizes on a background task with `svg_renderer().render_single_frame`, then shows `img(image)` (`Z crates/svg_preview/src/svg_preview_view.rs:104-113`, `:292`). For us, `img(path)` already rasterizes SVG (§4.7).

### 4.9 gpui-kit 0.6.6 reusable pieces [Apache: gpui-base, gpui-component, gpui-pre]

- **Code editor, readonly-capable:** `gpui_kit::component::input::{Editor, EditorState}`.
  - `EditorState = InputBaseState<EditorMode>` (`R gpui-base-0.6.6/src/input/editor/mod.rs:5`). `EditorState::new` (`R gpui-base-0.6.6/src/input/base/state.rs:9220-9235`) documents: line numbers, tab size 2 soft tabs, indent guides, search enabled, "large text up to about 50K lines".
  - Builders: `.language()` `:9240`, `.folding()` `:9265`, `.line_number()` `:9322`, `.searchable()` `:9097`, `.soft_wrap()` `:9110` (default on), `set_value` `:897` (UI thread), `set_readonly` `:1065`, `show_whitespaces` `:1112`, `set_highlighter` `:772`, `set_cursor_position` `:1240`, `set_search_query` (`editor/search.rs:172`).
  - Element: `Editor::new(&state).readonly(true)` "still can be focused, selected and copied" (`R gpui-component-0.6.6/src/input/editor.rs:81`); `EDITOR_LINE_HEIGHT = 1.5` (`:14`).
  - Virtualized: `calculate_visible_range` (`R gpui-base-0.6.6/src/input/base/element.rs:954-1017`); highlights requested per visible line group (`:1540-1570`); lines over `MAX_HIGHLIGHT_LINE_LENGTH = 10_000` stay plain (`:54`, `:1576`). Gutter `layout_line_numbers` (`:1021-1060`).
  - Parsing (`R gpui-component-0.6.6/src/highlighter/input_adapter.rs:55-130`): 2 ms sync try, always background above 256 KB, 150 ms debounce, cancel flag via tree-sitter progress callback. Same shape as Zed's §4.3.
  - Extra ranges: `TextDecorationCollection::set/append/clear` of `TextDecoration { range, style: HighlightStyle }` (`R gpui-base-0.6.6/src/input/editor/decorations.rs:1-80`, `:238`). Fits search hits and change tints. **No gutter-marker API.**
  - Colors: `InputEditorStyle` (`R gpui-base-0.6.6/src/input/editor/highlighting.rs:89-104`) is rebuilt from `cx.theme()` + `cx.theme().highlight_theme` each render (`R gpui-component-0.6.6/src/input/input.rs:494-525`). To theme it: set `Theme::global_mut(cx).highlight_theme` to a `HighlightTheme { style: HighlightThemeStyle { editor_*, syntax: SyntaxColors { keyword, function, string, comment, .. } } }` (`R gpui-component-0.6.6/src/highlighter/registry.rs:113-135`, `:436-487`; field `theme/mod.rs:115`). Hook: `P crates/theme/src/theme.rs:131-140`.
- **`virtual_list`** (h/v, variable sizes) (`R gpui-base-0.6.6/src/virtual_list.rs:1-8`, `:139-215`).
- **Scrollbar:** `ScrollbarHandle` for `ScrollHandle`, `UniformListScrollHandle`, `ListState` (`R gpui-base-0.6.6/src/scrollbar.rs:69`, `:84`, `:102`, `:121`); `vertical_scrollbar(&handle)` (`R gpui-component-0.6.6/src/scroll/scrollable.rs:33`).
- **`SelectableText` / `TextSelectionLayer`** (`R gpui-base-0.6.6/src/selectable_text.rs:11-70`; `text_selection.rs:44-62` virtualized keys): wraps plain `StyledText::new(text)`, no highlights. Doesn't fit syntax rows.
- **gpui-pre primitives:** `StyledText::layout() -> &TextLayout` (`R gpui-pre-0.3.6/src/elements/text.rs:412`), `TextLayout::index_for_position` / `position_for_index` (`:830`, `:864`), `InteractiveText` (`:981`); `uniform_list` `.track_scroll` (`uniform_list.rs:683`), `.with_width_from_item` (`:622`), `.with_horizontal_sizing_behavior` (`:636`).

### 4.10 Markdown rendering [GPL: markdown, markdown_preview, mermaid_render; Apache: gpui-base, gpui-component; MIT: pulldown-cmark, markdown 1.0.0]

Zed has a single shared renderer, `crates/markdown` [GPL] (8k lines in `markdown.rs`). `markdown_preview` [GPL] is a thin view around it. The older `markdown_preview` parser/renderer pair is gone at this SHA: the crate holds only `markdown_preview.rs`, `markdown_preview_settings.rs` and `markdown_preview_view.rs`, and depends on `markdown` (`Z crates/markdown_preview/Cargo.toml:25`).

#### 4.10.1 Parsing [GPL: markdown; MIT: pulldown-cmark]

- **Parser:** pulldown-cmark 0.13, `default-features = false` (`Z Cargo.toml:797`; MIT). Its `offset_iter` gives each event a source byte range (`Z crates/markdown/src/parser.rs:260`).
- **Options** (`PARSE_OPTIONS`, `parser.rs:13-23`):
  - On: tables, footnotes (plus old footnotes), strikethrough, task lists, smart punctuation, heading attributes, `+++` metadata, GFM (alerts), super/subscript. YAML front matter only if `render_metadata_blocks` is set (`:252-256`).
  - Explicitly off: **math**, definition lists, wikilinks. A test fails if pulldown-cmark adds an option nobody has triaged (`:984-1010`).
- **Own event model:** a flat `Vec<(Range<usize>, MarkdownEvent)>` (`parser.rs:32-46`). There is no AST.
  - `MarkdownEvent`: `Start`/`End(MarkdownTag)`, `Text` (the text is the source range itself), `SubstitutedText(String)` (entities, smart quotes), `Code`/`SubstitutedCode`, `Html`, `InlineHtml`, `FootnoteReference`, `SoftBreak`, `HardBreak`, `Rule`, `TaskListMarker`, `RootStart`, `RootEnd(ix)` (`:762-799`).
  - `RootStart`/`RootEnd` wrap each top-level block (`ParseState::push_event`, `:60-99`). This is what powers "active block" markers and scroll sync.
  - `CodeBlockKind::{Indented, Fenced, FencedLang(name), FencedSrc(path#L1-2)}`. `FencedSrc` lets agents cite file ranges (`:886-896`). `CodeBlockMetadata { content_range, line_count, is_fenced_closed }` (`:898-902`).
  - Consecutive `Text` events are merged, and bare URLs are auto-linked with `linkify` (MIT/Apache) outside links and code (`:519-560`, `:722-760`).
  - HTML blocks are parsed with html5ever into a small tree of heading/list/table/blockquote/paragraph/image (`html/html_parser.rs:1-30`).
- **Off-thread:** `Markdown::parse` → `start_background_parse` (`Z crates/markdown/src/markdown.rs:1197-1398`). The background task also:
  - resolves code-block languages via `LanguageRegistry` (`:1280-1290`),
  - decodes `data:` images (`:1305-1330`),
  - pre-computes code-block syntax highlights (`compute_code_block_highlights`, `:1625-1700`).
  - If an edit arrives mid-parse it only sets `should_reparse`, so at most one parse runs and at most one is queued (`:1215-1221`, `:1381-1384`).
  - `reset` keeps the old parse on screen until the new one lands (`:1030-1056`).
- **Not incremental:** `append` concatenates onto the source and reparses the **whole** document (`:985-993`).
- **Streaming (agent chat):** `StreamingTextBuffer` reveals pending bytes every 16 ms, aiming to reveal a pending burst within 200 ms. Each tick calls `markdown.append` (`Z crates/acp_thread/src/acp_thread.rs:2545-2575`, `:829`). Full reparse plus coalescing keeps this cheap enough for chat-sized messages.

#### 4.10.2 Rendering [GPL: markdown; Apache: gpui]

- **Not virtualized.** `MarkdownElement::request_layout` walks all events and builds a fresh div tree every frame (`markdown.rs:2577-3313`). `prepaint`/`paint` lay out and paint that tree, then attach mouse and key handlers (`:3315-3390`). Laid-out text still hits gpui's line cache (§4.1).
- **Builder** (`MarkdownElementBuilder`, `:3692-3830`): a div stack plus a text-style stack. Inline text accumulates into a `PendingLine { text, runs: Vec<TextRun>, source_mappings }` (`:3795-3803`).
  - `push_text` pushes one `TextRun` per chunk from the current style stack (`:4047-4097`).
  - `flush_text` turns the line into `StyledText::new(text).with_runs(runs)` wrapped in a `RenderedLineElement`. That element paints code chips, then backgrounds, selection/search highlights, then glyphs (`:4192-4304`).
- **Block → element** (`:2662-3300`):

| Markdown | gpui |
|---|---|
| paragraph / heading | div + `heading_level_styles` refinement (`:2711-2735`) |
| blockquote | div with left border; GFM alert colors from `block_quote_kind_colors` (`:2736`) |
| code block | div (`code_block` style), optional horizontal `ScrollHandle`, copy/wrap buttons absolute top-right, `visible_on_hover` (`:2744-2856`, `:3090-3140`) |
| list / item | `div().pl_2p5()`; bullet `•`, `N.` or a ui `Checkbox` for task items (clickable via `on_checkbox_toggle`) (`:2864-2907`) |
| table | a grid built by `TableState` (`:2983-3030`, `:3613+`) |
| rule | `div().border_b_1()` (`:3269-3279`) |
| emphasis / strong / strike | pushed `TextStyleRefinement` (`:2908-2925`) |
| link | `link` style (or `link_callback`) + a `RenderedLink` source range (`:2926-2938`) |
| inline code | `inline_code` style + chip background painted behind the glyphs (`:1871+`) |
| image | only if a `data:` image was decoded or `image_resolver` returns an `ImageSource`; otherwise alt text (`:2678-2710`) |
| HTML | parsed blocks rendered by `render_html_block`; inline `<code>` and `<br>` handled; comments skipped; other inline HTML shown as text (`:2857-2863`, `:3230-3268`) |
| mermaid | `mermaid_render` crate [GPL] on `merman` 0.8-alpha, with zoom (`Z crates/mermaid_render/Cargo.toml:1-25`; `markdown.rs:2745-2780`) |

- **Theme hook:** `MarkdownStyle` (`:109-136`) carries:
  - text styles: `base_text_style`, `inline_code`, `block_quote`, `link`, `heading` + `heading_level_styles`;
  - colors: `rule_color`, `block_quote_border_color`, `selection_background_color`, `syntax: Arc<SyntaxTheme>`;
  - spacing: `paragraph_spacing` (8 px), `paragraph_line_height` (1.3 rem), `table_cell_padding`;
  - behavior flags.
  - `MarkdownStyle::themed(MarkdownFont::{Agent, Editor, Preview})` fills it from the active theme (`:170-200`).
- **Code highlighting:** reuses the editor's `Language` (`language.highlight_text_resolved`) during the background parse. Runs are keyed by source offset. If the cached runs are stale when rendering, it re-highlights on the fly (`:4057-4090`).
- **Selection/copy:** `Selection { start, end, reversed, pending, mode: Character | Word | Line | All }` is stored in **source offsets** (`:1420-1480`).
  - `RenderedText` keeps every `RenderedLine` with `source_mappings` for position → source index hit-testing across blocks (`:4318`, `:4597`, `:4700`, `:4828-4947`).
  - Click count 1/2/3/4 → char/word/line/all (`:2354-2385`).
  - `Copy` writes the rendered text for the range (`:1133-1139`). `CopyAsMarkdown` writes `rebalanced_markdown_for_selection`, which re-closes open `**`/`` ` `` spans (`:1141-1153`, `:1599`; `selection.rs`).
- **Links:** hovering a link or footnote shows a PointingHand cursor, otherwise an IBeam (`:2263-2293`). A link opens on mouse-up only if the up lands on the same link as the down, via `on_url_click`, defaulting to `cx.open_url` (`:2455-2480`).
- **Search:** `set_search_highlights` ranges are painted per line like selection (`:1092-1130`, `:3714-3760`).

#### 4.10.3 `markdown_preview` flow [GPL]

- **Open:** actions `OpenPreview`, `OpenPreviewToTheSide`, `OpenFollowingPreview` (follows the active editor) (`Z crates/markdown_preview/src/markdown_preview.rs:7-49`; registration `markdown_preview_view.rs:193-215`).
- **Setup:** `Markdown::new_with_options(.., MarkdownOptions { parse_html, render_mermaid_diagrams, parse_heading_slugs, render_metadata_blocks })` (`markdown_preview_view.rs:372-395`).
- **Editor → preview:** subscribes to `EditorEvent::{Edited, BufferEdited, DirtyChanged, BuffersEdited}` (`:541-560`).
  - Updates are debounced by `REPARSE_DEBOUNCE = 200ms` (`:58`, `:660-670`).
  - It then snapshots the buffer rope to a string and calls `markdown.reset(contents)` (`:672-705`).
- **Scroll sync:** on `SelectionsChanged`, the cursor's byte offset becomes a source index.
  - `request_autoscroll_to_source_index` scrolls to it, and `set_active_root_for_source_index` highlights the enclosing top-level block (`:566-580`, `:708-752`).
  - If a parse is pending, the scroll waits for it (`markdown.rs:995-1005`, `:1388-1392`).
- **Preview → editor:** `on_source_click` moves the editor cursor (`:1145-1156`). Toggling a checkbox edits the buffer (`:1157-1165`).
- **Container:** `div().overflow_y_scroll().track_scroll(..)` around a single `MarkdownElement` (`:1785-1800`). The whole document is laid out.
- **Images:** `resolve_preview_image` maps `http(s)` to `Resource::Uri` and relative or project paths to `Resource::Path` (`:1496-1561`).

#### 4.10.4 Other users of `markdown`

Crates depending on it (their `Cargo.toml`): acp_thread, acp_tools, agent_ui, diagnostics, edit_prediction_ui, editor, extensions_ui, git_ui, markdown_preview, project_panel, repl, ui_prompt, workspace, and others. Examples:

- LSP hover popovers: `Markdown::new` + `MarkdownElement::new(markdown, hover_markdown_style(..))` (`Z crates/editor/src/hover_popover.rs:665`, `:1310`).
- Agent thread messages (`Z crates/agent_ui/src/conversation_view.rs:1496`, `:3522`).

It is Zed's single reusable rich-text primitive.

#### 4.10.5 gpui-kit `TextView` [Apache: gpui-base, gpui-component; MIT: markdown 1.0.0]

- **Already compiled in:** `gpui-base` depends on `markdown` 1.0.0 (= markdown-rs by wooorm, MIT, `R markdown-1.0.0/Cargo.toml:45-46`) and html5ever 0.27 unconditionally (`R gpui-base-0.6.6/Cargo.toml:87`, `:98`, `:102`). No new crates needed.
- **Entry points:**
  - `gpui_base::text::markdown(src)` / `TextView::markdown(id, src)` / `TextView::new(&Entity<TextViewState>)` (`R gpui-base-0.6.6/src/text/mod.rs:34-41`, `text_view.rs:161-205`).
  - Re-exported as `gpui_kit::component::text::*` (`R gpui-component-0.6.6/src/text/mod.rs:1-16`).
- **Parser:** `markdown::to_mdast` with `ParseOptions::gfm()` + `math_text` + `math_flow` (+ frontmatter/MDX opt-in) (`text/markdown_ext.rs:392-409`, `format/markdown.rs:16-21`).
  - The mdast is mapped to its own `BlockNode` tree: Root, Paragraph, Heading, Blockquote, List, ListItem{checked}, CodeBlock, Custom, Table, Break, HorizontalRule, Definition (`text/node.rs:44-90`). Source spans are kept.
  - Unclaimed math renders literally (`$5` stays `$5`); block math becomes a code block (`format/markdown.rs:290-300`, `:467-471`).
  - Raw HTML goes through its html5ever parser (`:308`, `:472`).
- **Off-thread + incremental:**
  - A background `UpdateFuture` parses. Results are dropped if the revision moved (`text/state.rs:154-203`).
  - Full replaces ≤ 4 KB parse synchronously so first layout has the exact height; up to 64 queued updates are coalesced (`:35-40`).
  - `push_str` re-parses only **the last block + appended text** (`parse_content`, `:950-1003`). That is cheaper than Zed's full reparse for streaming.
  - Optional stream fade-in (`TextViewMotion`, `stream_fade.rs:1-10`).
- **Virtualization:** `.scrollable(true)` renders top-level blocks through `gpui::list` with a scrollbar (`text_view.rs:247-260`, `document.rs:212`). `ListState::measure_all` measures every block up front to keep the scrollbar stable (`state.rs:215-221`).
  - Non-scrollable mode lays out everything. `max_lines(n)` clamps.
- **Line reuse:** `Inline` hands the same `StyledText` to the next frame, keeping `TextLayout` (`text/inline.rs:226-240`).
- **Selection/copy:**
  - `.selectable(true)` is the default for `TextView::markdown` (`text_view.rs:191`). Selection works across blocks.
  - `cmd-c`/`cmd-a` are bound in context `TextView` (`state.rs:42-52`); copy is trimmed (`text_view.rs:618-625`).
  - `SelectionFormat::{Plain, Source}`: Source rebuilds markdown for partial selections (`state.rs:68-78`, `:383-420`).
- **Links:** `.on_link_click(|url, event, window, cx|)`; the default is `cx.open_url` on left/middle click (`text_view.rs:77-97`, `:333`).
- **Code blocks:**
  - `.code_block_highlighter(Fn(&CodeBlock) -> Vec<(Range, HighlightStyle)>)` or app-wide via `TextViewDefaults::with_code_block_highlighter` (`text_view.rs:29-70`, `:306`).
  - Called lazily **on the UI thread** at first render of each block, then cached per block (`node.rs:1482-1505`).
  - gpui-component installs one backed by `SyntaxHighlighter` + `theme.highlight_theme` (`R gpui-component-0.6.6/src/text/mod.rs:64-125`).
  - `.code_block_actions(..)` adds your own buttons, e.g. a copy button (`text_view.rs:291`, `node.rs:1590-1600`).
- **Images:** `img(ImageNode::source())`. `data:` URLs are decoded; everything else is `ImageSource::Uri`, i.e. HTTP only, with no filesystem access (`node.rs:465-478`, `:1808-1840`). **Relative images in a repo won't load** unless rewritten, e.g. with a `MarkdownPlugin` for `Node::Image` (`markdown_ext.rs:44-75`).
- **Theming:**
  - `TextViewStyle` exposes: foreground, muted, link, selection, code background, border, paragraph gap, heading sizes, `code_block`/`inline_code`/`table`/`table_head`/`table_cell` refinements (`text/style.rs:14-215`).
  - Set per view with `.style(..)` or globally with `TextViewDefaults::with_style(..).install(cx)`.
  - gpui-component derives defaults from its `Theme` (`R gpui-component-0.6.6/src/text/mod.rs:64-73`).
- **Extensibility:** block/inline `MarkdownPlugin`s parse off-thread and render on the UI thread (`markdown_ext.rs:26-75`).
- **Gaps vs Zed:**
  - no source ↔ editor scroll sync or active-block markers;
  - no local images;
  - code highlight on the UI thread (cached);
  - no mermaid;
  - no footnote/heading-slug navigation found;
  - no search-highlight API.
  - Streaming, virtualization and selection formats are ahead of Zed's.
- **vs our own pulldown-cmark renderer** (pulldown-cmark 0.13.4, MIT): we would have to rebuild the event model, builder, per-line `StyledText`, table layout, cross-block selection and hit-testing. In Zed that is thousands of lines of GPL code we can't copy. TextView gives all of it under Apache.

#### 4.10.6 Our app today

- No markdown rendering anywhere: no `markdown`/`TextView`/`pulldown` use in `packages/desktop` (search over `*.rs`). `.md` files show as code with the heuristic highlighter (`P crates/pocket/src/explore.rs:49`).
- Agents run in terminals. Their output is terminal grid rows painted as `StyledText` runs (`P crates/pocket/src/termview.rs:86`), not markdown transcripts. So markdown is needed only for the explore preview of `.md` files, for now.

## 5. Performance

| Technique | Zed | Cite | Port? |
|---|---|---|---|
| Diff + parse off UI thread | background executor | `buffer_diff.rs:1978-2035`, `buffer.rs:1930-2000` | Yes (already do for diff) |
| Visible-rows-only layout | editor element | `element.rs:8779-8794` | Yes, `uniform_list` / `list` |
| Shaped-line cache | gpui frame cache | `line_layout.rs:523-560` | Free |
| Highlight only visible byte range | `set_byte_range` | `syntax_map.rs:1229-1240` | Optional; precompute is fine for ≤ ~1 MB |
| Parse timeout | 1 ms sync, then bg | `buffer.rs:1170-1174` | Use `update(.., Some(timeout))` |
| Long-line truncation | 1024 chars | `editor.rs:302` | Yes |
| Word diff gates | ≤ 5 equal lines; 512 B | `buffer_diff.rs:20`, `text_diff.rs:6` | Yes |
| Incremental reparse | anchors + `InputEdit` | `buffer.rs:1930-2000` | No (read-only views) |

## 6. Licensing

The repo has no LICENSE file yet. GPL-3.0 code cannot be copied into a non-GPL project, so treat every GPL crate as **read-only reference; reimplement**.

| Crate | License | Source | Verdict |
|---|---|---|---|
| Zed README | "primarily GPL-3.0-or-later, Apache-2.0 components where marked" | `Z README.md:30-40` | |
| buffer_diff, language, multi_buffer, editor, git, git_ui, project, worktree, workspace, image_viewer, search, svg_preview, markdown, markdown_preview, mermaid_render, text, rope, theme, ui, streaming_diff | GPL-3.0-or-later | each `Cargo.toml` `license` (e.g. `Z crates/buffer_diff/Cargo.toml:6`) | Reimplement ideas |
| language_core, grammars (incl. Zed's `highlights.scm`) | GPL (LICENSE-GPL symlink, `publish = false`) | crate dirs | Do not copy queries |
| syntax_theme | GPL declared; both LICENSE files symlinked | `crates/syntax_theme/Cargo.toml` | Treat as GPL |
| gpui, sum_tree, collections, util, refineable | Apache-2.0 | `Z crates/gpui/Cargo.toml:9` | Copy OK (with notice) |
| gpui-kit / gpui-component / gpui-base 0.6.6 (incl. `Editor`, `SyntaxHighlighter`, `virtual_list`, scrollbar, `TextView`) | Apache-2.0 | `R gpui-kit-0.6.6/Cargo.toml:35`, `R gpui-component-0.6.6/Cargo.toml:34`, `R gpui-base-0.6.6/Cargo.toml:34` | Use |
| gpui-pre 0.3.6 (snapshot of zed@bcf6582) | Apache-2.0 | `R gpui-pre-0.3.6/Cargo.toml:24`, `:33`, `:48-50` | Use |
| imara-diff 0.2.0 | Apache-2.0 | https://crates.io/crates/imara-diff | Use |
| tree-sitter, tree-sitter-highlight, grammar crates + their bundled queries | MIT | crates.io pages, upstream repos | Use |
| aho-corasick 1.1, fancy-regex 0.19 (Zed search) | Unlicense OR MIT / MIT | `Z Cargo.toml:526`, `:631`; crates.io | Use if needed |
| pulldown-cmark 0.13 (Zed markdown parser) | MIT | `Z Cargo.toml:797`; crates.io | Use if own renderer |
| markdown 1.0.0 (markdown-rs, TextView parser) | MIT | `R markdown-1.0.0/Cargo.toml:45` | Use (transitive) |
| html5ever, markup5ever_rcdom, linkify | MIT OR Apache-2.0 | `Z Cargo.toml:646`, `:692`, `:686`; crates.io | Use if needed |
| merman =0.8.0-alpha.5 (Zed mermaid) | MIT OR Apache-2.0 (alpha) | `Z crates/mermaid_render/Cargo.toml:19-22`; https://crates.io/crates/merman | Later, via a `MarkdownPlugin` |
| similar 3.2.0 (alt diff lib) | Apache-2.0 | https://crates.io/crates/similar | Not needed |

## 7. Porting plan

Minimal, in order. Each phase ships on its own.

### Phase 1: intra-line word diff

- **Crates:** `imara-diff = "0.2"` in `crates/git/Cargo.toml`.
- **Model:** in `git.rs`, extend `Line` with the changed byte ranges:

  ```rust
  pub struct Line { pub kind: Kind, pub old: Option<usize>, pub new: Option<usize>, pub text: String, pub words: Vec<Range<usize>> }
  ```

- **Computation:**
  - After `parse`, walk each run of `Del` lines followed by a run of `Add` lines. `split()` already does this pairing (L258-276).
  - If the counts are equal, ≤ 5 lines, and ≤ 512 bytes per side, join each side and tokenize (word / whitespace / single punctuation char).
  - Run `Diff::compute(Algorithm::Histogram, &InternedInput)`, then map the token ranges back to per-line byte ranges.
- **Render:**
  - In `diff.rs` `code()` (L131-144), switch from a plain string to `StyledText::new(text).with_highlights(words → HighlightStyle { background_color: stronger DIFF_ADD/DEL, .. })`.
  - Add `DIFF_ADD_WORD`/`DIFF_DEL_WORD` to `crates/theme/src/theme.rs` (next to L47-50). This is a later code change, not part of this doc.

### Phase 2: real syntax highlighting (diff + explore)

- **Crates:** enable gpui-kit features `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-tsx`, `tree-sitter-javascript`, `tree-sitter-python`, `tree-sitter-go`, `tree-sitter-toml`, `tree-sitter-yaml`, `tree-sitter-markdown`, `tree-sitter-bash`, `tree-sitter-css`, `tree-sitter-html` (list: `R gpui-kit-0.6.6/Cargo.toml`; JSON is included with base `tree-sitter`). Nothing else to add.
- **Resolver:** a `HighlightStyleResolver` in `crates/theme` or `pocket`. It maps `keyword*`, `function*`, `string*`, `comment*`, `number`/`constant*`, `type*` to `SYN_*` and falls back by dotted prefix like Zed.
- **In `refresh_git`** (background, `main.rs:475-532`):
  1. Read the new text (worktree) and the old text (`git show HEAD:path`).
  2. Run `SyntaxHighlighter::new(lang).update(None, &Rope::from(text), Some(50ms))`, then `styles(0..len, &resolver)`.
  3. Bucket the spans per line.
- **Render:** each diff `Line` looks up spans by `old`/`new` line number (deleted lines use the old text, like Zed's base buffer). Merge with the word ranges via `gpui::combine_highlights`.
- **explore.rs:** delete the heuristic `highlight()` (L67-108) and map `language()` (L38-57) to `Language::from_name`.
- **Guards:** skip highlighting for files over ~1 MB or when the parse times out; plain text is fine.

### Phase 3: virtualized file preview (`explore.rs`)

Today `code_box` (`P crates/pocket/src/explore.rs:313-343`) re-splits and re-highlights up to 3000 lines every render, `file_view` recounts lines every render (`:228-311`), and `refresh_git` reloads the file on every tick (`P crates/pocket/src/main.rs:475-532`).

- **Option A, spike first:** gpui-kit `Editor::new(&state).readonly(true)` (§4.9). Gets virtualization, tree-sitter, Cmd+F, selection/copy for free. Costs: gpui-component look, 1.5× line height, colors via `highlight_theme` (map `SYN_*` in `theme::init`), git marks only as `TextDecoration` tints (no bar), `set_value` on the UI thread. Take it if the look is acceptable after half a day.
- **Option B, own element (below):** our primitives, ~300 lines, pixel-matches the current design.

**Data model**

```rust
enum Preview { Text(TextFile), Image(PathBuf), Binary(u64), TooLarge(u64), Error(SharedString) }
struct TextFile {
    key: (PathBuf, SystemTime, u64),                         // skip reload when unchanged
    text: SharedString,
    lines: Vec<Range<usize>>,                                // tabs expanded, 1024-char cap
    spans: Vec<Vec<(Range<usize>, HighlightStyle)>>,         // line-relative; empty until parsed
    marks: HashMap<usize, bool>,                             // existing `gutter()` output
    widest: usize,                                           // index of longest line
}
```

Replaces `file_text: Option<String>` (`main.rs:131`) and `load()`'s tuple (`explore.rs:21-27`).

**Load (background executor)**

1. `metadata` → compare `key`; unchanged → keep current `Preview`, no rehighlight.
2. Extension in `Img::extensions()` → `Image(path)` (svg included; `img()` rasterizes it).
3. Size > cap (e.g. 8 MB) → `TooLarge`. First 1024 bytes contain NUL → `Binary` (Zed §4.7).
4. `from_utf8_lossy`, build `lines` (expand tabs to 4-col stops, cap column 256, truncate at 1024 chars), `widest`, `marks`. Push as `Preview::Text` with empty `spans`, `cx.notify()`: plain text shows immediately.
5. Second task, skipped above ~2 MB: `SyntaxHighlighter::new(lang).update(None, &Rope::from(text), None)` then `styles(0..len, &resolver)` (§3), bucket per line, skip lines > 10 000 bytes (gpui-kit's limit). Drop the result if `key` changed; else store `spans`, `cx.notify()`. Same plain-then-`Reparsed` flow as Zed §4.3.

**Rows**

- `uniform_list("code", lines.len(), cx.processor(|this, range, ..| rows))` + `.track_scroll(&handle)` + `.with_width_from_item(Some(widest))` + `.with_horizontal_sizing_behavior(Unconstrained)` for horizontal scroll. Delete `MAX_LINES`.
- Row = today's markup (`explore.rs:313-343`): 22 px `div`, 3 px absolute bar (`WAITING` modified / `RUNNING` added), number column, `StyledText::new(line).with_highlights(combine_highlights(syntax, overlays))` where overlays = search hits + selection as `background_color`. Container keeps `bg(SURFACE_SUNKEN)`, `font_family(MONO)`, `text_size(px(13.))`.
- Shaping is cached across frames by gpui (§4.1); no own cache needed.

**Gutter**

- Number column width = `max(digits(lines.len()), 4) × digit advance + padding` (Zed §4.4), replacing the fixed `w(px(46.))`.
- Active row (clicked line / current match): `bg(FILL_1)` on the row, number in `TEXT_2` instead of `TEXT_5`.
- Git marks: keep the 3 px bar. No folds.

**Scrolling**

- `uniform_list` handles visible-range math. Scrollbar: gpui-kit `ScrollbarHandle` for `UniformListScrollHandle` (§4.9).
- Search / goto-line: `handle.scroll_to_item(ix, ScrollStrategy::Center)` (Zed's default autoscroll).

**Selection / copy**

- Step 1, line-granular: mouse down/drag/shift-click on rows set `sel: Range<usize>` of lines; Cmd+A all; Cmd+C writes those lines. Empty selection copies the active line (Zed `do_copy`).
- Step 2, char-level: keep each visible row's `TextLayout` from `StyledText::layout()`; hit-test with `row = (y - top) / 22 + first_visible`, `index_for_position` (Zed `point_for_position`); paint via the overlay spans above. Only worth a custom element if row-by-row overlays look wrong.

**Search (Cmd+F)**

- Bar with our `ui` input above the list. Matches = ASCII-case-insensitive `match_indices` per line, on the background executor for large files, stored as `Vec<(line, Range<usize>)>`.
- Paint all hits `ACCENT_BG`, active hit stronger. Enter / Shift+Enter cycle and scroll to center. Regex later (fancy-regex MIT / aho-corasick Unlicense-or-MIT, as Zed).

**Image / binary / too large / error**

- `Image`: `img(path)` with default `object_fit(Contain)` on a `gpui::checkerboard` background, `with_fallback` → error view. Zoom later with Zed's constants (0.1–20, step 1.1, fit never upscales).
- Others: existing `empty(..)` message + "Open in default app" button (`cx.open_with_system`, already used at `explore.rs:253`), like Zed's `InvalidItemView`.

**Tests:** extend `explore.rs:346-392` for tab expansion, truncation, NUL sniff, span bucketing.

### Phase 3b: markdown preview (`explore.rs`)

**Recommendation: gpui-kit `TextView`** (§4.10.5). It is Apache-2.0, already in the dependency tree, off-thread and virtualized, and has selection/copy. Build our own pulldown-cmark renderer only if restyling via `TextViewStyle` can't match the design.

- **Model:** add `Preview::Markdown { text: TextFile, view: Entity<TextViewState> }` to Phase 3's enum. `TextFile` is reused for the Source tab.
- **UI:** a `segmented` Preview / Source toggle in the file header (`P crates/ui/src/ui.rs:141`). Source = Phase 3 code view. Default to Preview for `.md`/`.markdown`.
- **Load:**
  - The Phase 3 background load yields the text. Then call `cx.new(|cx| TextViewState::markdown(&text, cx).selectable(true).scrollable(true))`; parsing moves to gpui-base's background task.
  - On refresh with a changed `key`, call `view.update(|s, cx| s.set_text(&text, cx))`. The old render stays until the new parse lands, as in Zed's `reset`.
- **Render:** `TextView::new(&view).scrollable(true).style(style).on_link_click(..).code_block_highlighter(..)` inside a `size_full()` container on `SURFACE`.
- **Theming** (`TextViewStyle`, in `crates/theme`):
  - `with_foreground(TEXT_BODY)`, `with_muted_foreground(TEXT_3)`, `with_link(ACCENT)`, `with_selection(ACCENT_BG)`, `with_code_background(SURFACE_SUNKEN)`, `with_border(HAIRLINE)`.
  - `with_code_block(..)` using `MONO` 13 px, `with_heading_base_font_size(px(14.))`, `with_paragraph_gap(..)`.
  - Install once with `TextViewDefaults::new().with_style(..).install(cx)` in `theme::init`, after `gpui_kit::init`, which installs its own defaults.
- **Code blocks:**
  - `with_code_block_highlighter` using Phase 2's `SyntaxHighlighter` + our `SYN_*` resolver, with a thread-local per-language cache like gpui-component's (`R gpui-component-0.6.6/src/text/mod.rs:76-125`). Cached per block after first paint.
  - Skip blocks > 64 KB (Zed's chunk limit, §4.3).
  - `code_block_actions` adds our `icon_button("copy")`, which writes `block.code()`.
- **Links:**
  - `http(s)` → `cx.open_url`.
  - Relative `.md` path → open that file in explore.
  - `#anchor` → ignore for now.
- **Images:**
  - Remote URLs and `data:` work as-is.
  - Relative paths: a `MarkdownPlugin` for `Node::Image` resolves against the file's directory, confined to the repo root, and renders `img(PathBuf)` with `object_fit(Contain)` and `max_w_full()`. Mirrors Zed's `resolve_preview_image`.
- **Selection/copy:** built in (cmd-c/cmd-a, `SelectionFormat::Plain`). Offer `Source` later as "Copy as Markdown", like Zed.
- **Guards:** above ~1 MB, fall back to the Source view (`measure_all` lays out every block). Binary/too-large handling comes from Phase 3.
- **Fallback, own renderer** (only if needed):
  - pulldown-cmark 0.13 with Zed's option set (§4.10.1), parsed in the background into `Vec<Block { range, kind, inlines: Vec<(Range, HighlightStyle)> }>`.
  - Render as a `list` of blocks from `crates/ui` primitives: `page_title` for h1–h3, `StyledText::with_highlights` for paragraphs, `checkbox` (`P crates/ui/src/ui.rs:430`) for tasks, the Phase 3 code row for fenced code, `tag`-like chips for inline code.
  - Line-granular copy first. Hand-built cross-block selection is the expensive part.

### Phase 4 (optional): in-process diff + expandable context

- **Diff:** replace `git diff` parsing with old/new full texts (already loaded in phase 2), then imara-diff Histogram + `postprocess_lines`, with 3 context lines.
- **Context:** make the "N unchanged lines" header (`diff.rs:95-118`) clickable to reveal 5 more lines, like Zed's `expand_excerpt_lines`.
- **Numstat:** keep `git diff --numstat` for the file list, with `--histogram` so the counts match.

### Skip

- Zed's `rope`, `text`, anchors, `SumTree` hunk storage.
- `MultiBuffer` / `DiffTransform` / excerpts.
- `display_map` / `block_map` spacers and `SplittableEditor`. Our `git::split` already aligns rows.
- Staged vs unstaged secondary status and hollow hunk styling.
- Incremental reparse and edits; LSP semantic tokens.
- Zed's GPL `highlights.scm` files and theme JSON.
- `diffy` and `streaming_diff`.
