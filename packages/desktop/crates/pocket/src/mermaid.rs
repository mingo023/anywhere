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

struct Merman {
    renderer: Renderer,
    presentation: ResolvedPresentation,
}

static MERMAN: LazyLock<Merman> = LazyLock::new(|| {
    let roles = [
        (ThemeRole::Canvas, SURFACE),
        (ThemeRole::Surface, SURFACE_SUNKEN),
        (ThemeRole::SurfaceAlt, WINDOW),
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
    let theme = roles.into_iter().fold(HostTheme::new().try_with_font_family("Geist, sans-serif").unwrap(), |t, (role, c)| t.try_with_role(role, hex(c)).unwrap());
    let presentation = Presentation::new().with_theme(theme.try_with_series_palette(PALETTE.map(hex)).unwrap()).resolve();
    let renderer = Renderer::new().with_engine(presentation.materialize_engine(merman::Engine::new()));
    Merman { renderer, presentation }
});

static USVG: LazyLock<Options<'static>> = LazyLock::new(|| {
    let mut options = Options::default();
    let db = Arc::make_mut(&mut options.fontdb);
    db.load_system_fonts();
    for font in theme::FONTS {
        db.load_font_data(font.to_vec());
    }
    options
});

/// Merman wants opaque colours, so translucent tokens are flattened onto white.
fn hex(c: u32) -> String {
    let a = (c & 0xff) as f32 / 255.;
    let mix = |v: u32| ((v & 0xff) as f32 * a + 255. * (1. - a)).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(c >> 24), mix(c >> 16), mix(c >> 8))
}

/// Mermaid source as SVG that resvg can draw: labels are plain `<text>`, not `<foreignObject>`.
fn svg(source: &str) -> Result<String, String> {
    let pipeline = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, ..Default::default() }.pipeline();
    let request = SvgRequest { pipeline: Some(pipeline), presentation: MERMAN.presentation.render_policy(), ..Default::default() };
    match MERMAN.renderer.render(RenderRequest::svg(source, OperationControl::new(), request)) {
        Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_string()),
        Ok(_) => Err("Not a Mermaid diagram".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn tree(source: &str) -> Result<Arc<Tree>, String> {
    Tree::from_str(&svg(source)?, &USVG).map(Arc::new).map_err(|e| e.to_string())
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

/// Diagrams keyed by source hash. Layout happens off the UI thread as soon as a fence is seen; pixels only for painted fences, within [`BUDGET`].
pub struct Diagrams {
    md: WeakEntity<TextViewState>,
    items: HashMap<u64, Diagram>,
    images: HashMap<u64, (Arc<RenderImage>, Instant)>,
    rastering: HashSet<u64>,
}

impl Diagrams {
    pub fn new(md: &Entity<TextViewState>) -> Self {
        Self { md: md.downgrade(), items: HashMap::new(), images: HashMap::new(), rastering: HashSet::new() }
    }

    fn get(&mut self, key: u64, source: &SharedString, cx: &mut Context<Self>) -> Diagram {
        if let Some(d) = self.items.get(&key) {
            return d.clone();
        }
        self.items.insert(key, Diagram::Pending);
        let source = source.clone();
        cx.spawn(async move |this, cx| {
            let diagram = cx.background_spawn(async move { tree(&source) }).await;
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
                        this.images.insert(key, (image, Instant::now()));
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
    key: u64,
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
        let mut hasher = DefaultHasher::new();
        code.value.hash(&mut hasher);
        let fence = Fence { key: hasher.finish(), source: code.value.clone().into() };
        Some(MarkdownNode::new("mermaid", fence).text(code.value.clone()).markdown(cx.node_source(node).unwrap_or(&code.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Fence { key, source } = node.data::<Fence>().unwrap();
        let key = *key;
        let frame = div().id(SharedString::from(format!("mermaid-{key}"))).my(px(8.)).p(px(12.)).rounded(px(8.)).border_1().border_color(rgba(HAIRLINE)).bg(rgba(SURFACE));
        match self.0.update(cx, |d, cx| d.get(key, source, cx)) {
            Diagram::Ready(tree) => {
                let size = tree.size();
                let image = self.0.read(cx).images.get(&key).map(|(image, _)| image.clone());
                let diagrams = self.0.clone();
                let paint = canvas(|_, _, _| {}, move |_, _, window, cx| diagrams.update(cx, |d, cx| d.painted(key, &tree, window, cx)));
                frame.overflow_x_scroll().child(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(size.width()))
                        .h(px(size.height()))
                        .child(paint.absolute().size_full())
                        .children(image.map(|image| img(ImageSource::Render(image)).size_full())),
                )
            }
            Diagram::Pending => frame.text_size(px(12.)).text_color(rgba(TEXT_3)).child("Rendering diagram…"),
            Diagram::Failed(error) => frame
                .flex()
                .flex_col()
                .gap(px(6.))
                .text_size(px(12.))
                .child(div().text_color(rgba(FAILED)).child(error))
                .child(div().font_family(MONO).text_color(rgba(TEXT_2)).child(source.clone())),
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

    fn image(w: u32, h: u32) -> Arc<RenderImage> {
        let buffer = image::ImageBuffer::from_raw(w, h, vec![0; (w * h * 4) as usize]).unwrap();
        Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
    }

    #[test]
    fn flattens_translucent_tokens_onto_white() {
        assert_eq!(hex(0x5b5bd6ff), "#5b5bd6");
        assert_eq!(hex(0x00000000), "#ffffff");
        assert_eq!(hex(0x00000080), "#7f7f7f");
    }

    #[test]
    fn renders_resvg_safe_svg_with_text_labels() {
        let out = svg("flowchart LR\n  A[Open] --> B[Close]\n").unwrap();
        assert!(out.starts_with("<svg"));
        assert!(out.contains(">Open<"));
        assert!(!out.contains("<foreignObject"));
    }

    #[test]
    fn reports_syntax_errors() {
        assert!(svg("flowchart LR\n  A -->\n").unwrap_err().contains("parse error"));
    }

    #[test]
    fn rasterizes_at_scale_and_caps_the_long_side() {
        let t = tree("flowchart LR\n  A --> B\n").unwrap();
        let (w, h) = (t.size().width(), t.size().height());
        let size = rasterize(&t, 2.).unwrap().size(0);
        assert_eq!((size.width.0, size.height.0), ((w * 2.).ceil() as i32, (h * 2.).ceil() as i32));
        let size = rasterize(&t, 10_000.).unwrap().size(0);
        assert_eq!(size.width.0.max(size.height.0), 8192);
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
