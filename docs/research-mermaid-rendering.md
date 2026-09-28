# Research: rendering mermaid in the markdown preview

Date: 2026-09-28. Zed at `1a28cff4b409169bac058bca40dfbfeb7621d19b`. Our stack: gpui-kit 0.6.6 (gpui-pre 0.3.6), edition 2024. Background: `docs/research-zed-diff-rendering.md` §4.10, `docs/spike-zed-preview.md`.

## TL;DR

- **Zed renders mermaid** natively, with no JS. The pipeline is **merman** (pure Rust, MIT OR Apache-2.0) → SVG → gpui's resvg `SvgRenderer` → `img(ImageSource::Render)`. It runs on the background executor and is cached by content hash (§1).
- **Renderer: use merman** (`=0.8.0-alpha.6`, `SvgPipelinePreset::ResvgSafe`). In the spike it matched mermaid.js visually, rendered typical diagrams in 0.25–3.2 ms warm, and covers 35 diagram families.
  - mermaid-rs-renderer is the runner-up: fewer deps, but layout bugs and a 68 ms flowchart.
  - Anything JS-based needs a real browser (mermaid measures text with `getBBox`), which means 1–6 s and GBs of Chrome.
- **Hook exists:** gpui-kit's `MarkdownPlugin` (block) can claim `mdast::Node::Code` with `lang == "mermaid"` before the built-in code block handles it. No markdown splitting is needed (§4).
- **Display:** `cx.svg_renderer().parse_svg` + `render_parsed` produce a 2x `RenderImage`. Show it with `img(ImageSource::Render(..))` at explicit w/h inside a horizontal scroller. `svg()` is monochrome, so it's no use for diagrams (§3).
- **Perf:** render and rasterize on `background_spawn` (5–70 ms total). Show the source code block as the placeholder. On error, show the source plus the message. Cache by source hash, then invalidate `TextViewState` layout when a render lands (§5).
- **Main risks:**
  - merman is alpha, so pin it exactly.
  - ResvgSafe fails hard on flowcharts of ~150+ nodes.
  - Geist isn't visible to resvg.
  - Off-screen diagrams keep a stale height unless invalidated.

### Citation legend

- `Z path:L` = zed at SHA above.
- `R crate/path:L` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/...`.
- `P path:L` = repo path relative to `packages/desktop/`.
- `S` = timing spike in `/tmp/mermaid-spike` (not in repo). Release build, M-series Mac, median of 20 warm runs, `cargo tree -e normal` crate counts.

## 0. What we have today

- `.md` preview is `TextView::markdown(id, code_text)` with a local `TextViewStyle` (`P crates/pocket/src/explore.rs:322-336`). Fenced code renders as a plain highlighted code block.
- Text is loaded in `refresh_git` on a background task (`P crates/pocket/src/main.rs:513-545`, `explore::load` :541). The refresh runs about every 2 s (:831-836). `MAX_BYTES` is 512 KB (`P crates/pocket/src/explore.rs:15`).
- Theme is light-only tokens (`P crates/theme/src/theme.rs`). Geist is loaded into the gpui text system only (:127-136, `add_fonts` :167).
- No resvg/usvg/mermaid dependency (`Cargo.toml:25`).

## 1. Does Zed render mermaid? Yes [GPL: mermaid_render, markdown]

- **Crate:** `Z crates/mermaid_render/Cargo.toml:1-33`, GPL-3.0-or-later.
  - It wraps `merman = "=0.8.0-alpha.5"` with default features off and features `layout-cytoscape` + `svg`, plus quick-xml.
- **Render:** `render_to_svg(source, &MermaidTheme) -> Result<String>` (`Z crates/mermaid_render/src/mermaid_render.rs:178-182`).
  - Builds merman `HeadlessRenderer` with a site config, the vendored text measurer and a diagram id, then `SvgPipeline::resvg_safe()` + a CSS override postprocessor, then `render_svg_with_pipeline_sync` (`Z crates/mermaid_render/src/render.rs:7-31`).
- **Theme:** `MermaidTheme` has about 25 colours plus font family (`mermaid_render.rs:78-106`). It maps to merman `themeVariables`: theme `base`, `darkMode`, `htmlLabels`, `cScale0-7`, `pie1-8`, … (`render.rs:33-134`).
  - It is built from editor theme colours (`Z crates/markdown/src/mermaid.rs:362-405`).
  - `sans-serif` is appended to the font because merman measures text font-agnostically (:340-360).
- **Postprocess:** quick-xml event passes (accent colours, CSS inject, foreignObject strip, element fixups), because merman's raw CSS was "borderline unusable" under resvg (`mermaid_render.rs:1-64`; PR [#57644](https://github.com/zed-industries/zed/pull/57644)).
- **Detection:** fenced ` ```mermaid [scale] `, with scale clamped to 10–500 (`Z crates/markdown/src/mermaid.rs:408-420`).
  - A whitelist of 15 type prefixes (:423-453).
  - `.mermaid`/`.mmd` files are also accepted (:455-505).
  - Extraction runs inside the background markdown parse (`Z crates/markdown/src/markdown.rs:1271-1272`).
- **Async + cache:** a `HashMap<contents+scale, Arc<CachedMermaidDiagram>>` (`mermaid.rs:20, :29-57`).
  - On a miss it does `cx.spawn` → `background_spawn { render_to_svg → svg_renderer.parse_svg → render_parsed(scale) }` → `OnceLock` → `cx.notify` (:203-253).
  - The previous image stays as the fallback while an edit re-renders (:90-138).
  - Zoom re-rasterizes the cached `ParsedSvg` (:153-200, :255-290). Stale images are freed with `cx.drop_image` (:292-316).
  - A theme change clears the cache (`markdown.rs:671-678, :750-768`).
- **Element:** `img(ImageSource::Render)` at explicit w/h inside a horizontal scroll container, not `max_w_full` (`mermaid.rs:571-745`; [#61051](https://github.com/zed-industries/zed/pull/61051)).
  - Code/Diagram tab toggle.
  - An error shows the source.
  - While pending, it shows the previous image, else the code plus a pulsing "Rendering…".
- **History:** [#49064](https://github.com/zed-industries/zed/pull/49064) (2026-02-13) shipped the first version on mermaid-rs-renderer, closing issue [#10696](https://github.com/zed-industries/zed/issues/10696) (365 reactions).
  - [#57644](https://github.com/zed-industries/zed/pull/57644) (2026-05-27) switched to merman for fidelity.
  - Related: [#56468](https://github.com/zed-industries/zed/pull/56468) (pixmap cap), [#51623](https://github.com/zed-industries/zed/pull/51623) (SVG text without system fonts).
  - Open: [#51480](https://github.com/zed-industries/zed/issues/51480) (complex diagrams break).

**Takeaway:** we can reuse the same architecture with the same permissive crate (merman). Don't copy anything from `mermaid_render`/`markdown` (GPL).

## 2. Rendering options in Rust

### 2.1 merman [MIT OR Apache-2.0]

- **Status:** 0.8.0-alpha.6 (2026-09-02); stable 0.7.0; about 100k downloads; MSRV 1.95 (`R merman-0.8.0-alpha.6/Cargo.toml:14, :45`). Repo [Latias94/merman](https://github.com/Latias94/merman).
- **Coverage:** follows mermaid@11.17.2 with parity checks, 35 families (README :23-24, :176). `layout-cytoscape` (crate `manatee`, MIT/Apache) is needed for mindmap (`R merman-render-0.8.0-alpha.6/src/mindmap.rs`).
- **API** (alpha.6; alpha.5 still has `HeadlessRenderer`):
  ```rust
  let out = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, ..Default::default() };
  let req = SvgRequest { pipeline: Some(out.pipeline()), ..Default::default() };
  Renderer::new().render(RenderRequest::svg(src, OperationControl::new(), req))?
  // -> RenderOutput::Svg(Some(svg)) => svg.svg()
  ```
  `OperationControl` gives cancellation and deadlines (README :143-149).
- **Theming:**
  - Build a `HostTheme` with an appearance, a font family, `ThemeRole` colours (Canvas, Surface, Text, Border, Line, Note*, Actor*, Cluster*, …) and a series palette.
  - Resolve it with `Presentation::new().with_theme(t).resolve()`, which feeds `Renderer::with_engine(resolved.materialize_engine(..))` + `SvgRequest.presentation` (`R merman-0.8.0-alpha.6/examples/custom_presentation_theme.rs`; roles `R merman-render-0.8.0-alpha.6/src/presentation/theme.rs:30`, `HostTheme` :117).
- **Text measurement:** `DeterministicTextMeasurer` by default, which is font-free (`src/text/deterministic.rs:63`). A pluggable `TextMeasurer` "for editors… to match their own font system" is available (`src/text/measure.rs:16-29`, `environment.rs:803, :1573`).
- **ResvgSafe is required** (S). The default preset emits `<foreignObject>` labels, which resvg ignores, so every label comes out blank. ResvgSafe emits `<text>` fallbacks (`R merman-render-0.8.0-alpha.6/src/svg/pipeline/preset.rs:17-34`).
- **Output:** a `<style>` block, `width="100%"`, and a `viewBox` that usvg sizes from. Default font is `"trebuchet ms",verdana,arial,sans-serif` (S).
- **Deps:** usvg is a dev-dependency only (`R merman-0.8.0-alpha.6/Cargo.toml:447`), so there is no clash with gpui's resvg 0.46.
  - 162 crates with defaults (includes math); 109 with `features=["svg"]`. Clean release build 86 s (S).
  - ELK layout is EPL-2.0 and opt-in. The math feature may bundle OFL font data (README :115-130, :254-256).

### 2.2 mermaid-rs-renderer (mmdr) [MIT]

- 0.3.1 (2026-07-06), about 138k downloads, [1jehuang/mermaid-rs-renderer](https://github.com/1jehuang/mermaid-rs-renderer). README says "active early development"; 23 types (README :303).
- **API:** `render(&str) -> anyhow::Result<String>`, `render_with_options(&str, RenderOptions{theme, layout})` (`R mermaid-rs-renderer-0.3.1/src/lib.rs:205, :222`). `Theme` has public fields and 5 presets (`src/theme.rs:37-67`).
- **Output:** plain SVG with inline attributes: no `<style>`, no foreignObject, explicit size (S).
- **Text measurement:** measures real glyphs via fontdb behind a global `Mutex` (`src/text_metrics.rs:12, :72`).
  - **Side effect:** it writes font copies to `~/.cache/mmdr/font-cache` (:275-284).
- **Deps:** 32 crates; 36 s clean release build (S).
- **Quality (S):**
  - flowchart edge label detached from its edge;
  - sequence loop box collapses;
  - class diagrams sprawl and drop the `*` multiplicity;
  - `<<enumeration>>` fails to parse.

### 2.3 Other pure-Rust crates (not tested)

- [mermaid-to-svg](https://github.com/warpdotdev/mermaid-to-svg) (Warp): flowchart first, the rest experimental.
- mermaid-svg 0.7.0, mermaid-render 0.10.0, rusty-mermaid 0.2.0.
- mermaid-little 11.14.0-6 (claims byte-exact parity).
- mmdflux 2.6.1.
- mermaid-text / merman-ascii: terminal output only.

All are MIT, and none has merman's coverage or Zed's field use.

### 2.4 Embedded JS engine (rquickjs / boa / deno_core + mermaid.js)

- Not viable. mermaid needs a DOM plus `SVGTextElement.getBBox` for layout. jsdom lacks it: "browser environment is required to precompute widths/heights" ([mermaid#3650](https://github.com/mermaid-js/mermaid/issues/3650), open since 2022; also [#6634](https://github.com/mermaid-js/mermaid/issues/6634), [discussion #3843](https://github.com/orgs/mermaid-js/discussions/3843)).
- No rquickjs/boa/deno_core mermaid project was found. We would have to write a text-measuring DOM shim ourselves.
- `mermaid-rs` 0.1.1 ("Rust bindings for Mermaid JS") drives `headless_chrome` (crates.io deps).

### 2.5 mermaid-cli (`mmdc`)

- Requires Node ≥22.13 + puppeteer ^25 ([package.json](https://github.com/mermaid-js/mermaid-cli/blob/master/package.json) :12-13, :47). Puppeteer downloads Chrome for Testing, about 170 MB on macOS ([installation](https://pptr.dev/guides/installation) :9-10).
- **Measured (mmdc 12.0.0, M3 Pro):**
  - cold run 6.0 s; warm runs 1.0–2.4 s;
  - `node_modules` 443 MB; puppeteer cache 1.6 GB.
  - Output had 6 `<foreignObject>` and 0 `<text>`, so resvg would drop every label. It would need a PNG output instead.

### 2.6 Remote services

- [kroki](https://github.com/yuzutech/kroki): its mermaid companion is a puppeteer 25.1.0 server (`kroki/mermaid/package.json:26`).
- [mermaid.ink](https://github.com/jihchi/mermaid.ink): headless Chrome (README :62-63), self-hostable via docker (:26).
- Either way the diagram source leaves the machine and needs a network connection, and we get PNG/SVG back with the same foreignObject problem for SVG.

### 2.7 Comparison

| Option | Coverage | Fidelity | License | Deps | Speed (typical / 100 nodes) | Offline |
|---|---|---|---|---|---|---|
| **merman ResvgSafe** | 35 families | ≈ mermaid.js; minor state/generic glitches (S) | MIT OR Apache-2.0 | 109–162 crates, +86 s clean build | 0.25–3.2 ms / 43 ms; ≥150 nodes errors (S) | yes |
| mermaid-rs-renderer | 23 types | own "modern" look; layout bugs (S) | MIT | 32 crates, +36 s | 0.04–1.7 ms, flowchart 68 ms / 206 ms (S) | yes (writes `~/.cache/mmdr`) |
| Other Rust crates | mostly flowchart/partial | unknown | MIT | small | unknown | yes |
| JS engine + mermaid.js | full in theory | blocked on `getBBox` | MIT + engine | engine + DOM shim | unknown | yes |
| mmdc CLI | full | exact (PNG); SVG labels lost in resvg | MIT (+Chromium) | Node + 443 MB + 1.6 GB Chrome | 1–2.4 s warm, 6 s cold | yes |
| kroki / mermaid.ink | full | exact | service | HTTP client | network RTT + ~1 s | no (unless self-hosted) |

Raster cost on top of any SVG option: resvg at 2x takes 4–23 ms per diagram, and the first system fontdb load takes 25.6 ms (949 fonts) (S).

## 3. SVG in GPUI [Apache: gpui-pre]

- `cx.svg_renderer() -> SvgRenderer` (`R gpui-pre-0.3.6/src/app.rs:1756`). It is `Clone` and can be moved into background futures, as `img` does (`src/elements/img.rs:624-631`).
- `parse_svg(&[u8]) -> Result<ParsedSvg>` (`src/svg_renderer.rs:190`). `render_parsed(&ParsedSvg, SvgSize) -> Result<Arc<RenderImage>>` (:195-219).
  - A `ScaleFactor` size is multiplied by `SMOOTH_SVG_SCALE_FACTOR = 2` (:81), so the image is the natural SVG size at 2x pixels. `RenderImage::size()` is public but `render_size()`/`scale_factor` are `pub(crate)` (`src/assets.rs:42-93`), so compute the logical size as `size(0) / 2`.
- The pixmap is capped at 8192 px (:270-300), which is about 4096 logical px at 2x.
- **Fonts:** a lazy system fontdb, falling back to sans-serif (:124-187). Bundled fonts are requested from the AssetSource at `fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf` and `fonts/lilex/Lilex-Regular.ttf` (:309-320).
  - Our `Assets` serves neither (`P crates/theme/src/theme.rs:152-158`), and gpui-kit-assets ships no `.ttf`. So diagrams render in a system font, not Geist.
- **Elements:**
  - `img(ImageSource::Render(Arc<RenderImage>))` (`src/elements/img.rs:40-50`, From impls :101-126).
    - Auto size means intrinsic size (:348-378): `max_w_full` shrinks only the width and `object_fit(Contain)` letterboxes it. So set explicit `w`/`h` and scroll horizontally (the same fix as Zed #61051).
  - `img(Arc<Image>)` with `ImageFormat::Svg` also works (`src/platform.rs:2953, :3001-3040`), but it always rasterizes at scale 1.0, which blurs on Retina.
  - `svg().data(..)` paints a single-colour alpha mask (`src/elements/svg.rs:39-62`, `src/window.rs:4815-4833`), which is wrong for multi-colour diagrams.
- `cx.drop_image(Arc<RenderImage>, window)` frees the atlas entry (`src/window.rs:4997`).

## 4. Hooking fenced code in gpui-kit's TextView [Apache: gpui-base, gpui-component]

- **Trait:** `trait MarkdownPlugin { is_block(), name(), parse(&mdast::Node, &MarkdownParseContext) -> Option<MarkdownNode>, render(&MarkdownNode, &mut Window, &mut App) -> impl IntoElement }` (`R gpui-base-0.6.6/src/text/markdown_ext.rs:45-74`).
  - Register it with `TextView::plugin(..)` (`src/text/text_view.rs:398`) or `markdown_block_parser`/`_renderer` closures (:369, :384).
  - All of these are re-exported via `gpui_kit` / `gpui_component::text` (`R gpui-component-0.6.6/src/text/mod.rs:1-16`).
- **Order:** `ast_to_node` asks `parse_block` before built-in handling (`src/text/format/markdown.rs:392-398`). So a plugin can claim `Node::Code` (which carries `lang`, :450-454), including inside lists and quotes.
  - The result becomes `BlockNode::Custom`, rendered via `render_block` in a `div().pb(mb)` (`src/text/node.rs:2969-2976`).
- **Parse contract:**
  - Parsers may run on a background task, with no `Window`/`App` (`markdown_ext.rs:23-33`). Docs ≤4 KB parse on the UI thread (`src/text/state.rs:40, :472`). **So never render mermaid in `parse`.**
  - Store `{source, hash}` via `MarkdownNode::new(name, data).text(..).markdown(cx.node_source(node)?)` (`markdown_ext.rs:100, :119, :191-203`).
- **Precedent:** gpui-component's `FrontmatterPlugin` is a block plugin matching `mdast::Node::Yaml` (`R gpui-component-0.6.6/src/text/frontmatter.rs:47-81`).
- **No reparse churn:** extensions are re-synced every frame (`text_view.rs:582` → `state.rs:363-379`). An unchanged plugin set doesn't reparse (`markdown_ext.rs:350-367`).
- **Selection:** a custom node contributes `text()` only to select-all copy, not to partial selection (`node.rs:271-279, :324, :352`). Source copy uses `markdown()` (:2351).
- **Workaround if the hook didn't exist:** split the source on mermaid fences and render alternating `TextView`s and images. That breaks cross-block selection, the single scroll/virtualized list, and heading anchors. Not needed.

## 5. Performance and UX

- **Where it runs:** `render` sees a cache miss and spawns `background_spawn(merman → SVG → parse_svg → render_parsed(ScaleFactor(1.0)))`. Typical total is 5–30 ms; a 100-node flowchart is about 43 ms + raster (S).
  - Never on the UI thread: the first fontdb load alone is 25.6 ms.
- **Cache:**
  - Key `hash(source)`. Hold `Pending | Ready(Arc<RenderImage>) | Failed(String)`.
  - The 2 s refresh re-sets identical text, so hits must be free.
  - Evict entries not rendered in the current document and `drop_image` them.
  - If a theme is ever added, clear the cache when it changes.
- **Remeasure:** `ListState` runs `measure_all()` on reset (`src/text/state.rs:221`), so every diagram's `render` runs (and spawns) at open, and off-screen items keep their cached height (`R gpui-pre-0.3.6/src/elements/list.rs:1061-1122`).
  - When a render lands, call `TextViewState::invalidate_inline_layout` (`state.rs:455-465`). That needs our own state entity via `TextView::new(&state)` (`text_view.rs:160`) instead of `TextView::markdown(id, ..)`.
- **Placeholder:** render the fence as the normal code block style plus a small "Rendering…" caption. This keeps the height close for short diagrams and is readable if rendering never finishes.
- **Error:** merman errors are readable, e.g. "Diagram parse error (flowchart-v2): Unterminated node label (missing `]]`)" or "No Mermaid diagram type detected" (S). Show the source block with a `FAILED`-tinted one-line message.
- **Limits:**
  - Cap source length (e.g. 50 KB) and use `OperationControl` deadlines to bound pathological input.
  - ResvgSafe fails on flowcharts of about 150+ nodes with `svg_fallback_selector_index exceeded … max=16777216` (S). That is a hard constant, `MAX_SELECTOR_MATCH_WORK` (`R merman-render-0.8.0-alpha.6/src/svg/fallback/cascade.rs:35`, raised at `fallback.rs:69`). Treat it as the error state.

## 6. Recommended approach

merman ResvgSafe → gpui `SvgRenderer` → `img(ImageSource::Render)`, via a `MarkdownPlugin`. Scope: fenced ` ```mermaid ` in `.md` preview only; `.mmd` files later.

`Cargo.toml` (workspace, then `crates/pocket`):

```toml
merman = { version = "=0.8.0-alpha.6", default-features = false, features = ["svg", "layout-cytoscape"] }
```

New `crates/pocket/src/mermaid.rs` (sketch, our own code):

```rust
pub struct Mermaid { cache: Entity<MermaidCache> }            // Entity is Send+Sync
struct Fence { key: u64, source: SharedString }
enum Diagram { Pending, Ready(Arc<RenderImage>), Failed(SharedString) }
pub struct MermaidCache { items: HashMap<u64, Diagram> }

impl MarkdownPlugin for Mermaid {
    fn is_block(&self) -> bool { true }
    fn name(&self) -> &str { "mermaid" }
    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext) -> Option<MarkdownNode> {
        let mdast::Node::Code(code) = node else { return None };
        (code.lang.as_deref() == Some("mermaid")).then(|| {
            MarkdownNode::new("mermaid", Fence { key: hash(&code.value), source: code.value.clone().into() })
                .text(code.value.clone())
                .markdown(cx.node_source(node).unwrap_or(&code.value))
        })
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let fence = node.data::<Fence>().unwrap();
        match self.cache.update(cx, |c, cx| c.get_or_spawn(fence, cx)) {
            Diagram::Ready(image) => {
                let dim = image.size(0);                      // render_size is pub(crate); raster is 2x
                div().id(fence.key as usize).overflow_x_scroll().max_w_full()
                    .child(img(image).w(px(dim.width.0 as f32 / 2.)).h(px(dim.height.0 as f32 / 2.)))
            }
            Diagram::Pending => code_placeholder(&fence.source, "Rendering…"),
            Diagram::Failed(e) => code_placeholder(&fence.source, e),
        }
    }
}
```

In `get_or_spawn`, on a miss, insert `Pending` and then:

```rust
cx.spawn(async move |this, cx| {
    let svg = cx.svg_renderer();
    let out = cx.background_spawn(async move {
        let text = merman_svg(&source)?;
        svg.render_parsed(&svg.parse_svg(text.as_bytes())?, SvgSize::ScaleFactor(1.))
    }).await;
    this.update(cx, |c, cx| { c.items.insert(key, out.into()); cx.notify() })
})
```

- `merman_svg` builds a `Renderer` once. It uses a `HostTheme` from tokens:
  - Canvas `SURFACE`, Surface `SURFACE_SUNKEN`, Text `TEXT`, SubtleText `TEXT_3`, Border `HAIRLINE`, Line `TEXT_4`, Note* from `ACCENT_BG`/`ACCENT`, Error `FAILED`;
  - palette from `PALETTE`;
  - font `"Geist, Helvetica Neue, sans-serif"`.
- **Wiring:**
  - `main.rs`: hold `md_state: Entity<TextViewState>` and `mermaid: Entity<MermaidCache>`. `cx.observe(&mermaid, ..)` calls `md_state.update(cx, |s, cx| s.invalidate_inline_layout(cx))` and `cx.notify()`. `sync_code` sets text on `md_state` instead of the `code_text` string path.
  - `explore.rs:322`: `TextView::new(&md_state).plugin(Mermaid { cache }).selectable(true).scrollable(true).style(..)`, with the existing style unchanged.
- **Geist in diagrams (optional polish):** register Geist in the SVG fontdb by serving the Geist TTF from `Assets` at `fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf` (`R gpui-pre-0.3.6/src/svg_renderer.rs:309-320`). It's a hack, so it's only worth doing if the system-font look clashes.

## 7. Licensing

- **Zed:** `mermaid_render` and `markdown` are GPL-3.0-or-later, so the ideas above are reimplemented and no code is copied.
- **Safe to use:** merman, merman-core, merman-render and manatee are MIT OR Apache-2.0 (`R merman-*/Cargo.toml`). gpui/gpui-kit are Apache-2.0. resvg/usvg are already linked via gpui.
- **Avoid:**
  - merman `layout-elk` (EPL-2.0);
  - `math` unless needed (it may bundle OFL fonts, which would need attribution);
  - `complete-svg`, because it pulls in math.

## 8. Open risks

1. **Alpha dependency.** The API changed between alpha.5 (Zed) and alpha.6, so pin `=` and expect churn when upgrading. MSRV is 1.95.
2. **Big flowcharts fail** under ResvgSafe at ≥150 nodes (S). The only fallback is the error state, unless upstream lifts `MAX_SELECTOR_MATCH_WORK`.
3. **Font mismatch.** merman lays out with font-free metrics while resvg draws with a system font, so labels can overflow boxes slightly. A `TextMeasurer` backed by the same fontdb fixes this at the cost of more code.
4. **Stale heights.** A missed `invalidate_inline_layout` shows overlapping or clipped blocks until the next scroll or resize.
5. **Size cap.** Diagrams over about 4096 logical px hit the 8192 px pixmap cap and are downscaled, so they look blurry.
6. **Retina only.** The 2x raster is fixed; zoom would need re-rasterizing from `ParsedSvg`.
7. **Selection.** Diagrams copy their source only on select-all; partial selection skips them.
8. **Build cost.** 109+ crates and about 86 s added to a clean release build (S). Measure binary size before merging.
9. **Minor fidelity glitches** (S): state-diagram edge labels leaving a composite state overlap, and nested class generics render slightly wrong.
