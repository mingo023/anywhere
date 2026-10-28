use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextViewState, markdown_ast as mdast};
use gpui_kit::*;
use merman::svg::{HostTheme, Presentation, ResolvedPresentation, SvgOutputPolicy, SvgPipelinePreset, ThemeRole};
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use resvg::usvg::{Options, Tree};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use theme::*;
use viewport::Viewport;

mod viewport;

const BUDGET: usize = 64 << 20;
const MAX_SIDE: f32 = 8192.;
// Diagrams painted this recently are on screen, so eviction spares them.
const IN_USE: Duration = Duration::from_millis(250);
const FADE: Duration = Duration::from_millis(200);
/// A card's tallest; a taller diagram pans.
const MAX_HEIGHT: f32 = 480.;

struct Merman {
    renderer: Renderer,
    presentation: ResolvedPresentation,
}

type Engines = HashMap<(u32, bool), Arc<Merman>>;

/// By canvas colour and `dark`: each scheme and canvas lays out its own colours.
static MERMAN: LazyLock<Mutex<Engines>> = LazyLock::new(Default::default);

fn merman(canvas: Token, dark: bool) -> Arc<Merman> {
    let mut cache = MERMAN.lock().unwrap();
    cache.entry((canvas.pick(dark), dark)).or_insert_with(|| Arc::new(build(canvas, dark))).clone()
}

fn build(canvas: Token, dark: bool) -> Merman {
    let roles = [
        (ThemeRole::Canvas, canvas),
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
    let flat = |c: u32| hex(c, dark);
    let theme = roles.into_iter().fold(HostTheme::new().try_with_font_family("Geist, sans-serif").unwrap(), |t, (role, c)| t.try_with_role(role, flat(c.pick(dark))).unwrap());
    let presentation = Presentation::new().with_theme(theme.try_with_series_palette(PALETTE.map(flat)).unwrap()).resolve();
    let renderer = Renderer::new().with_engine(presentation.materialize_engine(merman::Engine::new()));
    Merman { renderer, presentation }
}

static USVG: LazyLock<Options<'static>> = LazyLock::new(|| {
    let mut options = Options::default();
    let db = Arc::make_mut(&mut options.fontdb);
    db.load_system_fonts();
    for font in theme::FONTS {
        db.load_font_data(font.to_vec());
    }
    options
});

/// Merman wants opaque colours, so translucent tokens are flattened onto white, or onto the window in dark.
fn hex(c: u32, dark: bool) -> String {
    let [br, bg, bb, _] = if dark { WINDOW_SOLID.pick(true) } else { 0xffffffff }.to_be_bytes();
    let a = (c & 0xff) as f32 / 255.;
    let mix = |v: u32, base: u8| ((v & 0xff) as f32 * a + base as f32 * (1. - a)).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(c >> 24, br), mix(c >> 16, bg), mix(c >> 8, bb))
}

/// Mermaid source as SVG that resvg can draw: labels are plain `<text>`, not `<foreignObject>`.
fn svg(source: &str, canvas: Token, dark: bool) -> Result<String, String> {
    let merman = merman(canvas, dark);
    let pipeline = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, root_background_color: Some(hex(canvas.pick(dark), dark)), ..Default::default() }.pipeline();
    let request = SvgRequest { pipeline: Some(pipeline), presentation: merman.presentation.render_policy(), ..Default::default() };
    match merman.renderer.render(RenderRequest::svg(source, OperationControl::new(), request)) {
        Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_string()),
        Ok(_) => Err("Not a Mermaid diagram".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn tree(source: &str, canvas: Token, dark: bool) -> Result<Arc<Tree>, String> {
    Tree::from_str(&svg(source, canvas, dark)?, &USVG).map(Arc::new).map_err(|e| e.to_string())
}

fn rasterize(tree: &Tree, scale: f32) -> Result<Arc<RenderImage>, String> {
    let size = tree.size();
    let scale = scale.min(MAX_SIDE / size.width().max(size.height()));
    let mut pixmap = resvg::tiny_skia::Pixmap::new((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32).ok_or("Diagram is empty")?;
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut bgra = pixmap.take();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buffer = image::ImageBuffer::from_raw(w, h, bgra).ok_or("Diagram buffer mismatch")?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn bytes(image: &RenderImage) -> usize {
    image.as_bytes(0).map_or(0, <[u8]>::len)
}

fn evict(images: &mut HashMap<u64, (Arc<RenderImage>, Instant)>, budget: usize) -> Vec<Arc<RenderImage>> {
    let mut total: usize = images.values().map(|(i, _)| bytes(i)).sum();
    let mut lru: Vec<(u64, Instant)> = images.iter().map(|(k, (_, t))| (*k, *t)).collect();
    lru.sort_by_key(|(_, t)| *t);
    let mut dropped = Vec::new();
    for (key, painted) in lru {
        if total <= budget || painted.elapsed() < IN_USE {
            break;
        }
        let (image, _) = images.remove(&key).unwrap();
        total -= bytes(&image);
        dropped.push(image);
    }
    dropped
}

#[derive(Clone)]
enum Diagram {
    Pending,
    Ready(Arc<Tree>),
    Failed(SharedString),
}

/// Diagrams keyed by a hash of source and scheme. Layout happens off the UI thread as soon as a fence is seen; pixels only for painted fences, within [`BUDGET`].
pub struct Diagrams {
    /// The markdown view the fences sit in, whose layout changes once a diagram is laid out; none outside one.
    md: Option<WeakEntity<TextViewState>>,
    /// What the diagrams are drawn on, and their card's colour.
    canvas: Token,
    items: HashMap<u64, Diagram>,
    images: HashMap<u64, (Arc<RenderImage>, Instant)>,
    rastered: HashMap<u64, Instant>,
    rastering: HashSet<u64>,
    /// The raster zoom each image was drawn for.
    drawn: HashMap<u64, f32>,
    /// Diagrams panned or zoomed off their fit.
    views: HashMap<u64, Viewport>,
    /// Each card's size when last painted, which sizes its image.
    cards: HashMap<u64, (f32, f32)>,
    /// The diagram being dragged, and where the pointer last was.
    drag: Option<(u64, Point<Pixels>)>,
}

impl Diagrams {
    pub fn new(md: &Entity<TextViewState>) -> Self {
        Self { md: Some(md.downgrade()), ..Self::standalone(MD_CODE_BG) }
    }

    pub fn standalone(canvas: Token) -> Self {
        Self {
            md: None,
            canvas,
            items: HashMap::new(),
            images: HashMap::new(),
            rastered: HashMap::new(),
            rastering: HashSet::new(),
            drawn: HashMap::new(),
            views: HashMap::new(),
            cards: HashMap::new(),
            drag: None,
        }
    }

    /// Back to showing the whole diagram.
    pub(crate) fn fit(&mut self, key: u64, cx: &mut Context<Self>) {
        if self.views.remove(&key).is_some() {
            cx.notify();
        }
    }

    /// Whether the diagram under `key` was laid out and didn't parse.
    pub(crate) fn failed(&self, key: u64) -> bool {
        matches!(self.items.get(&key), Some(Diagram::Failed(_)))
    }

    fn relayout(&self, cx: &mut App) {
        if let Some(md) = &self.md {
            md.update(cx, |md, cx| md.invalidate_inline_layout(cx)).ok();
        }
    }

    fn get(&mut self, key: u64, source: &SharedString, dark: bool, cx: &mut Context<Self>) -> Diagram {
        if let Some(d) = self.items.get(&key) {
            return d.clone();
        }
        self.items.insert(key, Diagram::Pending);
        let (source, canvas) = (source.clone(), self.canvas);
        cx.spawn(async move |this, cx| {
            let diagram = cx.background_spawn(async move { tree(&source, canvas, dark) }).await;
            this.update(cx, |this, cx| {
                this.items.insert(key, diagram.map_or_else(|e| Diagram::Failed(e.into()), Diagram::Ready));
                this.relayout(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        Diagram::Pending
    }

    /// Marks a diagram on screen at `zoom`, rasterizing it when it has no image yet or one drawn for less.
    fn painted(&mut self, key: u64, tree: &Arc<Tree>, zoom: f32, window: &mut Window, cx: &mut Context<Self>) {
        let want = viewport::raster_zoom(zoom);
        if let Some((_, painted)) = self.images.get_mut(&key) {
            *painted = Instant::now();
            if self.drawn.get(&key).is_some_and(|drawn| *drawn >= want) {
                return;
            }
        }
        if !self.rastering.insert(key) {
            return;
        }
        let first = !self.images.contains_key(&key);
        let size = tree.size();
        let (tree, scale) = (tree.clone(), viewport::raster_scale(window.scale_factor(), want, (size.width(), size.height())));
        cx.spawn_in(window, async move |this, cx| {
            let image = cx.background_spawn(async move { rasterize(&tree, scale) }).await;
            this.update_in(cx, |this, window, cx| {
                this.rastering.remove(&key);
                match image {
                    Ok(image) => {
                        let now = Instant::now();
                        if let Some((old, _)) = this.images.insert(key, (image, now)) {
                            window.drop_image(old).ok();
                        }
                        this.drawn.insert(key, want);
                        // A sharper redraw of a zoomed diagram swaps in without fading.
                        if first {
                            this.rastered.retain(|_, t| t.elapsed() < FADE * 2);
                            this.rastered.insert(key, now);
                        }
                    }
                    Err(e) => {
                        this.items.insert(key, Diagram::Failed(e.into()));
                        this.relayout(cx);
                    }
                }
                for image in evict(&mut this.images, BUDGET) {
                    window.drop_image(image).ok();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// A diagram's cache key: each scheme lays out its own colours.
pub(crate) fn key(source: &str, dark: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    (source, dark).hash(&mut hasher);
    hasher.finish()
}

/// Mermaid `source` drawn in the current scheme once laid out; its error and source if it doesn't parse.
pub(crate) fn diagram(diagrams: &Entity<Diagrams>, key: u64, source: &SharedString, cx: &mut App) -> Stateful<Div> {
    let dark = theme::is_dark();
    let frame = div().id(SharedString::from(format!("mermaid-{key}"))).p(px(16.)).rounded(px(15.)).bg(diagrams.read(cx).canvas);
    match diagrams.update(cx, |d, cx| d.get(key, source, dark, cx)) {
        Diagram::Ready(tree) => {
            let size = tree.size();
            let natural = (size.width(), size.height());
            let d = diagrams.read(cx);
            let image = d.images.get(&key).map(|(image, _)| image.clone());
            // Fresh for twice the fade, so dropping the animation can't cut it short; cached images scrolled back into view don't fade.
            let fresh = d.rastered.get(&key).is_some_and(|t| t.elapsed() < FADE * 2);
            let view = d.views.get(&key).copied();
            let shown = d.cards.get(&key).map(|card| view.unwrap_or_else(|| Viewport::fit(natural, *card)));
            let grabbing = d.drag.is_some_and(|(k, _)| k == key);
            let diagrams = diagrams.clone();
            let input = canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |bounds, hitbox, window, cx| {
                    let card = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let shown = view.unwrap_or_else(|| Viewport::fit(natural, card));
                    diagrams.update(cx, |d, cx| {
                        if d.cards.insert(key, card) != Some(card) {
                            cx.notify();
                        }
                        d.painted(key, &tree, shown.zoom, window, cx);
                    });
                    let (x, y) = shown.overflow(natural, card);
                    if grabbing || x + y > 0.5 {
                        window.set_cursor_style(if grabbing { CursorStyle::ClosedHand } else { CursorStyle::OpenHand }, &hitbox);
                    }
                    listen(&diagrams, key, natural, bounds, hitbox, window);
                },
            );
            // Its natural size until wider than the card, then the card's width; never taller than MAX_HEIGHT. Its height
            // follows its laid-out width once known, as the layout takes an aspect ratio from the width before `max_w` caps it.
            let card = div().relative().overflow_hidden().w_full().max_w(px(natural.0)).max_h(px(MAX_HEIGHT));
            let card = match d.cards.get(&key) {
                Some((width, _)) => card.h(px(width * natural.1 / natural.0)),
                None => card.aspect_ratio(natural.0 / natural.1),
            };
            // Sized outright: an image left a side to fill takes its raster's pixels, twice the diagram on a 2x display.
            frame.child(card.child(input.absolute().size_full()).children(image.zip(shown).map(|(image, v)| {
                let image = img(ImageSource::Render(image)).absolute().left(px(-v.offset.0)).top(px(-v.offset.1)).w(px(natural.0 * v.zoom)).h(px(natural.1 * v.zoom));
                match fresh {
                    true => image.with_animation(("mermaid-in", key), Animation::new(FADE).with_easing(ease_out_quint()), |i, t| i.opacity(t)).into_any_element(),
                    false => image.into_any_element(),
                }
            })))
        }
        Diagram::Pending => frame.text_size(px(12.)).text_color(TEXT_3).child("Rendering diagram…"),
        Diagram::Failed(error) => frame
            .flex()
            .flex_col()
            .gap(px(6.))
            .text_size(px(12.))
            .child(div().text_color(FAILED).child(error))
            .child(div().font_family(MONO).text_color(TEXT_2).child(source.clone())),
    }
}

/// Drag pans, pinch or ⌘-scroll zooms around the pointer, a sideways swipe pans, a double-click fits it again.
/// A plain scroll goes on to the page.
fn listen(diagrams: &Entity<Diagrams>, key: u64, natural: (f32, f32), bounds: Bounds<Pixels>, hitbox: Hitbox, window: &mut Window) {
    let card = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let at = move |p: Point<Pixels>| (f32::from(p.x - bounds.origin.x), f32::from(p.y - bounds.origin.y));
    let change = move |diagrams: &Entity<Diagrams>, cx: &mut App, f: &dyn Fn(Viewport) -> Viewport| {
        diagrams.update(cx, |d, cx| {
            let view = d.views.get(&key).copied().unwrap_or_else(|| Viewport::fit(natural, card));
            d.views.insert(key, f(view));
            cx.notify();
        })
    };
    let (d, hit) = (diagrams.clone(), hitbox.clone());
    window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !hit.is_hovered(window) {
            return;
        }
        d.update(cx, |d, cx| {
            if e.click_count == 2 {
                d.views.remove(&key);
            } else {
                d.drag = Some((key, e.position));
            }
            cx.notify();
        });
        // Not a text selection starting.
        cx.stop_propagation();
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
        let Some((dragged, last)) = d.read(cx).drag.filter(|(k, _)| phase == DispatchPhase::Bubble && *k == key) else { return };
        if e.pressed_button != Some(MouseButton::Left) {
            d.update(cx, |d, cx| {
                d.drag = None;
                cx.notify();
            });
            return;
        }
        let by = (f32::from(e.position.x - last.x), f32::from(e.position.y - last.y));
        d.update(cx, |d, _| d.drag = Some((dragged, e.position)));
        change(&d, cx, &|v| v.panned(by, natural, card));
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble && d.read(cx).drag.is_some_and(|(k, _)| k == key) {
            d.update(cx, |d, cx| {
                d.drag = None;
                cx.notify();
            });
        }
    });
    let (d, hit) = (diagrams.clone(), hitbox.clone());
    window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !hit.is_hovered(window) {
            return;
        }
        let delta = e.delta.pixel_delta(px(20.));
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        if e.modifiers.platform {
            let point = at(e.position);
            change(&d, cx, &|v| v.zoomed((dy / 200.).exp(), point, natural, card));
        } else if dx.abs() > dy.abs() {
            change(&d, cx, &|v| v.panned((dx, 0.), natural, card));
        } else {
            return;
        }
        cx.stop_propagation();
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |e: &PinchEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
            let point = at(e.position);
            change(&d, cx, &|v| v.zoomed(1. + e.delta, point, natural, card));
            cx.stop_propagation();
        }
    });
}

struct Fence {
    /// Indexed by `dark`.
    keys: [u64; 2],
    source: SharedString,
}

/// Renders `mermaid` code fences in a markdown `TextView` as diagrams.
pub struct Mermaid(pub Entity<Diagrams>);

impl MarkdownPlugin for Mermaid {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "mermaid"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Code(code) = node else { return None };
        if code.lang.as_deref() != Some("mermaid") {
            return None;
        }
        let fence = Fence { keys: [key(&code.value, false), key(&code.value, true)], source: code.value.clone().into() };
        Some(MarkdownNode::new("mermaid", fence).text(code.value.clone()).markdown(cx.node_source(node).unwrap_or(&code.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Fence { keys, source } = node.data::<Fence>().unwrap();
        diagram(&self.0, keys[theme::is_dark() as usize], source, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{BUDGET, IN_USE, evict, hex, rasterize, svg, tree};
    use gpui_kit::RenderImage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use theme::{MD_CODE_BG, Token};

    fn image(w: u32, h: u32) -> Arc<RenderImage> {
        let buffer = image::ImageBuffer::from_raw(w, h, vec![0; (w * h * 4) as usize]).unwrap();
        Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
    }

    #[test]
    fn flattens_translucent_tokens_onto_the_canvas() {
        assert_eq!(hex(0x5b5bd6ff, false), "#5b5bd6");
        assert_eq!(hex(0x00000000, false), "#ffffff");
        assert_eq!(hex(0x00000080, false), "#7f7f7f");
        assert_eq!(hex(0x00000000, true), "#171717");
        assert_eq!(hex(0xebebeb1a, true), "#2d2d2d");
    }

    #[test]
    fn renders_resvg_safe_svg_with_text_labels() {
        let out = svg("flowchart LR\n  A[Open] --> B[Close]\n", MD_CODE_BG, false).unwrap();
        assert!(out.starts_with("<svg"));
        assert!(out.contains(">Open<"));
        assert!(!out.contains("<foreignObject"));
    }

    #[test]
    fn reports_syntax_errors() {
        assert!(svg("flowchart LR\n  A -->\n", MD_CODE_BG, false).unwrap_err().contains("parse error"));
    }

    #[test]
    fn rasterizes_at_scale_and_caps_the_long_side() {
        let t = tree("flowchart LR\n  A --> B\n", MD_CODE_BG, false).unwrap();
        let (w, h) = (t.size().width(), t.size().height());
        let size = rasterize(&t, 2.).unwrap().size(0);
        assert_eq!((size.width.0, size.height.0), ((w * 2.).ceil() as i32, (h * 2.).ceil() as i32));
        let size = rasterize(&t, 10_000.).unwrap().size(0);
        assert_eq!(size.width.0.max(size.height.0), 8192);
    }

    #[test]
    fn paints_the_canvas_in_each_schemes_code_block_colour() {
        for dark in [false, true] {
            let t = tree("flowchart LR\n  A --> B\n", MD_CODE_BG, dark).unwrap();
            let [r, g, b, a] = MD_CODE_BG.pick(dark).to_be_bytes();
            assert_eq!(rasterize(&t, 1.).unwrap().as_bytes(0).unwrap()[..4], [b, g, r, a]);
        }
    }

    #[test]
    fn paints_the_canvas_it_is_given() {
        let canvas = Token::new(0xf4f4f5ff, 0x262626ff);
        let t = tree("flowchart LR\n  A --> B\n", canvas, false).unwrap();
        assert_eq!(rasterize(&t, 1.).unwrap().as_bytes(0).unwrap()[..4], [0xf5, 0xf4, 0xf4, 0xff]);
    }

    #[test]
    fn evicts_least_recently_painted_past_the_budget() {
        let old = Instant::now() - IN_USE * 4;
        let big = (BUDGET / 4 / 4096) as u32;
        let mut images = HashMap::from([
            (1, (image(1024, big), old)),
            (2, (image(1024, big), old + IN_USE)),
            (3, (image(1024, big), old + IN_USE * 2)),
            (4, (image(1024, big), Instant::now())),
            (5, (image(1024, big), Instant::now())),
        ]);
        assert_eq!(evict(&mut images, BUDGET).len(), 1);
        assert!(!images.contains_key(&1) && images.contains_key(&2));
    }

    #[test]
    fn spares_images_on_screen_even_over_budget() {
        let mut images = HashMap::from([(1, (image(64, 64), Instant::now())), (2, (image(64, 64), Instant::now()))]);
        assert!(evict(&mut images, 0).is_empty());
        assert_eq!(images.len(), 2);
    }
}
