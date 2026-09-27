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
    e.bg(rgba(SURFACE)).rounded(px(20.)).shadow(vec![ring(HAIRLINE, 0.5), shadow(HAIRLINE, 12., 40.)])
}

/// Floating chrome for the rail and sidebar column.
pub fn side<E: Styled>(e: E) -> E {
    e.bg(rgba(0xfafafbb3)).rounded(px(22.)).shadow(vec![ring(0x00000012, 0.5), highlight(0xffffffe6), shadow(0x281e1414, 10., 34.)])
}

/// Floating menus and sheets.
pub fn pop<E: Styled>(e: E) -> E {
    e.bg(rgba(0xffffffd6)).rounded(px(26.)).shadow(vec![ring(SEPARATOR, 0.5), highlight(0xfffffff2), shadow(0x00000024, 18., 50.), shadow(0x0000000f, 2., 6.)])
}

fn glass<E: Styled>(e: E) -> E {
    e.bg(rgba(0xffffff9e)).shadow(vec![ring(HAIRLINE, 0.5), highlight(0xfffffff2), shadow(0x0000000a, 1., 2.), shadow(0x0000000f, 6., 20.)])
}

pub fn dot(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded(px(size / 2.)).bg(rgba(color))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Secondary,
    Ghost,
    Accent,
}

impl Variant {
    pub fn fg(self) -> u32 {
        match self {
            Variant::Accent => WHITE,
            Variant::Secondary => TEXT,
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
        Variant::Accent => d.bg(rgba(ACCENT)).font_weight(FontWeight::SEMIBOLD).shadow(vec![highlight(0xffffff40), shadow(0x0a84ff59, 2., 6.)]),
        Variant::Secondary => d.bg(rgba(FILL_3)).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_4))),
        Variant::Ghost => d.font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_3))),
    };
    d.children(icon_name.map(|n| icon(n, 14., v.fg()))).child(label)
}

pub fn button_kbd(keys: &str) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::NORMAL).opacity(0.7).child(keys.to_string())
}

fn glyph_button(id: impl Into<ElementId>, name: &str, w: f32, h: f32, radius: f32, color: u32) -> Stateful<Div> {
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
        .child(icon(name, 16., color))
}

pub fn icon_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 28., 28., 7., TEXT_2)
}

pub fn icon_group(buttons: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    glass(div().h(px(32.)).px(px(1.)).flex().flex_none().items_center().rounded(px(16.))).children(buttons)
}

pub fn group_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 34., 30., 15., TEXT)
}

pub struct Segment<T> {
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
        State::Done(added, removed) => diffstat(added, removed),
    }
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

pub fn repo_mark(name: &str, selected: bool, waiting: bool, running: bool) -> Div {
    let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    let badge = |d: Div, color: u32| d.absolute().right(px(-2.)).size(px(9.)).rounded(px(5.)).bg(rgba(color)).shadow(vec![ring(WHITE, 2.)]);
    div()
        .relative()
        .size(px(22.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(selected, |d| d.bg(rgba(TEXT)).text_color(rgba(WHITE)))
        .when(!selected, |d| {
            d.bg(rgba(SURFACE)).text_color(rgba(TEXT_2)).shadow(vec![BoxShadow { inset: true, ..ring(SEPARATOR_STRONG, 0.5) }, shadow(0x0000000a, 1., 1.)])
        })
        .child(letter)
        .when(waiting, |d| d.child(badge(div().top(px(-2.)), WAITING)))
        .when(running, |d| d.child(badge(div().bottom(px(-2.)), RUNNING)))
}

pub fn repo_row(id: impl Into<ElementId>, name: &str, selected: bool, state: Option<State>, spin: impl Into<ElementId>) -> Stateful<Div> {
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
        .map(|d| match state {
            Some(State::Waiting) => d.child(dot(7., WAITING)),
            Some(State::Running) => d.child(spinner(spin, 11., RUNNING_TEXT)),
            _ => d,
        })
}

pub fn session_row(id: impl Into<ElementId>, selected: bool, title: String, state: State, provider: &str, branch: String, when: String) -> Stateful<Div> {
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
}

pub fn tree_row(id: impl Into<ElementId>, label: String, folder: bool, open: bool, depth: usize) -> Stateful<Div> {
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
        .font_weight(FontWeight(450.))
        .hover(|s| s.bg(rgba(FILL_2)))
        .child(div().w(px(12.)).flex().when(folder, |d| d.child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))))
        .child(icon(if folder { "folder" } else { "file" }, 14., TEXT_3))
        .child(div().flex_1().truncate().child(label))
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

pub fn change_row(id: impl Into<ElementId>, check: impl IntoElement, path: &str, selected: bool, added: usize, removed: usize) -> Stateful<Div> {
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
        .child(diffstat(added, removed))
}
