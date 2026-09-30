use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextViewState, markdown_ast as mdast};
use gpui_kit::*;
use merman::svg::{HostTheme, Presentation, ResolvedPresentation, SvgOutputPolicy, SvgPipelinePreset, ThemeRole};
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use resvg::usvg::{Options, Tree};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use theme::*;

const BUDGET: usize = 64 << 20;
const MAX_SIDE: f32 = 8192.;
// Diagrams painted this recently are on screen, so eviction spares them.
const IN_USE: Duration = Duration::from_millis(250);
const FADE: Duration = Duration::from_millis(200);

struct Merman {
    renderer: Renderer,
    presentation: ResolvedPresentation,
}

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
fn svg(source: &str, dark: bool) -> Result<String, String> {
    let merman = &*MERMAN[dark as usize];
    let pipeline = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, root_background_color: Some(hex(WINDOW_SOLID.pick(dark), dark)), ..Default::default() }.pipeline();
    let request = SvgRequest { pipeline: Some(pipeline), presentation: merman.presentation.render_policy(), ..Default::default() };
    match merman.renderer.render(RenderRequest::svg(source, OperationControl::new(), request)) {
        Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_string()),
        Ok(_) => Err("Not a Mermaid diagram".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn tree(source: &str, dark: bool) -> Result<Arc<Tree>, String> {
    Tree::from_str(&svg(source, dark)?, &USVG).map(Arc::new).map_err(|e| e.to_string())
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
    md: WeakEntity<TextViewState>,
    items: HashMap<u64, Diagram>,
    images: HashMap<u64, (Arc<RenderImage>, Instant)>,
    rastered: HashMap<u64, Instant>,
    rastering: HashSet<u64>,
}

impl Diagrams {
    pub fn new(md: &Entity<TextViewState>) -> Self {
        Self { md: md.downgrade(), items: HashMap::new(), images: HashMap::new(), rastered: HashMap::new(), rastering: HashSet::new() }
    }

    fn get(&mut self, key: u64, source: &SharedString, dark: bool, cx: &mut Context<Self>) -> Diagram {
        if let Some(d) = self.items.get(&key) {
            return d.clone();
        }
        self.items.insert(key, Diagram::Pending);
        let source = source.clone();
        cx.spawn(async move |this, cx| {
            let diagram = cx.background_spawn(async move { tree(&source, dark) }).await;
            this.update(cx, |this, cx| {
                this.items.insert(key, diagram.map_or_else(|e| Diagram::Failed(e.into()), Diagram::Ready));
                this.md.update(cx, |md, cx| md.invalidate_inline_layout(cx)).ok();
                cx.notify();
            })
            .ok();
        })
        .detach();
        Diagram::Pending
    }

    fn painted(&mut self, key: u64, tree: &Arc<Tree>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, painted)) = self.images.get_mut(&key) {
            *painted = Instant::now();
            return;
        }
        if !self.rastering.insert(key) {
            return;
        }
        let (tree, scale) = (tree.clone(), window.scale_factor());
        cx.spawn_in(window, async move |this, cx| {
            let image = cx.background_spawn(async move { rasterize(&tree, scale) }).await;
            this.update_in(cx, |this, window, cx| {
                this.rastering.remove(&key);
                match image {
                    Ok(image) => {
                        let now = Instant::now();
                        this.images.insert(key, (image, now));
                        this.rastered.retain(|_, t| t.elapsed() < FADE * 2);
                        this.rastered.insert(key, now);
                    }
                    Err(e) => {
                        this.items.insert(key, Diagram::Failed(e.into()));
                        this.md.update(cx, |md, cx| md.invalidate_inline_layout(cx)).ok();
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

struct Fence {
    /// Indexed by `dark`, since each scheme lays out its own colours.
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
        let key = |dark: bool| {
            let mut hasher = DefaultHasher::new();
            (&code.value, dark).hash(&mut hasher);
            hasher.finish()
        };
        let fence = Fence { keys: [key(false), key(true)], source: code.value.clone().into() };
        Some(MarkdownNode::new("mermaid", fence).text(code.value.clone()).markdown(cx.node_source(node).unwrap_or(&code.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Fence { keys, source } = node.data::<Fence>().unwrap();
        let dark = theme::is_dark();
        let key = keys[dark as usize];
        let frame = div().id(SharedString::from(format!("mermaid-{key}"))).my(px(8.)).p(px(12.)).rounded(px(10.)).border_1().border_color(SEPARATOR).bg(WINDOW_SOLID);
        match self.0.update(cx, |d, cx| d.get(key, source, dark, cx)) {
            Diagram::Ready(tree) => {
                let size = tree.size();
                let d = self.0.read(cx);
                let image = d.images.get(&key).map(|(image, _)| image.clone());
                // Fresh for twice the fade, so dropping the animation can't cut it short; cached images scrolled back into view don't fade.
                let fresh = d.rastered.get(&key).is_some_and(|t| t.elapsed() < FADE * 2);
                let diagrams = self.0.clone();
                let paint = canvas(|_, _, _| {}, move |_, _, window, cx| diagrams.update(cx, |d, cx| d.painted(key, &tree, window, cx)));
                frame.overflow_x_scroll().child(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(size.width()))
                        .h(px(size.height()))
                        .child(paint.absolute().size_full())
                        .children(image.map(|image| {
                            let image = img(ImageSource::Render(image)).size_full();
                            match fresh {
                                true => image.with_animation(("mermaid-in", key), Animation::new(FADE).with_easing(ease_out_quint()), |i, t| i.opacity(t)).into_any_element(),
                                false => image.into_any_element(),
                            }
                        })),
                )
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
}

#[cfg(test)]
mod tests {
    use super::{BUDGET, IN_USE, evict, hex, rasterize, svg, tree};
    use gpui_kit::RenderImage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use theme::WINDOW_SOLID;

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
        let out = svg("flowchart LR\n  A[Open] --> B[Close]\n", false).unwrap();
        assert!(out.starts_with("<svg"));
        assert!(out.contains(">Open<"));
        assert!(!out.contains("<foreignObject"));
    }

    #[test]
    fn reports_syntax_errors() {
        assert!(svg("flowchart LR\n  A -->\n", false).unwrap_err().contains("parse error"));
    }

    #[test]
    fn rasterizes_at_scale_and_caps_the_long_side() {
        let t = tree("flowchart LR\n  A --> B\n", false).unwrap();
        let (w, h) = (t.size().width(), t.size().height());
        let size = rasterize(&t, 2.).unwrap().size(0);
        assert_eq!((size.width.0, size.height.0), ((w * 2.).ceil() as i32, (h * 2.).ceil() as i32));
        let size = rasterize(&t, 10_000.).unwrap().size(0);
        assert_eq!(size.width.0.max(size.height.0), 8192);
    }

    #[test]
    fn paints_the_canvas_in_each_schemes_window_colour() {
        for dark in [false, true] {
            let t = tree("flowchart LR\n  A --> B\n", dark).unwrap();
            let [r, g, b, a] = WINDOW_SOLID.pick(dark).to_be_bytes();
            assert_eq!(rasterize(&t, 1.).unwrap().as_bytes(0).unwrap()[..4], [b, g, r, a]);
        }
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
