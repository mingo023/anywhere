mod samples;

use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView, TextViewState, TextViewStyle, markdown_ast as mdast};
use gpui_kit::component::{ActiveTheme, Root};
#[allow(unused_imports)]
use gpui_kit::*;
use merman::svg::{HostTheme, Presentation, ResolvedPresentation, SvgOutputPolicy, SvgPipelinePreset, ThemeRole};
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use theme::*;

fn hex(c: u32) -> String {
    let a = (c & 0xff) as f32 / 255.;
    let mix = |v: u32| ((v & 0xff) as f32 * a + 255. * (1. - a)).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(c >> 24), mix(c >> 16), mix(c >> 8))
}

struct Merman {
    renderer: Renderer,
    presentation: ResolvedPresentation,
}

static MERMAN: LazyLock<Merman> = LazyLock::new(|| {
    let t = Instant::now();
    let roles = [
        (ThemeRole::Canvas, SURFACE),
        (ThemeRole::Surface, SURFACE_SUNKEN),
        (ThemeRole::SurfaceAlt, WINDOW),
        (ThemeRole::Text, TEXT),
        (ThemeRole::SubtleText, TEXT_3),
        (ThemeRole::Border, 0xd4d4d8ff),
        (ThemeRole::Line, TEXT_4),
        (ThemeRole::ClusterBackground, SURFACE_SUNKEN),
        (ThemeRole::ClusterBorder, 0xe4e4e7ff),
        (ThemeRole::NoteBackground, ACCENT_BG),
        (ThemeRole::NoteBorder, ACCENT),
        (ThemeRole::NoteText, TEXT),
        (ThemeRole::Error, FAILED),
    ];
    let mut theme = HostTheme::new().try_with_font_family("Geist, Helvetica Neue, sans-serif").unwrap();
    for (role, c) in roles {
        theme = theme.try_with_role(role, hex(c)).unwrap();
    }
    let theme = theme.try_with_series_palette(PALETTE.map(hex)).unwrap();
    let presentation = Presentation::new().with_theme(theme).resolve();
    let renderer = Renderer::new().with_engine(presentation.materialize_engine(merman::Engine::new()));
    eprintln!("[time] merman init {:.1}ms", t.elapsed().as_secs_f32() * 1e3);
    Merman { renderer, presentation }
});

fn merman_svg(src: &str) -> Result<String, String> {
    let pipeline = SvgOutputPolicy { preset: SvgPipelinePreset::ResvgSafe, ..Default::default() }.pipeline();
    let req = SvgRequest { pipeline: Some(pipeline), presentation: MERMAN.presentation.render_policy(), ..Default::default() };
    match MERMAN.renderer.render(RenderRequest::svg(src, OperationControl::new(), req)) {
        Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_string()),
        Ok(_) => Err("No diagram".into()),
        Err(e) => Err(e.to_string()),
    }
}

static SYSTEM: LazyLock<resvg::usvg::Options<'static>> = LazyLock::new(|| {
    let t = Instant::now();
    let mut opt = resvg::usvg::Options::default();
    Arc::make_mut(&mut opt.fontdb).load_system_fonts();
    eprintln!("[time] fontdb load {:.1}ms", t.elapsed().as_secs_f32() * 1e3);
    opt
});

static GEIST: LazyLock<resvg::usvg::Options<'static>> = LazyLock::new(|| {
    let mut opt = resvg::usvg::Options { fontdb: SYSTEM.fontdb.clone(), ..Default::default() };
    let db = Arc::make_mut(&mut opt.fontdb);
    for f in ["Geist-Regular", "Geist-Medium", "Geist-SemiBold", "Geist-Bold"] {
        db.load_font_file(format!("{}/../../packages/desktop/crates/theme/assets/fonts/{f}.ttf", env!("CARGO_MANIFEST_DIR"))).unwrap();
    }
    opt
});

fn parse(svg: &str, geist: bool) -> Result<Arc<resvg::usvg::Tree>, String> {
    resvg::usvg::Tree::from_str(svg, if geist { &GEIST } else { &SYSTEM }).map(Arc::new).map_err(|e| e.to_string())
}

fn rasterize(tree: &resvg::usvg::Tree, scale: f32) -> Result<Arc<RenderImage>, String> {
    let size = tree.size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32).ok_or("Diagram too large")?;
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut data = pixmap.take();
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let buffer = image::ImageBuffer::from_raw(w, h, data).unwrap();
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn image_bytes(image: &RenderImage) -> usize {
    image.as_bytes(0).map_or(0, |b| b.len())
}

#[derive(Clone)]
enum Diagram {
    Pending,
    Ready(Arc<RenderImage>),
    Svg(Arc<resvg::usvg::Tree>),
    Failed(SharedString),
}

struct Cache {
    items: HashMap<u64, Diagram>,
    geist: bool,
    budget: Option<usize>,
    images: HashMap<u64, (Arc<RenderImage>, Instant)>,
    rastering: HashSet<u64>,
    layout_dirty: bool,
    blank_paints: usize,
    rasters: usize,
    peak: usize,
    spawned: usize,
    done: usize,
    opened: Instant,
    stats: Vec<(f32, f32)>,
}

impl Cache {
    fn get(&mut self, key: u64, source: &SharedString, cx: &mut Context<Self>) -> Diagram {
        if let Some(d) = self.items.get(&key) {
            return d.clone();
        }
        self.items.insert(key, Diagram::Pending);
        self.spawned += 1;
        let source = source.clone();
        let source_head: String = source.lines().nth(1).unwrap_or_default().chars().take(24).collect();
        let (geist, lazy) = (self.geist, self.budget.is_some());
        cx.spawn(async move |this, cx| {
            let (out, ms) = cx
                .background_spawn(async move {
                    let t = Instant::now();
                    let text = merman_svg(&source);
                    let m = t.elapsed().as_secs_f32() * 1e3;
                    let t = Instant::now();
                    let out = text.and_then(|text| parse(&text, geist)).and_then(|tree| if lazy { Ok(Diagram::Svg(tree)) } else { rasterize(&tree, 2.).map(Diagram::Ready) });
                    (out, (m, t.elapsed().as_secs_f32() * 1e3))
                })
                .await;
            this.update(cx, |c, cx| {
                if std::env::var("SPIKE_VERBOSE").is_ok() {
                    eprintln!("[diagram] {source_head} merman {:.1}ms parse/raster {:.1}ms ok={}", ms.0, ms.1, out.is_ok());
                }
                c.done += 1;
                c.stats.push(ms);
                c.items.insert(key, out.unwrap_or_else(|e| Diagram::Failed(e.into())));
                c.layout_dirty = true;
                if c.done == c.spawned {
                    c.report();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        Diagram::Pending
    }

    fn image(&self, key: u64) -> Option<Arc<RenderImage>> {
        self.images.get(&key).map(|(image, _)| image.clone())
    }

    /// Called from paint, so only on-screen diagrams rasterize and stay warm.
    fn painted(&mut self, key: u64, tree: &Arc<resvg::usvg::Tree>, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(entry) = self.images.get_mut(&key) {
            entry.1 = now;
            return;
        }
        self.blank_paints += 1;
        if !self.rastering.insert(key) {
            return;
        }
        let tree = tree.clone();
        let scale = window.scale_factor();
        cx.spawn_in(window, async move |this, cx| {
            let image = cx.background_spawn(async move { rasterize(&tree, scale) }).await;
            this.update_in(cx, |c, window, cx| {
                c.rastering.remove(&key);
                c.rasters += 1;
                if let Ok(image) = image {
                    c.images.insert(key, (image, Instant::now()));
                }
                c.evict(window);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn evict(&mut self, window: &mut Window) {
        let budget = self.budget.unwrap_or(usize::MAX);
        let mut total: usize = self.images.values().map(|(i, _)| image_bytes(i)).sum();
        self.peak = self.peak.max(total);
        let mut lru: Vec<(u64, Instant)> = self.images.iter().map(|(k, (_, t))| (*k, *t)).collect();
        lru.sort_by_key(|(_, t)| *t);
        for (key, used) in lru {
            if total <= budget || used.elapsed() < Duration::from_millis(250) {
                break;
            }
            let (image, _) = self.images.remove(&key).unwrap();
            total -= image_bytes(&image);
            window.drop_image(image).ok();
        }
    }

    fn report(&self) {
        let mut m: Vec<f32> = self.stats.iter().map(|s| s.0).collect();
        let mut r: Vec<f32> = self.stats.iter().map(|s| s.1).collect();
        m.sort_by(f32::total_cmp);
        r.sort_by(f32::total_cmp);
        let bytes: usize = self.items.values().map(|d| if let Diagram::Ready(i) = d { image_bytes(i) } else { 0 }).sum();
        eprintln!(
            "[open] {} diagrams ready {:.0}ms after open | merman p50 {:.1} max {:.1} | parse/raster p50 {:.1} max {:.1} | eager images {} MB",
            self.done,
            self.opened.elapsed().as_secs_f32() * 1e3,
            m[m.len() / 2],
            m.last().unwrap(),
            r[r.len() / 2],
            r.last().unwrap(),
            bytes / 1_000_000,
        );
    }
}

struct Fence {
    key: u64,
    source: SharedString,
}

struct Mermaid {
    cache: Entity<Cache>,
}

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
        let mut h = DefaultHasher::new();
        code.value.hash(&mut h);
        let fence = Fence { key: h.finish(), source: code.value.clone().into() };
        Some(MarkdownNode::new("mermaid", fence).text(code.value.clone()).markdown(cx.node_source(node).unwrap_or(&code.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let fence = node.data::<Fence>().unwrap();
        let key = fence.key;
        let diagram = self.cache.update(cx, |c, cx| c.get(key, &fence.source, cx));
        let frame = div().id(SharedString::from(format!("mmd-{key}"))).my(px(8.)).rounded(px(8.)).border_1().border_color(rgba(HAIRLINE)).bg(rgba(SURFACE));
        match diagram {
            Diagram::Ready(image) => {
                let s = image.size(0);
                let (w, h) = (s.width.0 as f32 / 2., s.height.0 as f32 / 2.);
                frame.p(px(12.)).overflow_x_scroll().child(img(ImageSource::Render(image)).flex_none().w(px(w)).h(px(h)))
            }
            Diagram::Svg(tree) => {
                let size = tree.size();
                let image = self.cache.read(cx).image(key);
                let cache = self.cache.clone();
                let paint = canvas(|_, _, _| {}, move |_, _, window, cx| cache.update(cx, |c, cx| c.painted(key, &tree, window, cx))).absolute().size_full();
                frame.p(px(12.)).overflow_x_scroll().child(
                    div().relative().flex_none().w(px(size.width())).h(px(size.height())).child(paint).children(image.map(|i| img(ImageSource::Render(i)).size_full())),
                )
            }
            Diagram::Pending => frame.p(px(12.)).text_color(rgba(TEXT_3)).text_size(px(12.)).child("Rendering diagram…"),
            Diagram::Failed(e) => frame
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().text_color(rgba(FAILED)).text_size(px(12.)).child(e))
                .child(div().font_family(MONO).text_size(px(12.)).text_color(rgba(TEXT_2)).child(fence.source.clone())),
        }
    }
}

#[derive(Default)]
struct Bench {
    speed: f32,
    start: Option<Instant>,
    last: Option<Instant>,
    frames: Vec<f32>,
    landing: Vec<f32>,
    reloaded: bool,
    work: Rc<RefCell<Vec<f32>>>,
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn document(copies: usize) -> String {
    let big = (0..100).fold(String::from("flowchart TD\n"), |s, i| s + &format!("  N{i}[Node {i}] --> N{}[Node {}]\n", (i * 7 + 3) % 100, (i * 7 + 3) % 100));
    let mut md = String::from("# Mermaid spike\n\nDiagrams render in the background and are cached by source hash.\n\n");
    md += "```mermaid\nflowchart LR\n  A --> \n```\n\n```rust\nfn not_mermaid() {}\n```\n\n";
    for i in 0..copies {
        md += &format!("## Copy {i}\n\n");
        for (name, src) in samples::DIAGRAMS {
            md += &format!("### {name}\n\nA paragraph of prose above the {name} diagram so the page reads like a real doc, with enough words to wrap across the preview width at least once.\n\n```mermaid\n%% copy {i}\n{src}```\n\n");
        }
        if i == 0 {
            md += &format!("### 100-node flowchart\n\n```mermaid\n{big}```\n\n");
        }
    }
    md
}

struct Spike {
    markdown: Entity<TextViewState>,
    cache: Entity<Cache>,
    bench: Option<Bench>,
    scroll: Option<f32>,
    _observe: Subscription,
}

impl Spike {
    fn new(cx: &mut Context<Self>) -> Self {
        let copies = std::env::var("SPIKE_COPIES").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
        let md = document(copies);
        eprintln!("[input] markdown {} lines, {} KB, {} fences", md.lines().count(), md.len() / 1024, md.matches("```mermaid").count());
        let markdown = cx.new(|cx| TextViewState::markdown(&md, cx));
        let cache = cx.new(|_| Cache {
            items: HashMap::new(),
            geist: std::env::var("SPIKE_FONT").as_deref() == Ok("geist"),
            budget: std::env::var("SPIKE_BUDGET_MB").ok().and_then(|s| s.parse::<usize>().ok()).map(|mb| mb * 1_000_000),
            images: HashMap::new(),
            rastering: HashSet::new(),
            layout_dirty: false,
            blank_paints: 0,
            rasters: 0,
            peak: 0,
            spawned: 0,
            done: 0,
            opened: Instant::now(),
            stats: vec![],
        });
        let _observe = cx.observe(&cache, |this, cache, cx| {
            if std::mem::take(&mut cache.update(cx, |c, _| std::mem::take(&mut c.layout_dirty))) {
                this.markdown.update(cx, |s, cx| s.invalidate_inline_layout(cx));
            }
        });
        let bench = std::env::var("SPIKE_BENCH").ok().map(|s| Bench { speed: s.parse().unwrap_or(40.), ..Default::default() });
        let scroll = std::env::var("SPIKE_SCROLL").ok().and_then(|s| s.parse().ok());
        Self { markdown, cache, bench, scroll, _observe }
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (spawned, done) = { let c = self.cache.read(cx); (c.spawned, c.done) };
        let Some(b) = self.bench.as_mut() else { return };
        let now = Instant::now();
        if b.start.is_none() && (spawned == 0 || done < spawned) {
            if let Some(last) = b.last.replace(now) {
                b.landing.push((now - last).as_secs_f32() * 1000.);
            }
            window.request_animation_frame();
            return;
        }
        if b.start.is_none() && !b.landing.is_empty() {
            let mut f = std::mem::take(&mut b.landing);
            f.sort_by(f32::total_cmp);
            eprintln!("[landing] frames {} p50 {:.2}ms p95 {:.2} max {:.2} | >33ms {}", f.len(), percentile(&f, 0.5), percentile(&f, 0.95), f.last().unwrap(), f.iter().filter(|&&x| x > 33.4).count());
            b.work.borrow_mut().clear();
        }
        let start = *b.start.get_or_insert(now);
        if let Some(last) = b.last.replace(now)
            && now - start > Duration::from_millis(100)
        {
            let ms = (now - last).as_secs_f32() * 1000.;
            b.frames.push(ms);
            if done < spawned {
                b.landing.push(ms);
            }
        }
        if std::env::var("SPIKE_RELOAD").is_ok() && !b.reloaded && now - start > Duration::from_millis(1000) {
            b.reloaded = true;
            self.cache.update(cx, |c, _| {
                c.items.clear();
                c.images.clear();
                c.stats.clear();
                (c.spawned, c.done, c.opened) = (0, 0, Instant::now());
            });
        }
        if now - start > Duration::from_millis(4100) {
            let mut f = b.frames.clone();
            f.sort_by(f32::total_cmp);
            let p50 = percentile(&f, 0.5);
            let over = |ms: f32| f.iter().filter(|&&x| x > ms).count();
            let mut w = b.work.borrow().clone();
            w.sort_by(f32::total_cmp);
            eprintln!("[work] render->paint p50 {:.2}ms p95 {:.2} p99 {:.2} max {:.2}", percentile(&w, 0.5), percentile(&w, 0.95), percentile(&w, 0.99), w.last().unwrap());
            eprintln!(
                "[bench] speed {}px/frame frames {} p50 {p50:.2}ms p95 {:.2} p99 {:.2} max {:.2} | >1.5x vsync {} >33ms {}",
                b.speed,
                f.len(),
                percentile(&f, 0.95),
                percentile(&f, 0.99),
                f.last().unwrap(),
                over(p50 * 1.5),
                over(33.4),
            );
            let c = self.cache.read(cx);
            eprintln!("[bench] diagrams spawned {} done {}", c.spawned, c.done);
            if c.budget.is_some() {
                let live: usize = c.images.values().map(|(i, _)| image_bytes(i)).sum();
                eprintln!("[lazy] rasters {} blank paints {} images live {} MB peak {} MB", c.rasters, c.blank_paints, live / 1_000_000, c.peak / 1_000_000);
            }
            let b = self.bench.as_mut().unwrap();
            if !b.landing.is_empty() {
                b.landing.sort_by(f32::total_cmp);
                let l = &b.landing;
                eprintln!("[reload] frames while landing {} p50 {:.2}ms p95 {:.2} max {:.2} | >33ms {}", l.len(), percentile(l, 0.5), percentile(l, 0.95), l.last().unwrap(), l.iter().filter(|&&x| x > 33.4).count());
            }
            cx.quit();
            return;
        }
        let list = self.markdown.read(cx).list_state().clone();
        let before = list.logical_scroll_top();
        list.scroll_by(px(b.speed));
        let after = list.logical_scroll_top();
        if before.item_ix == after.item_ix && before.offset_in_item == after.offset_in_item {
            list.scroll_to(ListOffset::default());
        }
        window.request_animation_frame();
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame_start = Instant::now();
        self.tick(window, cx);
        if let Some(y) = self.scroll
            && self.cache.read(cx).done > 0
            && self.cache.read(cx).done == self.cache.read(cx).spawned
        {
            self.scroll = None;
            self.markdown.read(cx).list_state().scroll_by(px(y));
        }
        let work = self.bench.as_ref().map(|b| b.work.clone());
        let style = TextViewStyle { highlight_theme: cx.theme().highlight_theme.clone(), ..Default::default() };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .text_color(rgba(TEXT))
            .pt(px(36.))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .mx(px(16.))
                    .mb(px(16.))
                    .rounded(px(12.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgba(HAIRLINE))
                    .bg(rgba(SURFACE))
                    .px(px(40.))
                    .py(px(20.))
                    .child(TextView::new(&self.markdown).plugin(Mermaid { cache: self.cache.clone() }).selectable(true).scrollable(true).style(style).size_full()),
            )
            .children(work.map(|w| canvas(|_, _, _| {}, move |_, _, _, _| w.borrow_mut().push(frame_start.elapsed().as_secs_f32() * 1000.)).absolute().size_0()))
    }
}

fn main() {
    gpui_kit::application().with_assets(theme::Assets).run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1100.), px(820.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Mermaid spike".into()), appears_transparent: true, traffic_light_position: Some(point(px(14.), px(14.))) }),
            focus: false,
            inactive_frame_interval: None,
            kind: if std::env::var("SPIKE_POPUP").is_ok() { WindowKind::PopUp } else { WindowKind::Normal },
            ..Default::default()
        };
        cx.open_window(opts, |window, cx| {
            let view = cx.new(Spike::new);
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open window");
    });
}
