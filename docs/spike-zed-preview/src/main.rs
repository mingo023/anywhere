use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use gpui_kit::component::input::{Editor, EditorState, Rope};
use gpui_kit::component::text::{TextView, TextViewState, TextViewStyle};
use gpui_kit::component::{ActiveTheme, Root};
#[allow(unused_imports)]
use gpui_kit::*;
use imara_diff::{Algorithm, Diff, InternedInput};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::*;
use ui::Segment;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const ROW: f32 = 22.;
const CONTEXT: usize = 3;

type Spans = Vec<(Range<usize>, HighlightStyle)>;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Editor,
    Rows,
    Markdown,
    Diff,
}

enum DiffRow {
    Fold(usize),
    Line { sign: char, old: Option<usize>, new: Option<usize>, text: SharedString, hl: Spans },
}

#[derive(Default)]
struct Bench {
    speed: f32,
    start: Option<Instant>,
    last: Option<Instant>,
    frames: Vec<f32>,
    work: std::rc::Rc<std::cell::RefCell<Vec<f32>>>,
    prev: f32,
}

struct Spike {
    tab: Tab,
    editor: Entity<EditorState>,
    lines: Vec<SharedString>,
    spans: Vec<Spans>,
    rows_scroll: UniformListScrollHandle,
    markdown: Entity<TextViewState>,
    diff: Vec<DiffRow>,
    diff_list: ListState,
    bench: Option<Bench>,
}

fn hsla(c: u32) -> Hsla {
    rgba(c).into()
}

fn bg(c: u32) -> HighlightStyle {
    HighlightStyle { background_color: Some(hsla(c)), ..Default::default() }
}

fn syntax_color(name: &str) -> u32 {
    match name.split('.').next().unwrap_or(name) {
        "keyword" | "boolean" | "preproc" | "attribute" => SYN_KEYWORD,
        "function" | "constructor" | "type" | "enum" | "tag" => SYN_FN,
        "string" | "number" | "constant" => SYN_STRING,
        "comment" => SYN_COMMENT,
        _ => TEXT_BODY,
    }
}

fn highlight_theme() -> Arc<HighlightTheme> {
    let mut v = serde_json::to_value(&*HighlightTheme::default_light()).unwrap();
    if let Some(syn) = v["style"]["syntax"].as_object_mut() {
        for (k, val) in syn.iter_mut() {
            *val = serde_json::json!({ "color": serde_json::to_value(hsla(syntax_color(k))).unwrap() });
        }
    }
    let style = &mut v["style"];
    for (k, c) in [
        ("editor.background", SURFACE_SUNKEN),
        ("editor.foreground", TEXT_BODY),
        ("editor.line_number", TEXT_5),
        ("editor.active_line_number", TEXT_2),
        ("editor.active_line.background", FILL_1),
    ] {
        style[k] = serde_json::to_value(hsla(c)).unwrap();
    }
    Arc::new(serde_json::from_value(v).unwrap())
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    eprintln!("[time] {label}: {:?}", t.elapsed());
    out
}

fn spans_per_line(label: &str, text: &str, lines: &[&str]) -> Vec<Spans> {
    let theme = highlight_theme();
    let styles = timed(&format!("{label} tree-sitter parse+styles ({} lines)", lines.len()), || {
        let mut hl = SyntaxHighlighter::new("rust");
        hl.update(None, &Rope::from(text), None);
        hl.styles(&(0..text.len()), &*theme)
    });
    let mut out = vec![Vec::new(); lines.len()];
    let mut starts = Vec::with_capacity(lines.len());
    let mut off = 0;
    for l in lines {
        starts.push(off);
        off += l.len() + 1;
    }
    for (r, s) in styles {
        let mut i = starts.partition_point(|&o| o <= r.start).saturating_sub(1);
        while i < lines.len() && starts[i] < r.end {
            let a = r.start.max(starts[i]) - starts[i];
            let b = (r.end.min(starts[i] + lines[i].len())).saturating_sub(starts[i]);
            if a < b {
                out[i].push((a..b, s));
            }
            i += 1;
        }
    }
    out
}

fn tokens(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            start.get_or_insert(i);
            continue;
        }
        if let Some(a) = start.take() {
            out.push((a, &s[a..i]));
        }
        out.push((i, &s[i..i + c.len_utf8()]));
    }
    if let Some(a) = start {
        out.push((a, &s[a..]));
    }
    out
}

fn word_diff(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    let mut input = InternedInput::default();
    input.update_before(a.iter().map(|t| t.1));
    input.update_after(b.iter().map(|t| t.1));
    let diff = Diff::compute(Algorithm::Histogram, &input);
    let span = |toks: &[(usize, &str)], r: Range<u32>| toks[r.start as usize].0..toks[r.end as usize - 1].0 + toks[r.end as usize - 1].1.len();
    let (mut del, mut add) = (Vec::new(), Vec::new());
    for h in diff.hunks() {
        if !h.before.is_empty() {
            del.push(span(&a, h.before.clone()));
        }
        if !h.after.is_empty() {
            add.push(span(&b, h.after.clone()));
        }
    }
    (del, add)
}

fn big_source(min_lines: usize) -> String {
    let unit = std::fs::read_to_string(format!("{REPO}/packages/desktop/crates/pocket/src/main.rs")).unwrap();
    let copies = min_lines.div_ceil(unit.lines().count());
    unit.repeat(copies)
}

fn mutate(old: &str) -> String {
    let mut out = String::with_capacity(old.len());
    for (i, line) in old.lines().enumerate() {
        if i % 97 == 50 {
            continue;
        }
        if i % 131 == 3 {
            out.push_str("        tracing::debug!(\"spike\");\n");
        }
        if i % 41 == 7 {
            let edited = if line.contains("self") { line.replacen("self", "this", 1) } else { format!("{line} // changed") };
            out.push_str(&edited);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn build_diff(old: &str, new: &str) -> Vec<DiffRow> {
    let (ol, nl): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let (os, ns) = (spans_per_line("diff old", old, &ol), spans_per_line("diff new", new, &nl));
    let t = Instant::now();
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let hunks: Vec<_> = diff.hunks().collect();
    eprintln!("[time] line diff ({} vs {} lines, {} hunks): {:?}", ol.len(), nl.len(), hunks.len(), t.elapsed());

    let t = Instant::now();
    let mut rows = Vec::new();
    let line = |sign, o: Option<usize>, n: Option<usize>, text: &str, syn: &Spans, words: &[Range<usize>], word_bg| DiffRow::Line {
        sign,
        old: o.map(|i| i + 1),
        new: n.map(|i| i + 1),
        text: text.to_string().into(),
        hl: combine_highlights(syn.iter().cloned(), words.iter().map(|r| (r.clone(), bg(word_bg)))).collect(),
    };
    let ctx = |rows: &mut Vec<DiffRow>, o: usize, n: usize| rows.push(line(' ', Some(o), Some(n), ol[o], &os[o], &[], 0));
    let (mut o, mut n) = (0usize, 0usize);
    for (k, h) in hunks.iter().enumerate() {
        let (bs, as_) = (h.before.start as usize, h.after.start as usize);
        let gap = bs - o;
        let head = if k == 0 { 0 } else { CONTEXT.min(gap) };
        let tail = CONTEXT.min(gap - head);
        for i in 0..head {
            ctx(&mut rows, o + i, n + i);
        }
        if gap > head + tail {
            rows.push(DiffRow::Fold(gap - head - tail));
        }
        for i in gap - tail..gap {
            ctx(&mut rows, o + i, n + i);
        }
        let (dels, adds) = (h.before.len(), h.after.len());
        let words: Vec<_> = (0..dels.min(adds)).map(|i| word_diff(ol[bs + i], nl[as_ + i])).collect();
        for i in 0..dels {
            let w = words.get(i).map_or(&[][..], |w| &w.0[..]);
            rows.push(line('-', Some(bs + i), None, ol[bs + i], &os[bs + i], w, 0xe5484d38));
        }
        for i in 0..adds {
            let w = words.get(i).map_or(&[][..], |w| &w.1[..]);
            rows.push(line('+', None, Some(as_ + i), nl[as_ + i], &ns[as_ + i], w, 0x30a46c40));
        }
        (o, n) = (h.before.end as usize, h.after.end as usize);
    }
    let rest = ol.len() - o;
    for i in 0..CONTEXT.min(rest) {
        ctx(&mut rows, o + i, n + i);
    }
    if rest > CONTEXT {
        rows.push(DiffRow::Fold(rest - CONTEXT));
    }
    eprintln!("[time] word diff + rows ({} rows): {:?}", rows.len(), t.elapsed());
    rows
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

impl Spike {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let n: usize = std::env::var("SPIKE_LINES").ok().and_then(|s| s.parse().ok()).unwrap_or(20_000);
        let text = big_source(n);
        eprintln!("[input] source {} lines, {} KB", text.lines().count(), text.len() / 1024);

        let editor = cx.new(|cx| EditorState::new(window, cx).language("rust").line_number(true).searchable(true).soft_wrap(false));
        timed("editor set_value", || editor.update(cx, |s, cx| s.set_value(text.clone(), window, cx)));

        let lines: Vec<&str> = text.lines().collect();
        let spans = spans_per_line("rows", &text, &lines);
        let lines = lines.into_iter().map(|l| SharedString::from(l.to_string())).collect();

        let md = std::fs::read_to_string(format!("{REPO}/docs/research-zed-diff-rendering.md")).unwrap().repeat(std::env::var("SPIKE_MD").ok().and_then(|s| s.parse().ok()).unwrap_or(30));
        eprintln!("[input] markdown {} lines, {} KB", md.lines().count(), md.len() / 1024);
        let markdown = timed("markdown state new", || cx.new(|cx| TextViewState::markdown(&md, cx)));

        let diff_old = big_source(n.min(5_000));
        let diff = build_diff(&diff_old, &mutate(&diff_old));
        let diff_list = ListState::new(diff.len(), ListAlignment::Top, px(200.));

        let tab = match std::env::var("SPIKE_TAB").as_deref() {
            Ok("rows") => Tab::Rows,
            Ok("md") => Tab::Markdown,
            Ok("diff") => Tab::Diff,
            _ => Tab::Editor,
        };
        let bench = std::env::var("SPIKE_BENCH").ok().map(|s| Bench { speed: s.parse().unwrap_or(40.), ..Default::default() });
        Self { tab, editor, lines, spans, rows_scroll: UniformListScrollHandle::new(), markdown, diff, diff_list, bench }
    }

    fn scroll_step(&mut self, speed: f32, cx: &mut Context<Self>) -> f32 {
        match self.tab {
            Tab::Editor => self.editor.update(cx, |s, cx| {
                let y = s.scroll_offset().y;
                let next = if self.bench.as_ref().is_some_and(|b| b.prev == f32::from(y) && y < px(0.)) { px(0.) } else { y - px(speed) };
                s.set_scroll_offset(point(px(0.), next), cx);
                f32::from(y)
            }),
            Tab::Rows => {
                let handle = &self.rows_scroll.0.borrow().base_handle;
                let y = handle.offset().y - px(speed);
                let max = self.lines.len() as f32 * ROW - 1000.;
                handle.set_offset(point(px(0.), if f32::from(-y) > max { px(0.) } else { y }));
                f32::from(y)
            }
            Tab::Markdown | Tab::Diff => {
                let list = if self.tab == Tab::Diff { self.diff_list.clone() } else { self.markdown.read(cx).list_state().clone() };
                let before = list.logical_scroll_top();
                list.scroll_by(px(speed));
                let after = list.logical_scroll_top();
                if before.item_ix == after.item_ix && before.offset_in_item == after.offset_in_item {
                    list.scroll_to(ListOffset::default());
                }
                after.item_ix as f32
            }
        }
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(speed) = self.bench.as_ref().map(|b| b.speed) else { return };
        let now = Instant::now();
        let b = self.bench.as_mut().unwrap();
        let start = *b.start.get_or_insert(now);
        if let Some(last) = b.last.replace(now)
            && now - start > Duration::from_millis(500)
        {
            b.frames.push((now - last).as_secs_f32() * 1000.);
        }
        if now - start > Duration::from_millis(4500) {
            let mut f = b.frames.clone();
            f.sort_by(f32::total_cmp);
            let avg = f.iter().sum::<f32>() / f.len() as f32;
            let p50 = percentile(&f, 0.5);
            let over = |ms: f32| f.iter().filter(|&&x| x > ms).count();
            let mut w = b.work.borrow().clone();
            w.sort_by(f32::total_cmp);
            eprintln!("[work] render->paint p50 {:.2}ms p95 {:.2} p99 {:.2} max {:.2}", percentile(&w, 0.5), percentile(&w, 0.95), percentile(&w, 0.99), w.last().unwrap());
            eprintln!(
                "[bench] speed {speed}px/frame frames {} avg {avg:.2}ms p50 {p50:.2} p95 {:.2} p99 {:.2} max {:.2} | >1.5x vsync {} >16.7ms {} >33ms {}",
                f.len(),
                percentile(&f, 0.95),
                percentile(&f, 0.99),
                f.last().unwrap(),
                over(p50 * 1.5),
                over(16.7),
                over(33.4),
            );
            cx.quit();
            return;
        }
        let pos = self.scroll_step(speed, cx);
        self.bench.as_mut().unwrap().prev = pos;
        window.request_animation_frame();
    }

    fn rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        uniform_list(
            "rows",
            self.lines.len(),
            cx.processor(|this, range: Range<usize>, _, _| {
                range
                    .map(|i| {
                        div()
                            .h(px(ROW))
                            .flex()
                            .items_center()
                            .child(div().w(px(46.)).pr(px(12.)).flex_none().flex().justify_end().text_color(rgba(TEXT_5)).child((i + 1).to_string()))
                            .child(div().whitespace_nowrap().text_color(rgba(TEXT_BODY)).child(StyledText::new(this.lines[i].clone()).with_highlights(this.spans[i].clone())))
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.rows_scroll)
        .flex_1()
        .min_h_0()
        .bg(rgba(SURFACE_SUNKEN))
        .font_family(MONO)
        .text_size(px(13.))
    }

    fn diff_row(&self, ix: usize) -> AnyElement {
        let num = |n: Option<usize>| div().w(px(44.)).pr(px(8.)).flex_none().flex().justify_end().text_color(rgba(TEXT_5)).child(n.map(|n| n.to_string()).unwrap_or_default());
        match &self.diff[ix] {
            DiffRow::Fold(n) => div()
                .w_full()
                .h(px(ROW))
                .flex()
                .items_center()
                .pl(px(96.))
                .bg(rgba(FILL_1))
                .text_color(rgba(TEXT_3))
                .font_family(SANS)
                .text_size(px(12.))
                .child(format!("{n} unchanged lines"))
                .into_any_element(),
            DiffRow::Line { sign, old, new, text, hl } => {
                let (row_bg, sign_color) = match sign {
                    '+' => (DIFF_ADD_BG, DIFF_ADD_TEXT),
                    '-' => (DIFF_DEL_BG, DIFF_DEL_TEXT),
                    _ => (0, TEXT_5),
                };
                div()
                    .w_full()
                    .h(px(ROW))
                    .flex()
                    .items_center()
                    .font_family(MONO)
                    .text_size(px(13.))
                    .bg(rgba(row_bg))
                    .child(num(*old))
                    .child(num(*new))
                    .child(div().w(px(18.)).flex_none().text_color(rgba(sign_color)).child(sign.to_string()))
                    .child(div().whitespace_nowrap().text_color(rgba(TEXT_BODY)).child(StyledText::new(text.clone()).with_highlights(hl.clone())))
                    .into_any_element()
            }
        }
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame_start = Instant::now();
        self.tick(window, cx);
        let work = self.bench.as_ref().map(|b| b.work.clone());
        let tabs = [(Tab::Editor, "gpui-kit Editor"), (Tab::Rows, "Own rows"), (Tab::Markdown, "Markdown"), (Tab::Diff, "Diff")]
            .into_iter()
            .map(|(value, label)| Segment { icon: None, value, label: label.into(), badge: None })
            .collect();
        let md_style = TextViewStyle { highlight_theme: cx.theme().highlight_theme.clone(), inline_code: bg(FILL_2), ..Default::default() };
        let body: AnyElement = match self.tab {
            Tab::Editor => Editor::new(&self.editor).readonly(true).bordered(false).h_full().font_family(MONO).text_size(px(13.)).line_height(px(ROW)).into_any_element(),
            Tab::Rows => self.rows(cx).into_any_element(),
            Tab::Markdown => div()
                .size_full()
                .p(px(24.))
                .bg(rgba(SURFACE))
                .child(TextView::new(&self.markdown).selectable(true).scrollable(true).style(md_style).size_full())
                .into_any_element(),
            Tab::Diff => list(self.diff_list.clone(), cx.processor(|this, ix, _, _| this.diff_row(ix)))
                .flex_1()
                .min_h_0()
                .bg(rgba(SURFACE_SUNKEN))
                .font_family(MONO)
                .text_size(px(13.))
                .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .text_color(rgba(TEXT))
            .child(div().pt(px(36.)).px(px(16.)).pb(px(12.)).child(ui::segmented(tabs, self.tab, false, false, |this: &mut Self, t, cx| {
                this.tab = t;
                cx.notify();
            }, cx)))
            .child(div().flex_1().min_h_0().flex().flex_col().mx(px(16.)).mb(px(16.)).rounded(px(12.)).overflow_hidden().border_1().border_color(rgba(HAIRLINE)).child(body))
            .children(work.map(|w| canvas(|_, _, _| {}, move |_, _, _, _| w.borrow_mut().push(frame_start.elapsed().as_secs_f32() * 1000.)).absolute().size_0()))
    }
}

fn main() {
    gpui_kit::application().with_assets(theme::Assets).run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        let t = gpui_kit::component::Theme::global_mut(cx);
        t.mono_font_family = MONO.into();
        t.mono_font_size = px(13.);
        t.highlight_theme = highlight_theme();
        t.link = hsla(ACCENT);
        t.selection = hsla(ACCENT_BG);
        t.muted = hsla(SURFACE_SUNKEN);
        t.border = hsla(HAIRLINE);
        t.accent = hsla(FILL_2);
        let _ = cx.theme();
        let bounds = Bounds::centered(None, size(px(1200.), px(820.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Pocket spike".into()), appears_transparent: true, traffic_light_position: Some(point(px(14.), px(14.))) }),
            focus: false,
            inactive_frame_interval: None,
            ..Default::default()
        };
        cx.open_window(opts, |window, cx| {
            let view = cx.new(|cx| Spike::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open window");
    });
}

