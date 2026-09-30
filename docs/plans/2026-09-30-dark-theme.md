# Dark Theme Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** The desktop app follows the macOS appearance. Light looks exactly as today and stays opaque. Dark uses monocode's palette over a blurred, translucent window.

**Architecture:**
- Colour tokens change from `const u32` to `const Token { light, dark }`. A process-wide `AtomicBool` picks the value when a token is turned into `Hsla`/`Fill`, so call sites pass tokens straight to GPUI with no `rgba(..)` wrapper.
- `theme::set_appearance` flips the flag and re-points gpui-kit's theme. A window-appearance observer in `Desktop` calls it, switches the window to `Blurred`, and recolours cached diff spans.
- Dark fills are ink-over-transparent (monocode §1), so they stay right over the 0.85-alpha window background.

**Toolset** (run everything from `packages/desktop`):
- Build: `cargo build --workspace`
- Lint: `cargo clippy --workspace --all-targets`. It must add no warnings; compare against the count on `main` before starting.
- All tests: `cargo test --workspace`
- One crate's tests: `cargo test -p theme`, `cargo test -p pocket <filter>`
- Screens: from the repo root, `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …`. It builds release with `--features capture` and writes `<dir>/impl-<name>.png`.
  - The light baseline, taken before any change, is in `/tmp/dark-before`. It was captured with `home= session=session explore=explore changes=changes,comment inbox=inbox palette=palette new=new-session add=add-repo rail=rail`.
- Pixel compare: `python3 -c "from PIL import Image, ImageChops as C; import sys; a,b=(Image.open(p).convert('RGBA') for p in sys.argv[1:3]); print(C.difference(a,b).getbbox())" A.png B.png` prints `None` when the images are identical.

**Read first:**
- `CLAUDE.md` (repo root): layout rules, test style, and the pre-commit commands.
- `docs/research-monocode-dark-theme.md`: where the palette and the translucency model come from.
- `packages/desktop/crates/theme/src/theme.rs`: every token lives here.
- `packages/desktop/crates/ui/src/ui.rs`: the shared helpers. Most colour signatures change here.

**Rules for every task:**
- No comments that restate code. Keep WHY comments only.
- Match the surrounding style.
- Do not commit.
- Tests are named as sentences and sit in `#[cfg(test)] mod tests` at the bottom of the file.
- Never flip the global dark flag in a test, because tests run in parallel. Test a scheme with `Token::pick(dark)` or with functions that take `dark: bool`.

---

## PR 1: Runtime tokens, light unchanged

**Scope:**
- Introduce `Token` and migrate every crate to it.
- Every token is still `Token::fixed(<today's value>)`, so nothing can look different.
- No dark values yet.

**Depends on:** nothing.

**Done when:**
- Build, clippy and tests are green.
- The light captures are pixel-identical to `/tmp/dark-before`.

### Task 1.1: `Token` type

**Files:**
- Modify: `packages/desktop/crates/theme/src/theme.rs` (top of file, and the tests module)

**Context:** `rgba(u32)` is GPUI's constructor for `Rgba`; `Hsla: From<Rgba>`. Styling methods take `impl Into<Hsla>` (`text_color`, `border_color`) or `impl Into<Fill>` (`bg`). With `From<Token>` implemented for both, a token can be passed as-is.

**Step 1: Write the failing tests.** Append these inside `mod tests` in `theme.rs`, and add `Token` to its `use super::{…}` list:

```rust
    #[test]
    fn a_token_picks_its_light_or_dark_value() {
        let t = Token::new(0x111111ff, 0xeeeeeeff);
        assert_eq!((t.pick(false), t.pick(true)), (0x111111ff, 0xeeeeeeff));
        assert_eq!(Token::fixed(0x5b5bd6ff).pick(true), 0x5b5bd6ff);
    }

    #[test]
    fn a_token_paints_light_until_told_otherwise() {
        let t = Token::new(0x111111ff, 0xeeeeeeff);
        assert_eq!(Hsla::from(t), Hsla::from(rgba(0x111111ff)));
    }
```

Add `use gpui_kit::{Hsla, rgba};` in the tests module, replacing the existing `use gpui_kit::rgba;`.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p theme`
Expected: a compile error, `cannot find type Token`.

**Step 3: Write the implementation.** Insert this after the `MONO` const. Extend the `std::sync` import to `use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};`.

```rust
static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// A colour with a value per appearance, resolved when painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    light: u32,
    dark: u32,
}

impl Token {
    pub const fn new(light: u32, dark: u32) -> Self {
        Self { light, dark }
    }

    pub const fn fixed(c: u32) -> Self {
        Self::new(c, c)
    }

    pub fn pick(self, dark: bool) -> u32 {
        if dark { self.dark } else { self.light }
    }
}

impl From<Token> for Hsla {
    fn from(t: Token) -> Self {
        rgba(t.pick(is_dark())).into()
    }
}

impl From<Token> for Fill {
    fn from(t: Token) -> Self {
        rgba(t.pick(is_dark())).into()
    }
}
```

`DARK` is written in Task 3.1. Until then it is always false.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p theme`
Expected: PASS. Everything else is untouched, so `cargo build --workspace` still passes.

### Task 1.2: Migrate every crate to `Token`

**Files:**
- Modify: `packages/desktop/crates/theme/src/theme.rs` (tokens, `syntax_color`, `highlight_theme`, `provider_color`, `icon`, `spinner`, `init`, tests)
- Modify: `packages/desktop/crates/ui/src/ui.rs`
- Modify: every `.rs` under `packages/desktop/crates/pocket/src` and `packages/desktop/crates/storybook/src` that uses a colour token. That is about 250 `rgba(TOKEN)` sites.

**Context:**
- This is a mechanical, type-driven refactor, so it changes no behaviour. Every token becomes `Token::fixed(<same value>)`, and the compiler finds every site.
- These colours are **data, not tokens**, and stay `u32`: `PALETTE`, `repo_color`, `RepoDraft.color`, `ui::swatch(color: u32, ..)`, `ui::backdrop`'s computed colour, and `mermaid::hex`.

**Step 1: Tokens.** In `theme.rs`, rewrite every colour const from `pub const X: u32 = V;` to `pub const X: Token = Token::fixed(V);`. This covers everything from `TEXT` through `SYN_COMMENT`; the values are unchanged. Leave `SANS`, `MONO`, `PALETTE` and `FONTS` alone.

**Step 2: Theme functions.**

```rust
fn syntax_color(name: &str) -> Token { /* body unchanged */ }

pub fn highlight_theme() -> Arc<HighlightTheme> {
    let hsla = |c: Token| serde_json::to_value(Hsla::from(c)).expect("colour serializes");
    // rest unchanged
}

pub fn provider_color(provider: &str) -> Token { /* body unchanged */ }

pub fn icon(name: &str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

pub fn spinner(id: impl Into<ElementId>, size: f32, color: impl Into<Hsla>) -> impl IntoElement { /* body unchanged */ }
```

In `init`, change `rgba(X).into()` to `X.into()` for every field. In the test `colours_syntax_with_our_tokens`, change `Some(rgba(SYN_X).into())` to `Some(SYN_X.into())`.

**Step 3: Strip the wrappers.** From `packages/desktop`:

```sh
find crates -name '*.rs' -exec perl -pi -e 's/\brgba\(([A-Z][A-Z0-9_]*)\)/$1/g' {} +
```

**Step 4: Helper signatures in `ui.rs`.**
- `shadow`, `ring` and `highlight` take `color: impl Into<Hsla>`, and `shadow`'s body uses `color: color.into()`.
- `dot(size, color: Token)`, `glyph_button(.., color: Token)`, `icon_button_sized(.., color: Token)`.
- `menu_item(.., tint: Token, hover: Token)`.
- `Variant::fg(self) -> Token`, `alert_color(..) -> Option<Token>`, `git_color(..) -> Token`.
- The closures `pill = |bg: Token, fg: Token|`, `badge = |d: Div, color: Token|` and `label = |color: Token|`.
- Literal shadow colours become `rgba(0x…)`, e.g. `shadow(rgba(0x1111130f), 1., 2.)`. This applies in `ui.rs` and in pocket: `panel.rs`, `new_session.rs`, `composer.rs`, `comment.rs` and any others the compiler reports.

**Step 5: Fix what the compiler reports.** Run `cargo build --workspace` and apply these rules until it is clean:
- `rgba(<expr producing a Token>)` loses the `rgba`:
  - `rgba(if a { X } else { Y })` → `if a { X } else { Y }`. Sites: `ui.rs` `worktree_row` and `context_bar`, `explorer/preview/code.rs:27`, `modals/new_session/picker.rs:26`, `terminal_view/pane.rs:107,149`, `git_ui/diff/row.rs:25`.
  - `rgba(v.fg())`, `rgba(git_color(g))` (also `git_ui/changes/rows.rs:194`) and `rgba(provider_color(p))` (`sidebar/rail.rs:88`) lose it too.
  - So do the local variables `rgba(color)`, `rgba(bg)`, `rgba(fg)` and `rgba(hover)`, whose type becomes `Token`.
- Functions and tuples that return token colours return `Token`:
  - `explorer.rs` `status_word -> (Token, &'static str)`
  - `git_ui/diff/row.rs` `colors -> (Option<Token>, Token, &'static str)`
  - `git_ui/diff/target.rs` `session_chip -> (Token, String, String)`
  - `git_ui/diff.rs` `tint(.., color: Token)` with `background_color: Some(color.into())`
  - `sidebar/rail.rs` `badge = |d: Div, color: Token|`
  - `modals/add_project.rs:262`'s `(mark, line, color)`, whose `color` is a token
- `HighlightStyle { color: Some(rgba(X).into()) }` → `Some(X.into())`.
- The `terminal_view/tabs.rs:146` fade becomes:
  ```rust
  let solid: Hsla = SURFACE_SUNKEN.into();
  let (solid, clear) = (solid, solid.opacity(0.));
  ```
  This is the same colour with alpha 0, as `& 0xffffff00` gave before.
- `terminal_view/surface.rs:43`: `let paper: Hsla = WHITE.into();`, and the same for `ink`.
- `u32` data flowing into a helper that now takes `impl Into<Hsla>` gets wrapped, e.g. `ui::ring(rgba(c), 1.5)` in `add_project.rs:337`.
- Tests:
  - `syntax.rs` test closure `c: Token` → `s.color == Some(c.into())`.
  - `git_ui/diff.rs` test helper `fn color(c: Token)` → `Some(c.into())`.
- `explorer/mermaid.rs`:
  - The `roles` array holds tokens; map each with `hex(c.pick(false))`.
  - `svg`'s `root_background_color: Some(hex(WINDOW.pick(false)))`.
  - The test `paints_the_canvas_in_the_window_colour` uses `WINDOW.pick(false).to_be_bytes()`.
  - Task 2.4 makes these per scheme.
- Remove `use gpui_kit::rgba;` or `rgba` imports that became unused.

**Step 6: Verify**

Run each command, from `packages/desktop`:
- `cargo build --workspace`
  Expected: success.
- `cargo clippy --workspace --all-targets`
  Expected: no warnings beyond main's.
- `cargo test --workspace`
  Expected: all pass.
- `grep -rn --include='*.rs' -E 'rgba\([A-Z][A-Z0-9_]*\)' crates`
  Expected: no output.

### Task 1.3: Prove light is unchanged

**Files:** none.

**Step 1: Capture.** From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/dark-pr1 home= session=session explore=explore changes=changes,comment inbox=inbox palette=palette new=new-session add=add-repo rail=rail
```

**Step 2: Compare** each `/tmp/dark-pr1/impl-*.png` with `/tmp/dark-before/impl-*.png` using the Toolset pixel-compare command.

- Expected: `None` for every pair.
- A non-`None` box means a colour changed. Find the site inside that box and fix it back to its old value.
- Timestamps ("2m ago") can differ between runs. If a box covers only such text, check it by eye and note it.

---

## PR 2: Dark palette

**Scope:**
- Give every token its dark value and split out the role tokens the shared helpers need.
- Mermaid renders per scheme.
- This is **inert until PR 3**: nothing sets the dark flag yet, so the app still renders light, and light values are unchanged.

**Depends on:** PR 1.

**Done when:**
- Build, clippy and tests are green.
- Light captures are still identical to `/tmp/dark-before`.

### Task 2.1: Dark values and role tokens

**Files:**
- Modify: `packages/desktop/crates/theme/src/theme.rs` (token block, tests)

**Context:**
- Dark is monocode's defaults: bg `#171717`, ink `#ebebeb`, and fills made of ink at a percentage over transparent.
- The window paints bg at alpha 0.85 (`0xd9`) over the blurred desktop, so dark panes and pages are transparent and show it.
- Status text colours are brightened for contrast on dark.
- Accent, the status dots and backgrounds, the agent colours and the diff backgrounds are the same in both schemes.

**Step 1: Write the failing tests** in `theme.rs` `mod tests` (add the new names to the `use super::{…}`):

```rust
    #[test]
    fn dark_is_glass_over_the_desktop_and_light_is_opaque() {
        assert_eq!(WINDOW.pick(false) & 0xff, 0xff);
        assert_eq!(WINDOW.pick(true), 0x171717d9);
        assert_eq!(WINDOW_SOLID.pick(true), 0x171717ff);
    }

    #[test]
    fn dark_fills_are_ink_over_transparent() {
        for t in [FILL_1, FILL_2, FILL_3, FILL_4, HAIRLINE, SEPARATOR, SEPARATOR_STRONG, SURFACE] {
            assert_eq!(t.pick(true) >> 8, 0xebebeb);
        }
    }
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p theme`
Expected: compile error, `cannot find value WINDOW_SOLID`.

**Step 3: Write the implementation.** Replace the token block in `theme.rs` with this. Every light value is unchanged:

```rust
pub const TEXT: Token = Token::new(0x111113ff, 0xebebebff);
pub const TEXT_BODY: Token = Token::new(0x3f3f46ff, 0xbcbcbcff);
pub const TEXT_2: Token = Token::new(0x6f6f78ff, 0xa1a1a1ff);
pub const TEXT_3: Token = Token::new(0x8b8b94ff, 0x818181ff);
pub const TEXT_4: Token = Token::new(0xa1a1aaff, 0x6c6c6cff);
pub const TEXT_5: Token = Token::new(0xb4b4bcff, 0x616161ff);
pub const TEXT_6: Token = Token::new(0xd4d4d8ff, 0x414141ff);
pub const WHITE: Token = Token::fixed(0xffffffff);
/// Text and glyphs on a `TEXT` fill.
pub const ON_TEXT: Token = Token::new(0xffffffff, 0x171717ff);

pub const WINDOW: Token = Token::new(0xf4f4f5ff, 0x171717d9);
/// The window colour where nothing shows through: floated panels and images.
pub const WINDOW_SOLID: Token = Token::new(0xf4f4f5ff, 0x171717ff);
pub const SURFACE: Token = Token::new(0xffffffff, 0xebebeb1a);
pub const SURFACE_SUNKEN: Token = Token::new(0xfafafaff, 0x00000000);
pub const PAGE: Token = Token::new(0xf7f7f7ff, 0x00000000);
/// A recessed block inside a page: tables, code blocks.
pub const WELL: Token = Token::new(0xf4f4f5ff, 0xebebeb0d);
pub const SIDE: Token = Token::new(0xfafafbb3, 0x00000000);
/// Opaque because GPUI has no backdrop blur.
pub const POPOVER: Token = Token::new(0xffffffff, 0x232323ff);
pub const GLASS: Token = Token::new(0xffffff9e, 0x2a2a2acc);
pub const HIGHLIGHT: Token = Token::new(0xfffffff2, 0xffffff0f);
/// The ring that cuts a badge out of whatever it overlaps.
pub const CUTOUT: Token = Token::new(0xffffffff, 0x171717ff);

pub const FILL_1: Token = Token::new(0x11111308, 0xebebeb0d);
pub const FILL_2: Token = Token::new(0x1111130b, 0xebebeb14);
pub const FILL_3: Token = Token::new(0x1111130e, 0xebebeb1a);
pub const FILL_4: Token = Token::new(0x11111311, 0xebebeb1f);
pub const HAIRLINE: Token = Token::new(0x11111312, 0xebebeb12);
pub const SEPARATOR: Token = Token::new(0x11111317, 0xebebeb14);
pub const SEPARATOR_STRONG: Token = Token::new(0x1111131f, 0xebebeb24);

pub const ACCENT: Token = Token::fixed(0x5b5bd6ff);
pub const ACCENT_BG: Token = Token::fixed(0x5b5bd61c);
pub const ACCENT_RING: Token = Token::fixed(0x5b5bd633);
pub const ACCENT_TINT: Token = Token::fixed(0x5b5bd612);
pub const ACCENT_GLOW: Token = Token::fixed(0x5b5bd666);

pub const WAITING: Token = Token::fixed(0xffb224ff);
pub const WAITING_TEXT: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const WAITING_BG: Token = Token::fixed(0xffb2242e);
pub const RUNNING: Token = Token::fixed(0x30a46cff);
pub const RUNNING_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const RUNNING_BG: Token = Token::fixed(0x30a46c21);
pub const FAILED: Token = Token::fixed(0xe5484dff);
pub const FAILED_BG: Token = Token::fixed(0xe5484d1c);

pub const AGENT_CLAUDE: Token = Token::fixed(0xd97757ff);
pub const AGENT_CODEX: Token = Token::fixed(0x0f9d8aff);

pub const DIFF_ADD_BG: Token = Token::fixed(0x30a46c1c);
pub const DIFF_ADD_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const DIFF_DEL_BG: Token = Token::fixed(0xe5484d17);
pub const DIFF_DEL_TEXT: Token = Token::new(0xcd2b31ff, 0xff9592ff);
pub const DIFF_ADD_WORD: Token = Token::fixed(0x30a46c40);
pub const DIFF_DEL_WORD: Token = Token::fixed(0xe5484d38);

pub const MODIFIED: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const TEAL: Token = Token::new(0x0e7c86ff, 0x0bd8b6ff);
pub const TEAL_BG: Token = Token::fixed(0x0f9d8a14);

pub const SYN_KEYWORD: Token = Token::new(0x8e4ec6ff, 0xd19dffff);
pub const SYN_FN: Token = Token::new(0x3e63ddff, 0x9eb1ffff);
pub const SYN_STRING: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const SYN_COMMENT: Token = Token::new(0xa1a1aaff, 0x818181ff);
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p theme && cargo build --workspace`
Expected: PASS. The new tokens are unused until 2.2, and pub consts don't warn.

### Task 2.2: Roles in the shared helpers

**Files:**
- Modify: `packages/desktop/crates/ui/src/ui.rs`

**Context:**
- `WHITE` on a `TEXT` fill must invert in dark, because `TEXT` becomes light ink there. Such sites use `ON_TEXT`.
- `WHITE` on a coloured fill (accent, danger, waiting) stays `WHITE`.
- Literal translucent whites become role tokens.

**Step 1: Make the edits.** Each has the same light value, so light is unchanged:
- `side`: `.bg(rgba(0xfafafbb3))` → `.bg(SIDE)`.
- `pop`: `.bg(SURFACE)` → `.bg(POPOVER)`, and `highlight(rgba(0xfffffff2))` → `highlight(HIGHLIGHT)`.
- `glass`: `.bg(rgba(0xffffff9e))` → `.bg(GLASS)`, and `highlight(rgba(0xfffffff2))` → `highlight(HIGHLIGHT)`.
- `Variant::fg`:
  ```rust
  match self {
      Variant::Primary => ON_TEXT,
      Variant::Accent | Variant::Danger => WHITE,
      Variant::Glass | Variant::Secondary => TEXT,
      Variant::Ghost => TEXT_2,
  }
  ```
- `repo_tile`:
  - the badge ring `ring(WHITE, 2.)` → `ring(CUTOUT, 2.)`;
  - the selected text `.text_color(WHITE)` → `.text_color(ON_TEXT)`.
- `checkbox`: `icon("check", 11., WHITE)` → `icon("check", 11., ON_TEXT)`.
- `avatar`: `.text_color(WHITE)` → `.text_color(ON_TEXT)`.
- `add_tile`: `.border_color(rgba(0x00000040))` → `.border_color(Token::new(0x00000040, 0xebebeb40))`.
- `field_box`: `.bg(rgba(0xffffffd9))` → `.bg(Token::new(0xffffffd9, 0xebebeb14))`.
- `trigger_field`: `.bg(rgba(0xffffffb3))` → `.bg(Token::new(0xffffffb3, 0xebebeb0d))`.
- These stay as they are: `count_badge`'s `WHITE` (it sits on `WAITING`), `primary`'s and `Accent`'s highlights, and all shadows and `backdrop`.

**Step 2: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: green, with no new warnings.

### Task 2.3: Roles in pocket

**Files:**
- Modify:
  - `packages/desktop/crates/pocket/src/sidebar/rail.rs`
  - `packages/desktop/crates/pocket/src/sidebar/panel.rs`
  - `packages/desktop/crates/pocket/src/sidebar/usage.rs`
  - `packages/desktop/crates/pocket/src/terminal_view/tabs.rs`
  - `packages/desktop/crates/pocket/src/terminal_view/surface.rs`
  - `packages/desktop/crates/pocket/src/modals/new_session.rs`
  - `packages/desktop/crates/pocket/src/git_ui/changes/commit_box.rs`
  - `packages/desktop/crates/pocket/src/explorer/preview/markdown.rs`

**Context:**
- The same rules as Task 2.2 apply.
- The commit button is `primary` (a `TEXT` fill), so its white overlays become dark overlays in dark.

**Step 1: Make the edits.** Line numbers are from before PR 1 and may have shifted slightly.
- `rail.rs`:
  - The `badge` ring `ui::ring(SURFACE_SUNKEN, 2.)` becomes `ui::ring(Token::new(0xfafafaff, 0x171717ff), 2.)`. This is the rail's colour in each scheme; in dark `SURFACE_SUNKEN` is transparent, which would erase the cutout.
  - The selected session bg `WHITE` becomes `SURFACE` (about line 52).
  - The compose icon `WHITE` becomes `ON_TEXT` (about line 85).
- `panel.rs`: `.bg(WINDOW)` → `.bg(WINDOW_SOLID)`, because the floated panel must hide the page under it. The dim `0x1111131a` literals stay.
- `usage.rs:34`: `.bg(rgba(0xffffff8c))` → `.bg(Token::new(0xffffff8c, 0xebebeb0d))`.
- `tabs.rs:99`: the selected tab `.bg(WHITE)` → `.bg(SURFACE)`.
- `surface.rs`: `paper` `WHITE` → `ON_TEXT`, and update the `use theme::{…}` list.
- `new_session.rs`:
  - The send-arrow icon `WHITE` → `ON_TEXT` (about line 346).
  - The composer `.bg(WHITE)` → `.bg(SURFACE)` (about line 360).
- `commit_box.rs`:
  - Icons and text `WHITE` → `ON_TEXT` (about lines 47, 67, 90).
  - `rgba(0xffffff1a)` (three sites) → `Token::new(0xffffff1a, 0x1717171a)`.
  - `rgba(0xffffff8c)` → `Token::new(0xffffff8c, 0x1717178c)`.
  - `rgba(0xffffff33)` → `Token::new(0xffffff33, 0x17171733)`.
  - Where a token repeats three times, bind it once in the function (`let wash = Token::new(..);`) if that reads cleaner.
- `markdown.rs`: the `table` style `.bg(WINDOW)` → `.bg(WELL)`.
- `theme.rs` `init`: `t.muted = WINDOW.into()` → `t.muted = WELL.into()`, and `t.background = WINDOW.into()` → `t.background = WINDOW_SOLID.into()`. Keep the existing comment above `muted`.

**Step 2: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: green.

Then run: `grep -rn --include='*.rs' 'WHITE' crates/pocket crates/ui`
Expected: only `ui.rs` `count_badge`, `Variant::fg`'s Accent/Danger arm, and `git_ui/diff/row.rs`'s plus icon on `ACCENT`.

### Task 2.4: Mermaid per scheme

**Files:**
- Modify: `packages/desktop/crates/pocket/src/explorer/mermaid.rs`

**Context:**
- Merman bakes colours into the SVG, and `Diagrams` caches the laid-out tree and the pixels by key.
- Each scheme therefore needs its own renderer and its own cache key. Switching the appearance then just looks up (or lays out) the other key, with no invalidation.
- Merman wants opaque colours, so translucent tokens are flattened onto the canvas: white in light, `WINDOW_SOLID` in dark.

**Step 1: Write the failing tests.** In `mod tests`, replace `flattens_translucent_tokens_onto_white` and `paints_the_canvas_in_the_window_colour` with the versions below. Imports: `use super::{BUDGET, IN_USE, evict, hex, rasterize, svg, tree};` and `use theme::WINDOW_SOLID;`.

```rust
    #[test]
    fn flattens_translucent_tokens_onto_the_canvas() {
        assert_eq!(hex(0x5b5bd6ff, false), "#5b5bd6");
        assert_eq!(hex(0x00000000, false), "#ffffff");
        assert_eq!(hex(0x00000080, false), "#7f7f7f");
        assert_eq!(hex(0x00000000, true), "#171717");
        assert_eq!(hex(0xebebeb1a, true), "#2d2d2d");
    }

    #[test]
    fn paints_the_canvas_in_each_schemes_window_colour() {
        for dark in [false, true] {
            let t = tree("flowchart LR\n  A --> B\n", dark).unwrap();
            let [r, g, b, a] = WINDOW_SOLID.pick(dark).to_be_bytes();
            assert_eq!(rasterize(&t, 1.).unwrap().as_bytes(0).unwrap()[..4], [b, g, r, a]);
        }
    }
```

In the other tests, add `false` as the last argument of every `svg(..)` and `tree(..)` call.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket mermaid`
Expected: compile errors about argument counts.

**Step 3: Write the implementation.**

```rust
static MERMAN: [LazyLock<Merman>; 2] = [LazyLock::new(|| merman(false)), LazyLock::new(|| merman(true))];

fn merman(dark: bool) -> Merman {
    let roles = [
        (ThemeRole::Canvas, WINDOW_SOLID),
        (ThemeRole::Surface, SURFACE_SUNKEN),
        (ThemeRole::SurfaceAlt, WINDOW_SOLID),
        (ThemeRole::Text, TEXT),
        (ThemeRole::SubtleText, TEXT_3),
        (ThemeRole::Border, TEXT_6),
        (ThemeRole::Line, TEXT_4),
        (ThemeRole::ClusterBackground, SURFACE_SUNKEN),
        (ThemeRole::ClusterBorder, TEXT_6),
        (ThemeRole::NoteBackground, ACCENT_BG),
        (ThemeRole::NoteBorder, ACCENT),
        (ThemeRole::NoteText, TEXT),
        (ThemeRole::Error, FAILED),
    ];
    let hex = |c: u32| hex(c, dark);
    let theme = roles.into_iter().fold(HostTheme::new().try_with_font_family("Geist, sans-serif").unwrap(), |t, (role, c)| t.try_with_role(role, hex(c.pick(dark))).unwrap());
    let presentation = Presentation::new().with_theme(theme.try_with_series_palette(PALETTE.map(hex)).unwrap()).resolve();
    let renderer = Renderer::new().with_engine(presentation.materialize_engine(merman::Engine::new()));
    Merman { renderer, presentation }
}

/// Merman wants opaque colours, so translucent tokens are flattened onto the canvas.
fn hex(c: u32, dark: bool) -> String {
    let [br, bg, bb, _] = if dark { WINDOW_SOLID.pick(true) } else { 0xffffffff }.to_be_bytes();
    let a = (c & 0xff) as f32 / 255.;
    let mix = |v: u32, base: u8| ((v & 0xff) as f32 * a + base as f32 * (1. - a)).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(c >> 24, br), mix(c >> 16, bg), mix(c >> 8, bb))
}

fn svg(source: &str, dark: bool) -> Result<String, String> {
    let merman = &*MERMAN[dark as usize];
    let pipeline = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, root_background_color: Some(hex(WINDOW_SOLID.pick(dark), dark)), ..Default::default() }.pipeline();
    let request = SvgRequest { pipeline: Some(pipeline), presentation: merman.presentation.render_policy(), ..Default::default() };
    match merman.renderer.render(RenderRequest::svg(source, OperationControl::new(), request)) {
        // arms unchanged
    }
}

fn tree(source: &str, dark: bool) -> Result<Arc<Tree>, String> {
    Tree::from_str(&svg(source, dark)?, &USVG).map(Arc::new).map_err(|e| e.to_string())
}
```

Keep the existing doc comment on `svg`.

For `Fence` and the cache:

```rust
struct Fence {
    /// Indexed by `dark`, since each scheme lays out its own colours.
    keys: [u64; 2],
    source: SharedString,
}
```

In `parse`:

```rust
        let key = |dark: bool| {
            let mut hasher = DefaultHasher::new();
            (&code.value, dark).hash(&mut hasher);
            hasher.finish()
        };
        let fence = Fence { keys: [key(false), key(true)], source: code.value.clone().into() };
```

In `render`, open with:

```rust
        let Fence { keys, source } = node.data::<Fence>().unwrap();
        let dark = theme::is_dark();
        let key = keys[dark as usize];
```

Pass `dark` into `d.get(key, source, dark, cx)`. Change `Diagrams::get` to take `dark: bool` and call `tree(&source, dark)` in its background task. Also change the `frame`'s `.bg(WINDOW)` to `.bg(WINDOW_SOLID)`, so the frame matches the rasterized canvas.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket mermaid`
Expected: PASS. `0xebebeb1a` over `#171717` gives `0xeb·0.102 + 0x17·0.898 ≈ 0x2d`. If rounding lands one off, recompute and fix the expected value in the test rather than the code.

Then run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: green.

---

## PR 3: Follow macOS

**Scope:**
- Flip the flag from the system appearance at launch and on every change.
- Blur the window in dark.
- Recolour the cached diff spans.
- Add a `dark` capture step.

**Depends on:** PR 2.

**Done when:**
- Everything is green.
- The light captures are still identical to `/tmp/dark-before`.
- The dark captures of every screen are reviewed.

### Task 3.1: `set_appearance` in theme

**Files:**
- Modify: `packages/desktop/crates/theme/src/theme.rs` (`highlight_theme`, `init`, tests)
- Modify: `packages/desktop/crates/pocket/src/syntax.rs:37` (the `highlight_theme()` call)

**Context:**
- `gpui_kit::component::Theme::change(mode, window, cx)` resets gpui-kit's colours to its built-in light or dark theme, so our overrides must be re-applied after it.
- `ThemeMode: From<WindowAppearance>` already exists; `Theme::sync_system_appearance` passes an appearance straight in.
- gpui-kit's `Root` paints `cx.theme().tokens.background`. Task 3.2 makes it transparent.

**Step 1: Write the failing test.** Replace `colours_syntax_with_our_tokens`:

```rust
    #[test]
    fn colours_syntax_with_our_tokens_in_either_scheme() {
        for dark in [false, true] {
            let t = highlight_theme(dark);
            let color = |name: &str| t.style(name).and_then(|s| s.color);
            for (name, token) in [("keyword", SYN_KEYWORD), ("function", SYN_FN), ("string", SYN_STRING), ("comment", SYN_COMMENT)] {
                assert_eq!(color(name), Some(rgba(token.pick(dark)).into()), "{name} dark={dark}");
            }
        }
    }
```

**Step 2: Run the test to verify it fails**

Run: `cargo test -p theme`
Expected: compile error, `this function takes 0 arguments`.

**Step 3: Write the implementation.**

```rust
/// gpui-kit's highlight theme recoloured with the `SYN_*` tokens, for the code editor, markdown and diff rows.
pub fn highlight_theme(dark: bool) -> Arc<HighlightTheme> {
    let hsla = |c: Token| serde_json::to_value(Hsla::from(rgba(c.pick(dark)))).expect("colour serializes");
    // rest unchanged
}
```

Replace `init` with:

```rust
/// Loads the bundled fonts and follows the system appearance. Call after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()).expect("bundled fonts load");
    set_appearance(cx.window_appearance(), cx);
}

/// Switches every token to `appearance` and points gpui-kit's theme at them.
pub fn set_appearance(appearance: WindowAppearance, cx: &mut App) {
    let dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
    DARK.store(dark, Ordering::Relaxed);
    gpui_kit::component::Theme::change(appearance, None, cx);
    let t = gpui_kit::component::Theme::global_mut(cx);
    t.font_family = SANS.into();
    t.font_size = px(14.);
    t.foreground = TEXT.into();
    t.muted_foreground = TEXT_2.into();
    t.background = WINDOW_SOLID.into();
    // gpui-kit paints markdown code blocks and their copy-button backdrop with `muted`.
    t.muted = WELL.into();
    t.caret = TEXT.into();
    t.mono_font_family = MONO.into();
    t.mono_font_size = px(13.);
    t.link = ACCENT.into();
    t.highlight_theme = highlight_theme(dark);
}

/// Glass in dark, as monocode does; pale desktops make translucent light chrome illegible.
pub fn window_background() -> WindowBackgroundAppearance {
    if is_dark() { WindowBackgroundAppearance::Blurred } else { WindowBackgroundAppearance::Opaque }
}
```

In `syntax.rs`, change `&*theme::highlight_theme()` to `&*theme::highlight_theme(theme::is_dark())`.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p theme && cargo test -p pocket syntax && cargo build --workspace`
Expected: PASS.

### Task 3.2: Follow the window's appearance

**Files:**
- Modify:
  - `packages/desktop/crates/pocket/src/main.rs` (`WindowOptions`, `Root::new`)
  - `packages/desktop/crates/pocket/src/desktop.rs` (`Desktop::new` `_subs`, a new `set_appearance` method next to `follow_reduce_motion`)
  - `packages/desktop/crates/pocket/src/git_ui/diff.rs` (`DiffState::apply`, `load_diff`, a new `recolor_diff`)

**Context:**
- The diff's syntax colours are computed off the UI thread and cached in `DiffState::{syntax, hl}`. `read_diff` recolours only when the lines differ from `shown`.
- Passing an empty `shown` forces a recolour, with no new machinery. `apply` must then report a change even when the lines are equal, or nobody redraws.
- The markdown view, the code editor and the terminal read colours at render, so a `cx.notify()` covers them.

**Step 1: `main.rs`.**
- Add `window_background: theme::window_background(),` to the `WindowOptions` literal.
- Replace `cx.new(|cx| Root::new(view, window, cx))` with:

```rust
            // Root paints gpui-kit's opaque background, which would hide the blurred desktop in dark.
            cx.new(|cx| Root::new(view, window, cx).bg(transparent_black()))
```

Light is unaffected: `Desktop`'s own root already paints `WINDOW`, which is opaque in light.

**Step 2: `desktop.rs`.** Add the observer to the `_subs` vec literal in `Desktop::new`:

```rust
            cx.observe_window_appearance(window, |this, window, cx| this.set_appearance(window.appearance(), window, cx)),
```

Add to `impl Desktop`, next to `focus_agent` or at the end of the first `impl Desktop` block:

```rust
    pub(crate) fn set_appearance(&mut self, appearance: WindowAppearance, window: &mut Window, cx: &mut Context<Self>) {
        theme::set_appearance(appearance, cx);
        window.set_background_appearance(theme::window_background());
        self.recolor_diff(cx);
        cx.notify();
    }
```

**Step 3: `diff.rs`.** Change `DiffState::apply` so new colours count as a change:

```rust
    /// Shows `load` unless another file or fold state was picked while it ran.
    pub fn apply(&mut self, load: DiffLoad) -> bool {
        if self.file.as_ref() != Some(&load.path) || load.open != self.open {
            return false;
        }
        self.source = load.source;
        let recolored = load.colors.map(|(syntax, hl)| (self.syntax, self.hl) = (syntax, hl)).is_some();
        self.set_lines(load.lines, true) | recolored
    }
```

Use a non-short-circuit `|` so `set_lines` always runs. Split `load_diff` so the recolour shares its task:

```rust
    pub(crate) fn load_diff(&mut self, cx: &mut Context<Self>) {
        let shown = self.diff.lines.clone();
        self.read_diff_in_background(shown, cx);
    }

    /// Colours the shown diff again, for a new appearance.
    pub(crate) fn recolor_diff(&mut self, cx: &mut Context<Self>) {
        self.read_diff_in_background(Vec::new(), cx);
    }

    fn read_diff_in_background(&mut self, shown: Vec<Line>, cx: &mut Context<Self>) {
        let Some((cwd, path)) = self.cwd().zip(self.diff.file.clone()) else { return };
        let open = self.diff.open.clone();
        let task = cx.background_executor().spawn(async move { read_diff(&cwd, path, open, &shown) });
        // spawn/apply/notify block unchanged from load_diff
    }
```

**Step 4: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: green.

### Task 3.3: Capture dark screens

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`, `reset`)

**Context:**
- Capture mode renders a hidden window, and the system appearance can't be flipped from a test.
- A `dark` step forces the scheme; `reset` puts light back, so the steps before it in every screen start from light.

**Step 1: Add the step.** Change the array length to 13 and append:

```rust
    ("dark", |d, window, cx| d.set_appearance(WindowAppearance::Dark, window, cx)),
```

At the end of `reset`, add:

```rust
    d.set_appearance(WindowAppearance::Light, window, cx);
```

**Step 2: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: green.

**Step 3: Capture both schemes.** From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/dark-pr3 home= session=session explore=explore changes=changes,comment inbox=inbox palette=palette new=new-session add=add-repo rail=rail \
  dark-home=dark dark-session=dark,session dark-explore=dark,explore dark-changes=dark,changes,comment dark-inbox=dark,inbox dark-palette=dark,palette dark-new=dark,new-session dark-add=dark,add-repo dark-rail=dark,rail
```

Expected:
- The light PNGs compare `None` against `/tmp/dark-before`.
- In the `impl-dark-*.png` images:
  - all text is legible;
  - there are no white blocks left: popovers are `#232323`, panes are glass;
  - diff and code syntax is in the dark palette;
  - primary buttons are light with dark text.

Capture renders offscreen, so the glass areas appear transparent or black in the PNG, not blurred.
