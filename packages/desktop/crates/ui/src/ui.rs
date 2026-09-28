use theme::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub fn shadow(color: u32, y: f32, blur: f32) -> BoxShadow {
    BoxShadow { color: rgba(color).into(), offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false }
}

pub fn ring(color: u32, width: f32) -> BoxShadow {
    BoxShadow { spread_radius: px(width), ..shadow(color, 0., 0.) }
}

fn highlight(color: u32) -> BoxShadow {
    BoxShadow { inset: true, ..shadow(color, 1., 0.) }
}

pub fn row_shadow() -> Vec<BoxShadow> {
    vec![shadow(0x1111130f, 1., 2.), ring(0x1111130f, 0.5)]
}

/// Flat white surface that holds content.
pub fn page<E: Styled>(e: E) -> E {
    e.bg(rgba(SURFACE))
}

/// Translucent chrome for the rail and sidebar column, split from the next pane by a hairline.
pub fn side<E: Styled>(e: E) -> E {
    e.bg(rgba(0xfafafbb3)).border_r(px(0.5)).border_color(rgba(SEPARATOR))
}

/// Floating menus and sheets.
pub fn pop<E: Styled>(e: E) -> E {
    e.bg(rgba(SURFACE)).rounded(px(26.)).shadow(vec![ring(SEPARATOR, 0.5), highlight(0xfffffff2), shadow(0x00000024, 18., 50.), shadow(0x0000000f, 2., 6.)])
}

/// Translucent chrome for floating buttons and groups.
pub fn glass<E: Styled>(e: E) -> E {
    e.bg(rgba(0xffffff9e)).shadow(vec![ring(HAIRLINE, 0.5), highlight(0xfffffff2), shadow(0x0000000a, 1., 2.), shadow(0x0000000f, 6., 20.)])
}

pub fn dot(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded(px(size / 2.)).bg(rgba(color))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Primary,
    Glass,
    Secondary,
    Ghost,
    Accent,
}

impl Variant {
    pub fn fg(self) -> u32 {
        match self {
            Variant::Primary | Variant::Accent => WHITE,
            Variant::Glass | Variant::Secondary => TEXT,
            Variant::Ghost => TEXT_2,
        }
    }
}

pub fn button(id: impl Into<ElementId>, v: Variant, icon_name: Option<&str>, label: impl IntoElement) -> Stateful<Div> {
    let d = div()
        .id(id)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(16.))
        .cursor_pointer()
        .text_size(px(13.))
        .whitespace_nowrap()
        .text_color(rgba(v.fg()));
    let d = match v {
        Variant::Primary => d.bg(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD).shadow(vec![highlight(0xffffff2e), shadow(0x0000002e, 4., 12.)]),
        Variant::Glass => glass(d).font_weight(FontWeight::MEDIUM),
        Variant::Accent => d.bg(rgba(ACCENT)).font_weight(FontWeight::SEMIBOLD).shadow(vec![highlight(0xffffff40), shadow(0x0a84ff59, 2., 6.)]),
        Variant::Secondary => d.bg(rgba(FILL_3)).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_4))),
        Variant::Ghost => d.font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_3))),
    };
    d.children(icon_name.map(|n| icon(n, 14., v.fg()))).child(label)
}

/// The 36px size used for sheet footers.
pub fn large(b: Stateful<Div>) -> Stateful<Div> {
    b.h(px(36.)).px(px(18.)).rounded(px(18.)).text_size(px(14.))
}

pub fn button_kbd(keys: &str) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::NORMAL).opacity(0.7).child(keys.to_string())
}

fn glyph_button(id: impl Into<ElementId>, name: &str, w: f32, h: f32, radius: f32, color: u32) -> Stateful<Div> {
    let glyph = if w >= 34. && w == h { 17. } else { 16. };
    div()
        .id(id)
        .w(px(w))
        .h(px(h))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .cursor_pointer()
        .hover(|s| s.bg(rgba(FILL_3)))
        .child(icon(name, glyph, color))
}

pub fn icon_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 28., 28., 7., TEXT_2)
}

pub fn icon_button_sized(id: impl Into<ElementId>, name: &str, size: f32, color: u32) -> Stateful<Div> {
    glyph_button(id, name, size, size, 7., color)
}

pub fn icon_group(buttons: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    glass(div().h(px(32.)).px(px(1.)).flex().flex_none().items_center().rounded(px(16.))).children(buttons)
}

pub fn group_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 34., 30., 15., TEXT)
}

pub struct Segment<T> {
    pub icon: Option<&'static str>,
    pub value: T,
    pub label: SharedString,
    pub badge: Option<String>,
}

pub fn segmented<V: 'static, T: Copy + PartialEq + 'static>(
    items: Vec<Segment<T>>,
    active: T,
    small: bool,
    fill: bool,
    on: impl Fn(&mut V, T, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Div {
    let on = std::rc::Rc::new(on);
    let (h, r, ir, fs) = if small { (24., 11., 8., 12.) } else { (30., 12., 9., 13.5) };
    div().p(px(3.)).flex().gap(px(2.)).rounded(px(r)).bg(rgba(FILL_3)).children(items.into_iter().enumerate().map(|(i, s)| {
        let on = on.clone();
        let v = s.value;
        let selected = v == active;
        div()
            .id(("segment", i))
            .when(fill, |d| d.flex_1())
            .h(px(h))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .rounded(px(ir))
            .cursor_pointer()
            .text_size(px(fs))
            .whitespace_nowrap()
            .when(selected, |d| {
                d.bg(rgba(SURFACE)).shadow(vec![shadow(0x0000001a, 1., 3.), ring(0x0000000d, 0.5)]).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD)
            })
            .when(!selected, |d| d.text_color(rgba(TEXT_2)).font_weight(FontWeight::MEDIUM))
            .children(s.icon.map(|name| icon(name, fs - 1.5, if selected { TEXT } else { TEXT_2 })))
            .child(s.label)
            .children(s.badge.map(|b| {
                div()
                    .px(px(5.))
                    .rounded(px(8.))
                    .bg(rgba(HAIRLINE))
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(TEXT_2))
                    .child(b)
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on(this, v, cx)))
    }))
}

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Waiting,
    Running,
    Failed,
    Merged,
    Sent,
    Draft,
    Done(usize, usize),
}

pub fn diffstat(added: usize, removed: usize) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(6.))
        .font_family(MONO)
        .text_size(px(11.))
        .child(div().text_color(rgba(DIFF_ADD_TEXT)).child(format!("+{added}")))
        .child(div().text_color(rgba(DIFF_DEL_TEXT)).child(format!("−{removed}")))
}

pub fn status(id: impl Into<ElementId>, state: State) -> Div {
    let pill = |bg: u32, fg: u32| {
        div()
            .h(px(20.))
            .pl(px(7.))
            .pr(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.))
            .rounded(px(10.))
            .bg(rgba(bg))
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(fg))
    };
    match state {
        State::Waiting => pill(WAITING_BG, WAITING_TEXT).child(dot(6., WAITING)).child("Waiting"),
        State::Running => pill(RUNNING_BG, RUNNING_TEXT).child(spinner(id, 11., RUNNING_TEXT)).child("Running"),
        State::Failed => pill(FAILED_BG, FAILED).child(icon("x-bold", 11., FAILED)).child("Failed"),
        State::Merged => pill(MERGED_BG, MERGED).child(icon("merge", 11., MERGED)).child("Merged"),
        State::Sent => pill(FILL_3, TEXT_2).child(icon("check", 11., TEXT_2)).child("Sent"),
        State::Draft => pill(ACCENT_BG, ACCENT).child(dot(6., ACCENT)).child("Draft"),
        State::Done(added, removed) => diffstat(added, removed),
    }
}

fn mini_status(bg: u32, mark: impl IntoElement) -> AnyElement {
    div().h(px(20.)).px(px(6.)).flex().flex_none().items_center().justify_center().rounded(px(10.)).bg(rgba(bg)).child(mark).into_any_element()
}

pub fn agent_badge(provider: &str, label: impl Into<SharedString>) -> Div {
    div()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .rounded(px(11.))
        .bg(rgba(FILL_2))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .child(dot(7., provider_color(provider)))
        .child(label.into())
}

pub fn tag(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(10.))
        .bg(rgba(SURFACE))
        .shadow(vec![ring(SEPARATOR_STRONG, 0.5)])
        .text_size(px(11.5))
        .text_color(rgba(TEXT_2))
        .whitespace_nowrap()
        .child(label.into())
}

/// One chip per space-separated key, so "⌘ K" renders as ⌘ and K.
pub fn kbd(keys: &str) -> Div {
    div().flex().flex_none().gap(px(4.)).children(keys.split_whitespace().map(|k| {
        div().px(px(6.)).py(px(1.)).rounded(px(5.)).bg(rgba(FILL_4)).font_family(MONO).text_size(px(11.)).line_height(px(16.)).text_color(rgba(TEXT_2)).child(k.to_string())
    }))
}

/// One letter for a repository, taken from its last dash-separated word: "app-ios" is I.
pub fn repo_mark(name: &str, selected: bool, waiting: bool, running: bool) -> Div {
    let word = name.rsplit('-').find(|w| !w.is_empty()).unwrap_or(name);
    let letter = word.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    repo_tile(&letter, 22., selected, waiting, running)
}

/// A repository's initials on a square tile, dotted top-right while it waits and bottom-right while it runs.
pub fn repo_tile(letters: &str, size: f32, selected: bool, waiting: bool, running: bool) -> Div {
    let badge = |d: Div, color: u32| d.absolute().right(px(-2.)).size(px(9.)).rounded(px(5.)).bg(rgba(color)).shadow(vec![ring(WHITE, 2.)]);
    let scale = if letters.chars().count() > 1 { 0.34 } else { 0.5 };
    div()
        .relative()
        .size(px(size))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px((size * 0.27).round()))
        .text_size(px((size * scale).round()))
        .font_weight(FontWeight::SEMIBOLD)
        .when(selected, |d| d.bg(rgba(TEXT)).text_color(rgba(WHITE)))
        .when(!selected, |d| {
            d.bg(rgba(SURFACE)).text_color(rgba(TEXT_2)).shadow(vec![BoxShadow { inset: true, ..ring(SEPARATOR_STRONG, 0.5) }, shadow(0x0000000a, 1., 1.)])
        })
        .child(letters.to_string())
        .when(waiting, |d| d.child(badge(div().top(px(-2.)), WAITING)))
        .when(running, |d| d.child(badge(div().bottom(px(-2.)), RUNNING)))
}

pub fn repo_row(id: impl Into<ElementId>, name: &str, selected: bool, count: Option<usize>, state: Option<State>, spin: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .pl(px(6.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(9.))
        .cursor_pointer()
        .text_size(px(14.5))
        .font_weight(FontWeight::SEMIBOLD)
        .hover(|s| s.bg(rgba(FILL_2)))
        .child(div().w(px(14.)).flex().justify_center().child(icon(if selected { "chevron-down" } else { "chevron-right" }, 12., TEXT_5)))
        .child(repo_mark(name, selected, state == Some(State::Waiting), state == Some(State::Running)))
        .child(div().flex_1().truncate().child(name.to_string()))
        .children(count.map(|n| div().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_3)).child(n.to_string())))
        .map(|d| match state {
            Some(State::Waiting) => d.child(dot(7., WAITING)),
            Some(State::Running) => d.child(spinner(spin, 11., RUNNING_TEXT)),
            _ => d,
        })
}

#[allow(clippy::too_many_arguments)]
pub fn session_row(
    id: impl Into<ElementId>,
    selected: bool,
    title: String,
    state: State,
    provider: &str,
    branch: String,
    when: String,
    tags: Vec<String>,
) -> Stateful<Div> {
    let id = id.into();
    div()
        .id(id.clone())
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_none()
        .flex_col()
        .gap(px(4.))
        .rounded(px(12.))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgba(ROW_SELECTED)).shadow(row_shadow()))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_1))))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)).child(title))
                .child(status(id, state)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(rgba(TEXT_2))
                .child(dot(6., provider_color(provider)))
                .child(provider_name(provider))
                .child(div().text_color(rgba(TEXT_6)).child("·"))
                .child(icon("worktree", 12., TEXT_4))
                .child(div().min_w_0().truncate().font_family(MONO).text_size(px(11.5)).child(branch))
                .child(div().ml_auto().flex_none().text_color(rgba(TEXT_4)).child(when)),
        )
        .when(!tags.is_empty(), |d| d.child(div().mt(px(2.)).flex().gap(px(6.)).children(tags.into_iter().map(tag))))
}

/// Git's one-letter status for a file: M, A or D.
pub fn git_color(letter: char) -> u32 {
    match letter {
        'A' => RUNNING_TEXT,
        'D' => FAILED,
        _ => MODIFIED,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn tree_row(
    id: impl Into<ElementId>,
    label: String,
    folder: bool,
    open: bool,
    depth: usize,
    selected: bool,
    touched: bool,
    git: Option<char>,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .pl(px(8. + depth as f32 * 16.))
        .pr(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(13.5))
        .when(selected, |d| d.bg(rgba(ACCENT_BG)).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.font_weight(FontWeight(450.)).hover(|s| s.bg(rgba(FILL_2))))
        .child(div().w(px(12.)).flex().when(folder, |d| d.child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))))
        .child(icon(if folder { "folder" } else { "file" }, 14., TEXT_3))
        .child(div().flex_1().truncate().child(label))
        .when(touched, |d| d.child(dot(6., AGENT_CODEX)))
        .children(git.map(|g| div().font_family(MONO).text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(rgba(git_color(g))).child(g.to_string())))
}

pub fn checkbox(on: bool) -> Div {
    div()
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .when(on, |d| d.bg(rgba(TEXT)).child(icon("check", 11., WHITE)))
        .when(!on, |d| d.bg(rgba(SURFACE)).shadow(vec![ring(SEPARATOR_STRONG, 1.)]))
}

#[allow(clippy::too_many_arguments)]
pub fn change_row(
    id: impl Into<ElementId>,
    check: impl IntoElement,
    path: &str,
    selected: bool,
    added: usize,
    removed: usize,
    comments: usize,
) -> Stateful<Div> {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    div()
        .id(id)
        .px(px(12.))
        .py(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(12.))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgba(ROW_SELECTED)).shadow(row_shadow()))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_1))))
        .child(check)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().truncate().text_size(px(13.5)).font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM }).child(file.to_string()))
                .when(!dir.is_empty(), |d| d.child(div().truncate().font_family(MONO).text_size(px(11.)).text_color(rgba(TEXT_4)).child(dir.to_string()))),
        )
        .when(comments > 0, |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(WAITING_TEXT))
                    .child(icon("comment", 11., WAITING_TEXT))
                    .child(comments.to_string()),
            )
        })
        .child(diffstat(added, removed))
}

pub fn add_tile(id: impl Into<ElementId>, size: f32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(size))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px((size * 0.29).round()))
        .border_1()
        .border_dashed()
        .border_color(rgba(0x00000040))
        .cursor_pointer()
        .hover(|s| s.bg(rgba(FILL_2)))
        .child(icon("plus", 16., TEXT_3))
}

pub fn count_badge(count: usize) -> Div {
    div()
        .absolute()
        .top(px(3.))
        .right(px(3.))
        .min_w(px(16.))
        .h(px(16.))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .bg(rgba(WAITING))
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(WHITE))
        .child(count.to_string())
}

pub fn avatar(initials: &str, size: f32) -> Div {
    div()
        .size(px(size))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(size / 2.))
        .bg(rgba(TEXT))
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(WHITE))
        .child(initials.to_string())
}

/// Crumbs separated by slashes; the last one is the current page.
pub fn breadcrumb(crumbs: Vec<String>) -> Div {
    let last = crumbs.len().saturating_sub(1);
    div().flex().min_w_0().items_center().gap(px(8.)).text_size(px(13.)).whitespace_nowrap().children(crumbs.into_iter().enumerate().flat_map(|(i, c)| {
        let crumb = div()
            .truncate()
            .when(i == last, |d| d.min_w(px(60.)).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD))
            .when(i != last, |d| d.flex_none().text_color(rgba(TEXT_2)).font_weight(FontWeight(450.)))
            .child(c);
        let slash = (i > 0).then(|| div().flex_none().text_color(rgba(TEXT_6)).child("/"));
        slash.into_iter().chain([crumb])
    }))
}

pub fn page_bar() -> Div {
    div().h(px(44.)).pl(px(14.)).pr(px(12.)).flex().flex_none().items_center().gap(px(8.))
}

pub fn meta_item() -> Div {
    div().h(px(22.)).flex().flex_none().items_center().gap(px(6.)).text_size(px(12.5)).text_color(rgba(TEXT_2)).whitespace_nowrap()
}

/// Meta items after the breadcrumb, split by small dots; clipped when the bar runs out of room.
pub fn meta_row(items: Vec<AnyElement>) -> Div {
    let n = items.len();
    div().ml(px(8.)).min_w_0().flex().items_center().gap(px(10.)).overflow_hidden().children(items.into_iter().enumerate().flat_map(move |(i, item)| {
        [Some(item), (i + 1 < n).then(|| dot(3., TEXT_6).into_any_element())].into_iter().flatten()
    }))
}

/// "+n −n" in the status colors, as the top bar shows it.
pub fn meta_diff(added: usize, removed: usize, size: f32) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(size * 0.6))
        .font_family(MONO)
        .text_size(px(size))
        .child(div().text_color(rgba(RUNNING_TEXT)).child(format!("+{added}")))
        .child(div().text_color(rgba(FAILED)).child(format!("−{removed}")))
}

pub fn meta_value(text: impl Into<SharedString>) -> Div {
    div().text_color(rgba(TEXT)).font_weight(FontWeight::MEDIUM).child(text.into())
}

/// How much of the context window is left, drawn as a 32px bar.
pub fn context_bar(left: f32) -> Div {
    let used = (1. - left).clamp(0., 1.);
    div().w(px(32.)).h(px(4.)).rounded(px(2.)).bg(rgba(SEPARATOR_STRONG)).child(div().h_full().w(px(32. * used)).rounded(px(2.)).bg(rgba(if used > 0.8 { WAITING } else { TEXT })))
}

pub fn section_header(label: impl Into<SharedString>, count: Option<usize>) -> Div {
    div()
        .pt(px(12.))
        .px(px(10.))
        .pb(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(TEXT_3))
        .child(label.into())
        .children(count.map(|n| div().font_weight(FontWeight::MEDIUM).child(n.to_string())))
}

pub fn swatch(color: u32, size: f32, radius: f32) -> Div {
    div().size(px(size)).flex_none().rounded(px(radius)).bg(rgba(color))
}

pub fn field_label(label: impl Into<SharedString>) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child(label.into())
}

pub fn field_box() -> Div {
    div()
        .h(px(38.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(11.))
        .bg(rgba(0xffffffd9))
        .shadow(vec![ring(SEPARATOR, 0.5)])
}

/// A dim full-window layer behind a sheet; clicking it dismisses.
pub fn backdrop(id: impl Into<ElementId>, alpha: u8) -> Stateful<Div> {
    div().id(id).absolute().inset_0().bg(rgba(0x28241e00 | alpha as u32)).occlude()
}

/// Sheet with a title and close button, horizontally centred `top` pixels from the window's top.
pub fn modal(title: &str, width: f32, top: f32, close: Stateful<Div>, body: impl IntoIterator<Item = AnyElement>) -> Div {
    div().absolute().top(px(top)).left_0().right_0().flex().justify_center().child(
        pop(div().w(px(width)).pt(px(22.)).px(px(24.)).pb(px(20.)).flex().flex_col().gap(px(16.)))
            .occlude()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().flex_1().text_size(px(20.)).font_weight(FontWeight::BOLD).child(title.to_string()))
                    .child(close),
            )
            .children(body),
    )
}

pub fn palette_row(id: impl Into<ElementId>, selected: bool, lead: impl IntoElement, title: String, detail: String, keys: Option<&str>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(40.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(12.))
        .rounded(px(10.))
        .cursor_pointer()
        .text_size(px(14.))
        .when(selected, |d| d.bg(rgba(ACCENT_BG)))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
        .child(div().size(px(24.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(rgba(FILL_3)).child(lead))
        .child(div().flex_none().max_w(px(320.)).truncate().font_weight(FontWeight::MEDIUM).child(title))
        .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(rgba(TEXT_4)).child(detail))
        .children(keys.map(kbd))
}

pub fn menu_row(id: impl Into<ElementId>, icon_name: &str, label: &str, keys: Option<&str>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(13.5))
        .hover(|s| s.bg(rgba(FILL_2)))
        .child(icon(icon_name, 14., TEXT_2))
        .child(div().flex_1().child(label.to_string()))
        .children(keys.map(kbd))
}

pub fn chip(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .py(px(2.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(12.))
        .whitespace_nowrap()
        .when(selected, |d| d.bg(rgba(0xffffffcc)).text_color(rgba(TEXT)).shadow(vec![ring(FILL_3, 1.)]))
        .when(!selected, |d| d.text_color(rgba(TEXT_2)).hover(|s| s.bg(rgba(FILL_2))))
}

pub fn link(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex_none().cursor_pointer().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(rgba(ACCENT)).whitespace_nowrap().child(label.into())
}

/// Rounded search-like field that opens something when clicked.
pub fn trigger_field(id: impl Into<ElementId>, icon_name: &str, label: &str, keys: &str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(36.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .rounded(px(12.))
        .bg(rgba(0xffffffb3))
        .shadow(vec![ring(HAIRLINE, 0.5)])
        .cursor_pointer()
        .text_size(px(13.5))
        .text_color(rgba(TEXT_3))
        .child(icon(icon_name, 14., TEXT_3))
        .child(div().flex_1().child(label.to_string()))
        .child(div().text_size(px(11.5)).text_color(rgba(TEXT_4)).child(keys.to_string()))
}

pub fn worktree_row(id: impl Into<ElementId>, label: String, branch: String, main: bool, selected: bool, state: Option<State>) -> Stateful<Div> {
    let id = id.into();
    let lead = if main {
        icon("folder", 13., TEXT_3).into_any_element()
    } else if selected {
        dot(8., ACCENT).shadow(vec![ring(ACCENT_RING, 3.)]).into_any_element()
    } else {
        div().size(px(7.)).rounded(px(4.)).border_1().border_color(rgba(TEXT_5)).into_any_element()
    };
    div()
        .id(id.clone())
        .min_h(px(32.))
        .py(px(5.))
        .pl(px(34.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(9.))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgba(0xffffffe6)).shadow(row_shadow()))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
        .child(div().w(px(14.)).flex().flex_none().justify_center().child(lead))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(px(8.))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(14.))
                        .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight(450.) })
                        .text_color(rgba(if selected || main { TEXT } else { TEXT_BODY }))
                        .child(label),
                )
                .child(div().min_w_0().truncate().font_family(MONO).text_size(px(11.)).text_color(rgba(TEXT_4)).child(branch)),
        )
        .children(state.map(|s| match s {
            State::Waiting => mini_status(WAITING_BG, dot(6., WAITING)),
            State::Running => mini_status(RUNNING_BG, spinner(id.clone(), 11., RUNNING_TEXT)),
            _ => status(id, s).into_any_element(),
        }))
}
