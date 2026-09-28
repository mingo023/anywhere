# Spike: Mermaid diagrams in the markdown preview

Date: 2026-09-28. Code in `docs/spike-mermaid/` (standalone crate, not in the desktop workspace). Background: `docs/research-mermaid-rendering.md`.
Stack: gpui-kit 0.6.6, merman `=0.8.0-alpha.6` (`svg`, `layout-cytoscape`), resvg 0.46, Rust 1.98.1, release build, M-series Mac, 120 Hz display.

## Question

Can ```` ```mermaid ```` fences in Explore's markdown preview render as themed diagrams, fully in-process, without hurting open time or scrolling?

## What was built

A `TextView` over a generated document: a broken fence, a Rust fence, then N copies of 7 diagrams (flowchart, sequence, class, state, ER, pie, gantt), plus one 100-node flowchart. The document holds 72 diagrams at the default 10 copies.

- **Hook:** a gpui-kit `MarkdownPlugin` claims `mdast::Node::Code` with `lang == "mermaid"`. Other fences stay normal code blocks. It works with `TextView::new(&Entity<TextViewState>)`.
- **Render:** merman → SVG (`SvgPipelinePreset::ResvgSafe`, so labels are `<text>` and there is no `<foreignObject>`) → `usvg::Tree` → tiny-skia pixmap → BGRA `RenderImage` → `img(ImageSource::Render)`.
- **Theme:** merman `HostTheme` roles map to `theme` tokens (translucent tokens are flattened onto white), with `PALETTE` as the series palette and Geist as the font.
- **Cache:** an entity keyed by the source hash. Background work finishes, then `invalidate_inline_layout` remeasures the list.

Two raster strategies:

| Mode | Background task | On paint |
|---|---|---|
| Eager | merman + parse + rasterize at 2× | nothing |
| Lazy (`SPIKE_BUDGET_MB`) | merman + parse into a `Tree`, so the size is known | rasterize at `window.scale_factor()` when first painted; LRU eviction over a byte budget with `window.drop_image` |

Run it from `docs/spike-mermaid/`:
- `./bench.sh <px|open> [copies] [system|geist]`
- `./shot.sh <name> [font] [scroll-px]`

`SPIKE_POPUP=1` opens a `WindowKind::PopUp` window so it gets frames while you work in another Space.

## Results

### Open cost

| Step | Time |
|---|---|
| merman init (theme + engine, once) | 2–5 ms |
| usvg fontdb, system fonts + Geist (once) | 27–77 ms |
| merman SVG per diagram, serial | 5–14 ms |
| merman SVG, 100-node flowchart | 55 ms |
| usvg parse + 2× raster per diagram, serial | 20–35 ms |
| 72 diagrams in parallel, all ready after open | 240–360 ms |

- **All work runs on background threads.** Fences show "Rendering diagram…" and then pop in with one remeasure.
- **No UI-thread hitch.** Scrolling while all 72 diagrams land (`SPIKE_RELOAD`) stays smooth.

### Memory

| Mode | Diagrams | Images held |
|---|---|---|
| Eager 2× | 9 | 58 MB |
| Eager 2× | 72 | 391–399 MB |
| Lazy, 64 MB budget, 40 px/frame | 72 | 56 MB live, 74 MB peak |
| Lazy, 64 MB budget, 120 px/frame | 72 | 78 MB peak, 71 rasters |
| Lazy, 16–32 MB budget | 72 | 37 MB live, 78 MB peak |

The 100-node flowchart alone takes about 21 MB at 2×. The floor is the diagrams on screen plus that chart, because images painted in the last 250 ms are never evicted.

### Scrolling (4 s, 72 diagrams)

| Mode | px/frame | Frame p50 / p99 / max | >33 ms |
|---|---|---|---|
| Eager | 40 | 8.33 / 9.2 / ≤21 ms | 0 |
| Eager | 120 | 8.33 / 10.9 / ≤21 ms | 0 |
| Eager, all diagrams re-landing mid-scroll | 40 | 8.33 / ~10 ms | 0 |
| Lazy 64 MB | 40, 120 | 8.33 / ~9.3 ms | 0 |

Lazy mode costs about 2 blank frames per diagram (pop-in) while it rasterizes off-thread. The frame box keeps its final size, so nothing shifts.

### Look

Screenshots via `shot.sh` (`*.png` in `docs/spike-mermaid/`, gitignored):
- **Themed.** Flowchart, sequence, class, ER, pie and gantt read cleanly with our tokens.
- **Fonts.** Geist and system sans look nearly identical. Geist matches the design system.
- **Errors.** A broken fence shows merman's parse error in `FAILED` and the source in `MONO`.
- **stateDiagram edge labels overlap** in both font modes. This is a merman layout bug.
- One blank frame after a programmatic scroll was seen once; a retake was clean.

### Real-app trial

The production module and wiring from the plan were built in a scratch copy of `packages/desktop`:
- `cargo test -p git -p theme -p pocket`: green, including 6 new `mermaid::tests`.
- `cargo clippy -p git -p theme -p pocket`: only the existing `double_ended_iterator_last` warning.
- Release binary: 44 MB → 62 MB (+18 MB: merman, its HTML/CSS parsers and layout engines). A clean release build takes 4 min 35 s.
- The plan's code blocks, applied task by task to a fresh copy, reproduce the same green tree.

## Gotchas

- **Plugin `render` runs for every block when the file opens.** `TextViewState` uses `ListState::measure_all()`, so every fence starts its background layout immediately. This is fine for pixels only in lazy mode, where rasterizing waits for paint.
- **gpui's `SvgRenderer` fontdb has no Geist.** It holds system fonts plus IBM Plex/Lilex. Use your own `usvg::Options` and load `theme::FONTS` into it.
- **`usvg::Options` is not `Clone`.** Share one in a `LazyLock`.
- **merman wants opaque `#rrggbb`.** Flatten translucent tokens first (for example `HAIRLINE`).
- **tiny-skia gives premultiplied RGBA and gpui wants BGRA.** Swap bytes 0 and 2.
- **Free evicted images with `window.drop_image`.** Otherwise the atlas keeps them.
- **`TextViewState::set_text` keeps scroll.** Call `list_state().scroll_to(ListOffset::default())` yourself when a different file opens.
- **Rebuilding the plugin every frame is cheap.** `TextView` compares parser shape, not identity, so it doesn't reparse.
- **Occluded windows get no frames.** Benchmarks need `WindowKind::PopUp` (or the window on screen).
- **`merman` pins `granit-parser = "=1.0.0"`.** Adding it moves `serde-saphyr` (through `rust-i18n`) down to 0.0.29 in `Cargo.lock`. It builds and tests fine, but watch it.

## Risks

- **Binary size.** It grows by 18 MB.
- **Alpha dependency.** merman is `0.8.0-alpha.6`, so pin exactly and upgrade on purpose.
- **Big diagrams.** Around 150+ nodes, merman layout may fail or take long. It runs off-thread, and failures render as an error box.
- **Scale factor goes stale.** Images keep the scale from their first raster. Moving the window to a different-DPI display leaves them blurry or oversized until they are evicted.
- **Coverage gaps.** Diagram types or layouts merman doesn't support show the error box, not a diagram.

## Decision

| Question | Choice | Why |
|---|---|---|
| Renderer | **merman, in-process** | Pure Rust, MIT/Apache, no JS runtime or network. Most diagrams take 5–14 ms. |
| Hook | **gpui-kit `MarkdownPlugin`** on a `Desktop`-owned `Entity<TextViewState>` | No markdown splitting. Selection, scrolling and other code blocks stay stock. |
| Rasterizing | **Two-stage lazy:** background `Tree`, raster on first paint at `window.scale_factor()`, long side ≤ 8192 px | Exact layout before pixels exist. Memory stays ~60–80 MB instead of ~400 MB, and scrolling is unchanged. |
| Memory cap | **64 MB LRU, 250 ms in-use guard, `drop_image` on evict** | Covers a screenful plus scroll-back, and never evicts what is on screen. |
| Font | **Geist from `theme::FONTS`** plus system fallback | Matches the UI. The one-time fontdb load happens off-thread. |
