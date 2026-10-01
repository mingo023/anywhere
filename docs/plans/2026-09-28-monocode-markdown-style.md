# Monocode-style markdown preview — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Explore's markdown preview adopts monocode's look: compact headings, 10px-radius code and table cards, a lowercase language label and copy button on every code block, and a matching mermaid card. Text stays selectable everywhere.

**Architecture:** Everything goes through gpui-kit's stock `TextView`, with no new markdown plugin. The preview's `TextViewStyle` moves into a `markdown_style()` helper in `explore.rs`. `code_block_actions` renders the label and gpui-kit's `Clipboard` button in each code block's top-right corner. Setting the global `Theme.muted` to `WINDOW` makes the code card and the actions backdrop the same opaque colour, so the backdrop is invisible.

**Toolset** (run everything from `packages/desktop`):
- Setup, once per shell: `export PATH=$HOME/.cargo/bin:$PATH && cd packages/desktop`
- This plan's test: `cargo test -p pocket headings`
- Suites this plan touches: `cargo test -p theme -p pocket` (expected: pocket 36 passed after Task 1.1, theme 1 passed)
- Lint: `cargo clippy -p theme -p pocket`. The baseline has one warning, `double_ended_iterator_last` in `crates/git/src/git.rs`. Ignore it and add no new ones.
- Run the app: `cargo run --release -p pocket`

**Read first:**
- `docs/research-monocode-markdown-styles.md` §10 (token table) and §11 (gpui-kit mapping): the source of every value below.
- `packages/desktop/crates/pocket/src/explore.rs` ~L319-340: the markdown preview this plan restyles.
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-base-0.6.6/src/text/node.rs` L1553-1618 (`CodeBlock::render`: `p_3`, then the `code_block` refinement, then the actions div at `absolute top_2 right_2` with `bg(code_background)`), and L2680-2740 (table frame, head and cells).
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-component-0.6.6/src/text/mod.rs` L34-62: `code_background` is `Theme.muted`, and the style is resolved from the live theme on every render (`compat.rs` L237).
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-component-0.6.6/src/clipboard.rs`: the copy button. It shows a check for 2 s after copying, and its icons resolve to our `icons/copy.svg` / `icons/check.svg`.

**Never touch the git index.** Do not `git add`, `git stash`, `git checkout --`, `git restore` or commit. Edit files in place only.

**Code rules (from the user's CLAUDE.md):**
- Add no comments.
- Test modules import names explicitly: `use super::*` also pulls in `gpui_kit::*`, whose `test` macro shadows `#[test]`.
- Don't run `cargo fmt`; the files use long lines on purpose.

**Decisions (settled with the user):**
- Keep the stock code block so code stays drag-selectable. There are no line numbers and no left header; the label and copy button sit top-right.
- Headings are 22/18/16/14/14/14. Monocode's h3 (20px) is larger than its h2; we don't copy that.
- Keep Geist and our `SYN_*` syntax colours.
- Preview padding is 40 horizontal (aligned with the page title) and 32 vertical.
- Light theme only.

**Out of scope** (these need a plugin or a gpui-kit patch; see research §11.4):
- link underline/hover
- 78% body vs 100% bold
- heading weights and margins
- list spacing
- blockquote italic and 4px bar
- 1px hr
- table column rules
- inline-code pill
- frontmatter card
- fence paths in the header
- dark mode

**Compile-checked:** the code in Tasks 1.1–1.3 was applied to `HEAD` 5453496. Clippy added no warnings and `cargo test -p theme -p pocket` passed (pocket 36, theme 1). It was then reverted.

---

## PR 1: Monocode-style markdown preview

**Scope:** Restyle Explore's markdown preview: headings, code, table and mermaid cards, inline code, padding, plus a code-block label and copy button. No markdown plugins; gpui-kit is untouched.
**Depends on:** nothing
**Done when:** `cargo test -p theme -p pocket` is green, clippy adds no warnings, and the Task 1.4 manual check passes.

### Task 1.1: Preview style — headings, cards, tables, inline code, padding

**Files:**
- Modify: `packages/desktop/crates/theme/src/theme.rs:173` (`init`)
- Modify: `packages/desktop/crates/pocket/src/explore.rs:1-14` (imports), `:123-125` (after `pane()`), `:322-340` (preview), `:411-414` (test imports), `:442` (new test)

**Context:**
- The preview is `TextView::new(&self.md)` inside `pane()` in `Desktop::file_view`. Its `TextViewStyle` is currently built inline.
- gpui-kit folds our `TextViewStyle` onto a themed base, and each `StyleRefinement` refines the themed value:
  - A code block starts as `p_3().bg(Theme.muted)`, then gets our `code_block` refinement.
  - A table frame starts as `bg(surface).border_1().border_color(Theme.border)`, then gets `table`.
  - The table's first row starts as `bg(Theme.muted).text_color(foreground)`, then gets `table_head`. Without our override, the default theme makes the head text grey `#737373`.
- `heading_font_size` replaces gpui-kit's rem table (h1 28, h2 21, …). gpui-kit keeps h1 at weight 700 and h2–h5 at 600; that can't be changed here.
- The code block gets 36px of top padding. That space becomes the header band that Task 1.2's actions sit in.
- `Theme.muted` is also gpui-kit's actions backdrop. Only text views and gpui-kit's input group (which we don't use) read it.

**Step 1: Write the failing test**

In `explore.rs`, change the test imports at `:412-414` from:

```rust
    use super::{Preview, decode, decorations, gutter, language, preview, size};
    use git::{Kind, Line};
    use gpui_kit::rgba;
```

to:

```rust
    use super::{Preview, decode, decorations, gutter, heading_size, language, preview, size};
    use git::{Kind, Line};
    use gpui_kit::{px, rgba};
```

Add this test directly above `fn names_languages_and_sizes()` (`:442`). It pins the heading scale, including the deliberate h3 < h2 departure from monocode:

```rust
    #[test]
    fn headings_shrink_with_depth() {
        assert_eq!((1..=6).map(heading_size).collect::<Vec<_>>(), [22., 18., 16., 14., 14., 14.].map(px));
    }

```

**Step 2: Run the test to verify it fails**

Run: `cargo test -p pocket headings`
Expected: FAIL to compile with ``error[E0432]: unresolved import `super::heading_size` ``.

**Step 3: Write the implementation**

1. `theme.rs`, in `init`, add one line after `t.background = rgba(WINDOW).into();` (`:173`):

```rust
    t.muted = rgba(WINDOW).into();
```

2. `explore.rs` imports: replace lines 6-7:

```rust
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::{Editor, TextDecoration};
```

with:

```rust
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::input::{Editor, TextDecoration};
```

Then replace line 12:

```rust
use std::path::{Path, PathBuf};
```

with:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
```

3. `explore.rs`: directly after `fn pane()` (ends `:125`), add:

```rust

fn heading_size(level: u8) -> Pixels {
    px(match level {
        1 => 22.,
        2 => 18.,
        3 => 16.,
        _ => 14.,
    })
}

fn markdown_style(highlight_theme: Arc<HighlightTheme>) -> TextViewStyle {
    TextViewStyle {
        highlight_theme,
        heading_font_size: Some(Arc::new(|level, _| heading_size(level))),
        inline_code: HighlightStyle { background_color: Some(rgba(HAIRLINE).into()), ..Default::default() },
        code_block: StyleRefinement::default().border_1().border_color(rgba(SEPARATOR)).rounded(px(10.)).pt(px(36.)).px(px(12.)).pb(px(10.)).text_size(px(12.)),
        table: StyleRefinement::default().bg(rgba(WINDOW)).border_color(rgba(SEPARATOR)).rounded(px(10.)),
        table_head: StyleRefinement::default().bg(transparent_black()).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD),
        table_cell: StyleRefinement::default().px(px(10.)).py(px(8.)).text_size(px(12.)),
        ..Default::default()
    }
}
```

4. `explore.rs` preview (`:322-340`): replace

```rust
                .px(px(40.))
                .py(px(20.))
                .bg(rgba(SURFACE))
                .child(
                    TextView::new(&self.md)
                        .plugin(Mermaid(self.diagrams.clone()))
                        .selectable(true)
                        .scrollable(true)
                        .style(TextViewStyle {
                            highlight_theme: cx.theme().highlight_theme.clone(),
                            inline_code: HighlightStyle { background_color: Some(rgba(FILL_2).into()), ..Default::default() },
                            code_block: StyleRefinement::default().bg(rgba(SURFACE_SUNKEN)).rounded(px(8.)).p(px(12.)),
                            ..Default::default()
                        })
                        .size_full(),
```

with:

```rust
                .px(px(40.))
                .py(px(32.))
                .bg(rgba(SURFACE))
                .child(
                    TextView::new(&self.md)
                        .plugin(Mermaid(self.diagrams.clone()))
                        .selectable(true)
                        .scrollable(true)
                        .style(markdown_style(cx.theme().highlight_theme.clone()))
                        .size_full(),
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket headings`
Expected: PASS (`test explore::tests::headings_shrink_with_depth ... ok`, 1 passed).

Run: `cargo test -p theme -p pocket` and `cargo clippy -p theme -p pocket`
Expected: all pass (pocket 36, theme 1). Clippy shows only the baseline `git.rs` warning.

### Task 1.2: Code-block language label and copy button

**Files:**
- Modify: `packages/desktop/crates/pocket/src/explore.rs` (imports; new `code_actions` after `markdown_style`; one builder call on the preview `TextView`)

**Context:**
- `TextView::code_block_actions(f)` calls `f(&CodeBlock, &mut Window, &mut App)` for every fenced block.
- gpui-kit places the result in `div().id("actions").absolute().top_2().right_2().bg(Theme.muted).rounded(md)`. The block's element id scopes the `"copy"` id, so ids don't collide across blocks.
- Task 1.1 set `Theme.muted` = `WINDOW`, the same colour as the code card, so the backdrop is invisible.
- The block's 36px top padding (Task 1.1) holds the 20px `Clipboard` button (XSmall) at top 8, so code never runs under it.
- `CodeBlock::lang()` is the fence's first info word, or `None`. Monocode shows it lowercase in 12px mono at weight 500, at 65% (our `TEXT_2`).
- `Clipboard` (gpui-kit) writes the value to the clipboard and swaps its icon to a check for 2 s.
- Mermaid fences are claimed by the `Mermaid` plugin before they become code blocks, so they get no actions.
- No unit test: this is pure layout inside a gpui render closure. Task 1.4 checks it in the app.

**Step 1: Write the implementation**

1. `explore.rs` imports: replace

```rust
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::highlighter::HighlightTheme;
```

with:

```rust
use gpui_kit::base::text::CodeBlock;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::highlighter::HighlightTheme;
```

2. `explore.rs`: directly after `fn markdown_style` (added in Task 1.1), add:

```rust

fn code_actions(block: &CodeBlock) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .children(block.lang().map(|lang| div().font_family(MONO).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).child(lang.to_lowercase())))
        .child(Clipboard::new("copy").value(block.code()))
}
```

3. `explore.rs` preview: replace

```rust
                        .plugin(Mermaid(self.diagrams.clone()))
                        .selectable(true)
```

with:

```rust
                        .plugin(Mermaid(self.diagrams.clone()))
                        .code_block_actions(|block, _, _| code_actions(block))
                        .selectable(true)
```

**Step 2: Verify it builds and nothing regressed**

Run: `cargo clippy -p theme -p pocket`
Expected: only the baseline `git.rs` warning.

Run: `cargo test -p theme -p pocket`
Expected: all pass (pocket 36, theme 1).

### Task 1.3: Mermaid card matches the code card

**Files:**
- Modify: `packages/desktop/crates/pocket/src/mermaid.rs:24` (theme role), `:213` (frame)

**Context:**
- In monocode, mermaid diagrams sit in the same card as code: radius 10, 1px 10% border, 6% fill, 12px padding.
- Our frame is currently radius 8, `HAIRLINE`, `SURFACE` (white). It becomes radius 10, `SEPARATOR`, `WINDOW`.
- The diagram's canvas role moves from `SURFACE` to `WINDOW` too. Otherwise merman paints white label and canvas backgrounds on the grey card.
- `hex()` flattens colours onto white; `WINDOW` is opaque, so it is unaffected.
- The mermaid tests check SVG structure and sizes, not colours, so they stay green.

**Step 1: Write the implementation**

`mermaid.rs:24`: replace

```rust
        (ThemeRole::Canvas, SURFACE),
```

with:

```rust
        (ThemeRole::Canvas, WINDOW),
```

`mermaid.rs:213`: replace

```rust
        let frame = div().id(SharedString::from(format!("mermaid-{key}"))).my(px(8.)).p(px(12.)).rounded(px(8.)).border_1().border_color(rgba(HAIRLINE)).bg(rgba(SURFACE));
```

with:

```rust
        let frame = div().id(SharedString::from(format!("mermaid-{key}"))).my(px(8.)).p(px(12.)).rounded(px(10.)).border_1().border_color(rgba(SEPARATOR)).bg(rgba(WINDOW));
```

**Step 2: Verify**

Run: `cargo test -p pocket mermaid`
Expected: 6 mermaid tests pass.

Run: `cargo test -p theme -p pocket` and `cargo clippy -p theme -p pocket`
Expected: all pass. Clippy shows only the baseline warning.

### Task 1.4: Manual look check

**Files:** none

**Context:**
- The research doc exercises headings, inline code, tables, a labelled ` ```rust ` fence, and an unlabelled fence (§6 DOM block).
- `docs/plans/2026-09-28-mermaid-preview.md` has a mermaid flowchart.

**Step 1: Run the app**

Run: `cargo run --release -p pocket`.

Open the anywhere project, go to Explore, and open `docs/research-monocode-markdown-styles.md`.

**Step 2: Check each item** (all must hold):

1. **Headings:** h1 is clearly larger than h2, and every level is smaller than or equal to the one above it.
2. **Code blocks:**
   - Grey `#f4f4f5` card, 10px radius, faint 1px border, 12px mono code.
   - `rust` appears lowercase at the top-right, next to a copy icon.
   - The unlabelled block shows only the copy icon.
   - No differently-shaded rectangle sits behind the label or icon.
   - Code starts below them and never overlaps them.
3. **Copy:** clicking the icon turns it into a check for about 2 s. Pasting elsewhere gives the block's code.
4. **Selection:** you can drag-select part of a code line and ⌘C it.
5. **Inline code:** spans such as `TextViewStyle` in the prose have a faint grey highlight.
6. **Tables** (§10):
   - Grey card, 10px radius.
   - Header row has no fill, is semibold and near-black.
   - Cells are 12px with roomy padding.
7. **Padding:** body text lines up horizontally with the page title above, with about 32px of space above the first line.
8. **Mermaid:** open `docs/plans/2026-09-28-mermaid-preview.md`. The flowchart sits in the same grey card, with no white box behind the diagram or its labels.

If item 2's code overlaps the actions, raise `pt(px(36.))` in `markdown_style` to `pt(px(40.))`. If item 8 shows a white box, report it rather than changing other theme roles. Report any other failure without improvising.
