use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextViewState, markdown_ast as mdast};
use gpui_kit::*;
use resvg::usvg::{Options, Tree};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use theme::*;
use hud::Act;
use viewport::Viewport;

mod hud;
mod pele;
mod popup;
mod source;
mod viewport;

const BUDGET: usize = 64 << 20;
const MAX_SIDE: f32 = 8192.;
// Diagrams painted this recently are on screen, so eviction spares them.
const IN_USE: Duration = Duration::from_millis(250);
const FADE: Duration = Duration::from_millis(200);
/// Between a card's edges and its diagram, fitted.
const PAD: f32 = 16.;
/// A card's tallest, unless 70% of the window is less; a taller diagram shrinks to fit.
const MAX_HEIGHT: f32 = 720.;

static USVG: LazyLock<Options<'static>> = LazyLock::new(|| {
    let mut options = Options::default();
    let db = Arc::make_mut(&mut options.fontdb);
    db.load_system_fonts();
    for font in theme::FONTS {
        db.load_font_data(font.to_vec());
    }
    options
});

/// Mermaid source as SVG that resvg can draw, on `canvas`.
fn svg(source: &str, canvas: Token, dark: bool) -> Result<String, String> {
    pele::render(source).map(|svg| pele::resolve(&svg, canvas.pick(dark), dark))
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
    /// Cards panned or zoomed off their fit, by place: two cards of one diagram pan apart.
    views: HashMap<u64, Viewport>,
    /// Each card's size when last painted, by place, which sizes its image.
    cards: HashMap<u64, (f32, f32)>,
    /// The card being dragged, by place, and where the pointer last was.
    drag: Option<(u64, Point<Pixels>)>,
    /// The card under the pointer, by place, which shows its zoom buttons.
    hovered: Option<u64>,
    /// Cards showing their source instead, by place.
    sources: HashSet<u64>,
}

/// A card changed height, which a list measuring its rows can't see.
pub struct Resized;

impl EventEmitter<Resized> for Diagrams {}

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
            hovered: None,
            sources: HashSet::new(),
        }
    }

    fn view(&self, place: u64) -> Viewport {
        self.views.get(&place).copied().unwrap_or_default()
    }

    fn set_view(&mut self, place: u64, view: Viewport, cx: &mut Context<Self>) {
        if view == self.view(place) {
            return;
        }
        if view == Viewport::default() {
            self.views.remove(&place);
        } else {
            self.views.insert(place, view);
        }
        cx.notify();
    }

    /// Whether the card at `place` shows its zoom buttons: under the pointer or being dragged.
    fn shows_hud(&self, place: u64) -> bool {
        self.hovered == Some(place) || self.drag.is_some_and(|(p, _)| p == place)
    }

    fn hover(&mut self, place: u64, hovered: bool, cx: &mut Context<Self>) {
        let now = if hovered { Some(place) } else { self.hovered.filter(|p| *p != place) };
        if now != self.hovered {
            self.hovered = now;
            cx.notify();
        }
    }

    fn toggle_source(&mut self, place: u64, cx: &mut Context<Self>) {
        if !self.sources.remove(&place) {
            self.sources.insert(place);
        }
        // The card's buttons go with it; the pointer brings them back.
        self.hovered = None;
        self.relayout(cx);
        cx.notify();
    }

    fn relayout(&self, cx: &mut Context<Self>) {
        if let Some(md) = &self.md {
            md.update(cx, |md, cx| md.invalidate_inline_layout(cx)).ok();
        }
        cx.emit(Resized);
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
                this.drawn.retain(|key, _| this.images.contains_key(key));
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

/// Where a card shows its diagram, so each card pans and zooms on its own.
pub(crate) fn place(at: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    at.hash(&mut hasher);
    hasher.finish()
}

/// What the fullscreen button opens: the diagram's source and the heading above it, which titles the window.
struct Fullscreen {
    source: SharedString,
    heading: Option<SharedString>,
}

/// Mermaid `source` drawn in the current scheme once laid out, in the card at `place`; its error and source if it doesn't parse.
/// `heading` finds the nearest heading above it, asked only while its buttons show.
pub(crate) fn diagram(diagrams: &Entity<Diagrams>, key: u64, place: u64, source: &SharedString, heading: impl FnOnce() -> Option<SharedString>, window: &Window, cx: &mut App) -> Stateful<Div> {
    if diagrams.read(cx).sources.contains(&place) {
        return div().id(SharedString::from(format!("mermaid-{place}"))).child(source::block(diagrams, place, source, cx));
    }
    let dark = theme::is_dark();
    let frame = div().id(SharedString::from(format!("mermaid-{place}"))).rounded(px(15.)).border(px(0.5)).border_color(SEPARATOR).bg(diagrams.read(cx).canvas);
    match diagrams.update(cx, |d, cx| d.get(key, source, dark, cx)) {
        Diagram::Ready(tree) => {
            let size = tree.size();
            let natural = (size.width(), size.height());
            let tallest = (f32::from(window.viewport_size().height) * 0.7).min(MAX_HEIGHT).min(natural.1 + 2. * PAD);
            // The card spans the column, its height following its width by the diagram's shape; the width is known
            // once painted, before which the layout's aspect ratio stands in. The 1 is the border.
            let frame = frame.relative().w_full().overflow_hidden();
            let frame = match diagrams.read(cx).cards.get(&place) {
                Some((width, _)) => frame.h(px(card_height(*width, natural, tallest) + 1.)),
                None => frame.aspect_ratio(natural.0 / natural.1).max_h(px(tallest + 1.)),
            };
            let heading = diagrams.read(cx).shows_hud(place).then(heading).flatten();
            frame.child(surface(diagrams, key, place, tree, PAD, Some(Fullscreen { source: source.clone(), heading }), cx))
        }
        Diagram::Pending => frame.p(px(PAD)).text_size(px(12.)).text_color(TEXT_3).child("Rendering diagram…"),
        Diagram::Failed(error) => frame
            .p(px(PAD))
            .flex()
            .flex_col()
            .gap(px(6.))
            .text_size(px(12.))
            .child(div().text_color(FAILED).child(error))
            .child(div().font_family(MONO).text_color(TEXT_2).child(source.clone())),
    }
}

/// A card `width` wide fitting a `natural`-sized diagram [`PAD`] inside it by width, up to `tallest`.
fn card_height(width: f32, natural: (f32, f32), tallest: f32) -> f32 {
    ((width - 2. * PAD).max(0.) * natural.1 / natural.0 + 2. * PAD).min(tallest)
}

/// The diagram filling its parent, fitted `pad` inside its edges, with zoom buttons over it while the pointer is, and
/// with `fullscreen` one opening it in a window of its own.
fn surface(diagrams: &Entity<Diagrams>, key: u64, place: u64, tree: Arc<Tree>, pad: f32, fullscreen: Option<Fullscreen>, cx: &mut App) -> Stateful<Div> {
    let size = tree.size();
    let natural = (size.width(), size.height());
    let d = diagrams.read(cx);
    let image = d.images.get(&key).map(|(image, _)| image.clone());
    // Fresh for twice the fade, so dropping the animation can't cut it short; cached images scrolled back into view don't fade.
    let fresh = d.rastered.get(&key).is_some_and(|t| t.elapsed() < FADE * 2);
    let view = d.view(place);
    let placed = d.cards.get(&place).map(|card| view.placed(natural, *card, pad));
    let grabbing = d.drag.is_some_and(|(p, _)| p == place);
    let hovered = d.shows_hud(place);
    let canvas_color = d.canvas;
    let input = {
        let diagrams = diagrams.clone();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, cx| {
                let card = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                diagrams.update(cx, |d, cx| {
                    let was = d.cards.insert(place, card);
                    if was != Some(card) {
                        if let (Some(was), Some(view)) = (was, d.views.get_mut(&place)) {
                            *view = view.resized(was, card);
                        }
                        // The card's height follows its width, so the markdown around it lays out again.
                        d.relayout(cx);
                        cx.notify();
                    }
                    let zoom = d.view(place).placed(natural, card, pad).zoom;
                    d.painted(key, &tree, zoom, window, cx);
                });
                window.set_cursor_style(if grabbing { CursorStyle::ClosedHand } else { CursorStyle::OpenHand }, &hitbox);
                listen(&diagrams, place, bounds, hitbox, window);
            },
        )
    };
    // Sized outright: an image left a side to fill takes its raster's pixels, twice the diagram on a 2x display.
    let image = image.zip(placed).map(|(image, p)| {
        let image = img(ImageSource::Render(image)).absolute().left(px(p.origin.0)).top(px(p.origin.1)).w(px(natural.0 * p.zoom)).h(px(natural.1 * p.zoom));
        match fresh {
            true => image.with_animation(("mermaid-in", place), Animation::new(FADE).with_easing(ease_out_quint()), |i, t| i.opacity(t)).into_any_element(),
            false => image.into_any_element(),
        }
    });
    let hud = hovered.then(|| {
        let d = diagrams.clone();
        hud::hud(place, view.level(), fullscreen.is_some(), move |act, window, cx| match act {
            Act::Zoom(k) => d.update(cx, |d, cx| {
                let card = d.cards.get(&place).copied().unwrap_or_default();
                let view = d.view(place).zoomed(k, (card.0 / 2., card.1 / 2.));
                d.set_view(place, view, cx);
            }),
            Act::Fit => d.update(cx, |d, cx| d.set_view(place, Viewport::default(), cx)),
            Act::Source => d.update(cx, |d, cx| d.toggle_source(place, cx)),
            Act::Fullscreen => {
                if let Some(f) = &fullscreen {
                    popup::open(f.source.clone(), canvas_color, f.heading.clone(), window, cx);
                }
            }
        })
    });
    let d = diagrams.clone();
    div()
        .id(("mermaid-surface", place))
        .relative()
        .size_full()
        .overflow_hidden()
        .on_hover(move |hovered, _, cx| d.update(cx, |d, cx| d.hover(place, *hovered, cx)))
        .child(input.absolute().size_full())
        .children(image)
        .children(hud)
}

/// Drag pans, pinch or ⌘-scroll zooms around the pointer, a double-click zooms in or fits it again. A plain scroll,
/// sideways too, goes on to the page.
fn listen(diagrams: &Entity<Diagrams>, place: u64, bounds: Bounds<Pixels>, hitbox: Hitbox, window: &mut Window) {
    let at = move |p: Point<Pixels>| (f32::from(p.x - bounds.origin.x), f32::from(p.y - bounds.origin.y));
    let change = move |diagrams: &Entity<Diagrams>, cx: &mut App, f: &dyn Fn(Viewport) -> Viewport| {
        diagrams.update(cx, |d, cx| {
            let view = f(d.view(place));
            d.set_view(place, view, cx);
        })
    };
    let (d, hit) = (diagrams.clone(), hitbox.clone());
    window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !hit.is_hovered(window) {
            return;
        }
        if e.click_count == 2 {
            let point = at(e.position);
            change(&d, cx, &|v| v.double_clicked(point));
        } else {
            d.update(cx, |d, cx| {
                d.drag = Some((place, e.position));
                cx.notify();
            });
        }
        // Not a text selection starting.
        cx.stop_propagation();
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
        let Some((dragged, last)) = d.read(cx).drag.filter(|(p, _)| phase == DispatchPhase::Bubble && *p == place) else { return };
        if e.pressed_button != Some(MouseButton::Left) {
            d.update(cx, |d, cx| {
                d.drag = None;
                cx.notify();
            });
            return;
        }
        let by = (f32::from(e.position.x - last.x), f32::from(e.position.y - last.y));
        d.update(cx, |d, _| d.drag = Some((dragged, e.position)));
        change(&d, cx, &|v| v.panned(by));
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble && d.read(cx).drag.is_some_and(|(p, _)| p == place) {
            d.update(cx, |d, cx| {
                d.drag = None;
                cx.notify();
            });
        }
    });
    let (d, hit) = (diagrams.clone(), hitbox.clone());
    window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !hit.is_hovered(window) || !(e.modifiers.platform || e.modifiers.control) {
            return;
        }
        let (point, dy) = (at(e.position), f32::from(e.delta.pixel_delta(px(20.)).y));
        change(&d, cx, &|v| v.zoomed((dy * 0.01).exp(), point));
        cx.stop_propagation();
    });
    let d = diagrams.clone();
    window.on_mouse_event(move |e: &PinchEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
            let point = at(e.position);
            change(&d, cx, &|v| v.zoomed(1. + e.delta, point));
            cx.stop_propagation();
        }
    });
}

/// The nearest heading above byte `at` of `markdown`.
pub(crate) fn heading_above(markdown: &str, at: usize) -> Option<SharedString> {
    markdown.get(..at).and_then(heading).map(|h| h.to_string().into())
}

/// The last ATX heading's text in `markdown`, skipping fenced code.
fn heading(markdown: &str) -> Option<&str> {
    let mut fence: Option<&str> = None;
    let mut last = None;
    for line in markdown.lines().map(str::trim_start) {
        let marker = ["```", "~~~"].into_iter().find(|m| line.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) => fence = Some(m),
            (Some(open), Some(m)) if open == m => fence = None,
            (None, None) => {
                let text = line.trim_start_matches('#');
                let level = line.len() - text.len();
                if (1..=6).contains(&level) && (text.is_empty() || text.starts_with([' ', '\t'])) {
                    last = Some(text.trim().trim_end_matches('#').trim_end()).filter(|t| !t.is_empty());
                }
            }
            _ => {}
        }
    }
    last
}

struct Fence {
    /// Indexed by `dark`.
    keys: [u64; 2],
    place: u64,
    source: SharedString,
    heading: Option<SharedString>,
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
        let at = code.position.as_ref().map(|p| p.start.offset);
        let heading = at.and_then(|at| heading_above(cx.source(), at));
        let fence = Fence { keys: [key(&code.value, false), key(&code.value, true)], place: place((at, &code.value)), source: code.value.clone().into(), heading };
        Some(MarkdownNode::new("mermaid", fence).text(code.value.clone()).markdown(cx.node_source(node).unwrap_or(&code.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Fence { keys, place, source, heading } = node.data::<Fence>().unwrap();
        diagram(&self.0, keys[theme::is_dark() as usize], *place, source, || heading.clone(), window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{BUDGET, IN_USE, PAD, Viewport, card_height, evict, heading, rasterize, svg, tree};
    use gpui_kit::RenderImage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use theme::{MD_CODE_BG, Token};

    #[test]
    fn a_wide_diagram_fits_its_card_by_width() {
        let natural = (1150., 60.);
        let card = (720., card_height(720., natural, 500.));
        let zoom = Viewport::default().placed(natural, card, PAD).zoom;
        assert!((zoom - (720. - 2. * PAD) / 1150.).abs() < 1e-4, "{zoom}");
        assert_eq!(card_height(720., (100., 2000.), 500.), 500.);
    }

    fn image(w: u32, h: u32) -> Arc<RenderImage> {
        let buffer = image::ImageBuffer::from_raw(w, h, vec![0; (w * h * 4) as usize]).unwrap();
        Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
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
        assert!(svg("flowchart LR\n  A -->\n", MD_CODE_BG, false).unwrap_err().contains("syntax error"));
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
    fn finds_the_last_heading_above_a_diagram() {
        assert_eq!(heading("# Doc\n\nText\n\n## Architecture ##\n\nMore\n"), Some("Architecture"));
        assert_eq!(heading("Text\n#hashtag\n"), None);
        assert_eq!(heading("## Flow\n```sh\n# a comment\n```\n"), Some("Flow"));
        assert_eq!(heading("   ### Indented\n"), Some("Indented"));
        assert_eq!(heading("####### Seven\n"), None);
    }

    #[test]
    fn spares_images_on_screen_even_over_budget() {
        let mut images = HashMap::from([(1, (image(64, 64), Instant::now())), (2, (image(64, 64), Instant::now()))]);
        assert!(evict(&mut images, 0).is_empty());
        assert_eq!(images.len(), 2);
    }
}
