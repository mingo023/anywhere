use theme::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub fn shadow(color: impl Into<Hsla>, y: f32, blur: f32) -> BoxShadow {
    BoxShadow { color: color.into(), offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false }
}

pub fn ring(color: impl Into<Hsla>, width: f32) -> BoxShadow {
    BoxShadow { spread_radius: px(width), ..shadow(color, 0., 0.) }
}

fn highlight(color: impl Into<Hsla>) -> BoxShadow {
    BoxShadow { inset: true, ..shadow(color, 1., 0.) }
}

pub fn row_shadow() -> Vec<BoxShadow> {
    vec![shadow(rgba(0x1111130f), 1., 2.), ring(rgba(0x1111130f), 0.5)]
}

/// Flat surface that holds content.
pub fn page<E: Styled>(e: E) -> E {
    e.bg(PAGE)
}

/// Translucent chrome for the rail and sidebar column, split from the next pane by a hairline.
pub fn side<E: Styled>(e: E) -> E {
    e.bg(SIDE).border_r(px(0.5)).border_color(SEPARATOR)
}

/// Floating menus and sheets.
pub fn pop<E: Styled>(e: E) -> E {
    e.bg(POPOVER).rounded(px(R_POPOVER)).shadow(vec![ring(SEPARATOR, 0.5), highlight(HIGHLIGHT), shadow(rgba(0x00000024), 18., 50.), shadow(rgba(0x0000000f), 2., 6.)])
}

pub fn menu_in<E: Styled + IntoElement + 'static>(id: impl Into<ElementId>, menu: E) -> AnimationElement<E> {
    menu.with_animation(id, Animation::new(MENU_IN).with_easing(ease_out_quint()), |e, t| e.opacity(t).mt(px(-4. * (1. - t))))
}

/// A menu dropped `top` px below its `relative` parent, painted above later siblings and kept inside the window.
pub fn dropdown(top: f32, menu: impl IntoElement) -> Div {
    div().absolute().top(px(top)).left_0().child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1))
}

/// Translucent chrome for floating buttons and groups.
pub fn glass<E: Styled>(e: E) -> E {
    e.bg(GLASS).shadow(vec![ring(HAIRLINE, 0.5), highlight(HIGHLIGHT), shadow(rgba(0x0000000a), 1., 2.), shadow(rgba(0x0000000f), 6., 20.)])
}

pub fn dot(size: f32, color: Token) -> Div {
    div().size(px(size)).flex_none().rounded(px(size / 2.)).bg(color)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Primary,
    Glass,
    Secondary,
    Ghost,
    Accent,
    Danger,
}

impl Variant {
    pub fn fg(self) -> Token {
        match self {
            Variant::Primary => ON_TEXT,
            Variant::Accent | Variant::Danger => WHITE,
            Variant::Glass | Variant::Secondary => TEXT,
            Variant::Ghost => TEXT_2,
        }
    }
}

pub fn primary<E: Styled>(e: E) -> E {
    e.bg(TEXT).shadow(vec![highlight(rgba(0xffffff2e)), shadow(rgba(0x0000002e), 4., 12.)])
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
        .text_color(v.fg());
    let d = match v {
        Variant::Primary => primary(d).font_weight(FontWeight::SEMIBOLD),
        Variant::Glass => glass(d).font_weight(FontWeight::MEDIUM),
        Variant::Accent => d.bg(ACCENT).font_weight(FontWeight::SEMIBOLD).shadow(vec![highlight(rgba(0xffffff40)), shadow(rgba(0x0a84ff59), 2., 6.)]),
        Variant::Secondary => d.bg(FILL_3).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(FILL_4)),
        Variant::Ghost => d.font_weight(FontWeight::MEDIUM).hover(|s| s.bg(FILL_3)),
        Variant::Danger => d.bg(FAILED).font_weight(FontWeight::SEMIBOLD),
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

fn glyph_frame(id: impl Into<ElementId>, w: f32, h: f32, radius: f32) -> Stateful<Div> {
    div().id(id).w(px(w)).h(px(h)).flex_none().flex().items_center().justify_center().rounded(px(radius)).cursor_pointer().hover(|s| s.bg(FILL_3))
}

fn glyph_button(id: impl Into<ElementId>, name: &str, w: f32, h: f32, radius: f32, color: Token) -> Stateful<Div> {
    let glyph = if w >= 34. && w == h { 17. } else { 16. };
    glyph_frame(id, w, h, radius).child(icon(name, glyph, color))
}

pub fn icon_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 28., 28., 7., TEXT_2)
}

pub fn icon_button_sized(id: impl Into<ElementId>, name: &str, size: f32, color: Token) -> Stateful<Div> {
    glyph_button(id, name, size, size, 7., color)
}

pub fn icon_group(buttons: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    glass(div().h(px(32.)).px(px(1.)).flex().flex_none().items_center().rounded(px(16.))).children(buttons)
}

pub fn group_button(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    glyph_button(id, name, 34., 30., 15., TEXT)
}

/// A group button whose glyph fades in, for one that just swapped it.
pub fn group_button_swapped(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    let fade = Animation::new(std::time::Duration::from_millis(120)).with_easing(ease_out_quint());
    glyph_frame(id, 34., 30., 15.).child(icon(name, 16., TEXT).with_animation("swap-in", fade, |i, t| i.opacity(t)))
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
    div().p(px(3.)).flex().gap(px(2.)).rounded(px(r)).bg(FILL_3).children(items.into_iter().enumerate().map(|(i, s)| {
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
                d.bg(SURFACE).shadow(vec![shadow(rgba(0x0000001a), 1., 3.), ring(rgba(0x0000000d), 0.5)]).text_color(TEXT).font_weight(FontWeight::SEMIBOLD)
            })
            .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM))
            .children(s.icon.map(|name| icon(name, fs - 1.5, if selected { TEXT } else { TEXT_2 })))
            .child(s.label)
            .children(s.badge.map(|b| {
                div()
                    .px(px(5.))
                    .rounded(px(8.))
                    .bg(HAIRLINE)
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_2)
                    .child(b)
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on(this, v, cx)))
    }))
}

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    NeedsYou,
    Working,
    Failed,
    Sent,
    Draft,
    Done(usize, usize),
    Idle(usize, usize),
    NotAttached,
}

pub fn diffstat(added: usize, removed: usize) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(6.))
        .font_family(MONO)
        .text_size(px(11.))
        .child(div().text_color(DIFF_ADD_TEXT).child(format!("+{added}")))
        .child(div().text_color(DIFF_DEL_TEXT).child(format!("−{removed}")))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Glyph {
    Dot,
    Spinner,
    Check,
    Cross,
}

/// How a status looks: its glyph, the glyph's colour, its label's colour and its fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    pub glyph: Glyph,
    pub mark: Token,
    pub text: Token,
    pub bg: Token,
}

/// The one map from a status to its look. States that are not an agent's status have none.
pub fn tone(state: State) -> Option<Tone> {
    let (glyph, mark, text, bg) = match state {
        State::NeedsYou => (Glyph::Dot, WAITING, WAITING_TEXT, WAITING_BG),
        State::Working => (Glyph::Spinner, ACCENT, ACCENT, ACCENT_TINT),
        State::Done(..) => (Glyph::Check, SUCCESS, SUCCESS_TEXT, SUCCESS_BG),
        State::Failed => (Glyph::Cross, FAILED, FAILED_TEXT, FAILED_BG),
        _ => return None,
    };
    Some(Tone { glyph, mark, text, bg })
}

pub fn glyph(id: impl Into<ElementId>, t: Tone) -> AnyElement {
    match t.glyph {
        Glyph::Dot => dot(7., t.mark).into_any_element(),
        Glyph::Spinner => spinner(id, 11., t.mark).into_any_element(),
        Glyph::Check => icon("check", 11., t.mark).into_any_element(),
        Glyph::Cross => icon("x-bold", 11., t.mark).into_any_element(),
    }
}

fn word(state: State) -> &'static str {
    match state {
        State::NeedsYou => "Needs you",
        State::Working => "Working",
        State::Failed => "Failed",
        State::Sent => "Sent",
        State::Draft => "Draft",
        State::Done(..) => "Done",
        State::Idle(..) => "Idle",
        State::NotAttached => "Not attached",
    }
}

pub fn status(id: impl Into<ElementId>, state: State) -> Div {
    let pill = |bg: Token, fg: Token| {
        div()
            .h(px(20.))
            .pl(px(7.))
            .pr(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.))
            .rounded(px(10.))
            .bg(bg)
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(fg)
    };
    match (state, tone(state)) {
        (State::Done(added, removed), Some(t)) => div().flex().flex_none().items_center().gap(px(6.)).child(glyph(id, t)).child(diffstat(added, removed)),
        (_, Some(t)) => pill(t.bg, t.text).child(glyph(id, t)).child(word(state)),
        (State::Sent, _) => pill(FILL_3, TEXT_2).child(icon("check", 11., TEXT_2)).child("Sent"),
        (State::Draft, _) => pill(ACCENT_BG, ACCENT).child(dot(6., ACCENT)).child("Draft"),
        (State::Idle(added, removed), _) => diffstat(added, removed),
        _ => pill(FILL_3, TEXT_2).child("Not attached"),
    }
}

pub fn tag(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(10.))
        .bg(SURFACE)
        .shadow(vec![ring(SEPARATOR_STRONG, 0.5)])
        .text_size(px(11.5))
        .text_color(TEXT_2)
        .whitespace_nowrap()
        .child(label.into())
}

/// One chip per space-separated key, so "⌘ K" renders as ⌘ and K.
pub fn kbd(keys: &str) -> Div {
    div().flex().flex_none().gap(px(4.)).children(keys.split_whitespace().map(|k| {
        div().px(px(6.)).py(px(1.)).rounded(px(5.)).bg(FILL_4).font_family(MONO).text_size(px(11.)).line_height(px(16.)).text_color(TEXT_2).child(k.to_string())
    }))
}

/// One letter for a repository, taken from its last dash-separated word: "app-ios" is I.
fn mark_letter(name: &str) -> String {
    let word = name.rsplit('-').find(|w| !w.is_empty()).unwrap_or(name);
    word.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

pub fn repo_mark(name: &str, selected: bool, state: Option<State>) -> Div {
    repo_tile(&mark_letter(name), 22., selected, state)
}

/// The color that marks a state asking for a look: Needs you, Failed or Done.
pub fn alert_color(state: State) -> Option<Token> {
    tone(state).filter(|t| t.glyph != Glyph::Spinner).map(|t| t.mark)
}

/// A repository's initials on a square tile, dotted top-right when it asks for a look and bottom-right while it works.
pub fn repo_tile(letters: &str, size: f32, selected: bool, state: Option<State>) -> Div {
    let badge = |d: Div, color: Token| d.absolute().right(px(-2.)).size(px(9.)).rounded(px(5.)).bg(color).shadow(vec![ring(CUTOUT, 2.)]);
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
        .when(selected, |d| d.bg(TEXT).text_color(ON_TEXT))
        .when(!selected, |d| {
            d.bg(SURFACE).text_color(TEXT_2).shadow(vec![BoxShadow { inset: true, ..ring(SEPARATOR_STRONG, 0.5) }])
        })
        .child(letters.to_string())
        .children(state.and_then(alert_color).map(|c| badge(div().top(px(-2.)), c)))
        .when(state == Some(State::Working), |d| d.child(badge(div().bottom(px(-2.)), ACCENT)))
}

const ROW_GROUP: &str = "sidebar-row";

fn sidebar_row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .group(ROW_GROUP)
        .h(px(32.))
        .pr(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(9.))
        .cursor_pointer()
        .when(selected, |d| d.bg(FILL_4))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
}

/// `kept` is false for a project Pocket shows only while it has terminals: its mark is dashed and its name dim.
pub fn repo_row(id: impl Into<ElementId>, lead: impl IntoElement, name: &str, selected: bool, kept: bool) -> Stateful<Div> {
    let mark = if kept {
        repo_mark(name, false, None)
    } else {
        div()
            .size(px(22.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border(px(1.))
            .border_dashed()
            .border_color(TEXT_5)
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_3)
            .child(mark_letter(name))
    };
    sidebar_row(id, selected)
        .pl(px(4.))
        .text_size(px(14.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(lead)
        .child(mark)
        .child(div().flex_1().min_w_0().truncate().when(!kept, |d| d.text_color(TEXT_3)).child(name.to_string()))
}

pub fn chevron(id: impl Into<ElementId>, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(14.))
        .h(px(20.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .hover(|s| s.bg(FILL_3))
        .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
}

/// A row's status mark: its status's glyph.
pub fn indicator(id: impl Into<ElementId>, state: Option<State>) -> Option<AnyElement> {
    tone(state?).map(|t| glyph(id, t))
}

pub fn setting_up(id: impl Into<ElementId>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .text_color(TEXT_4)
        .child(spinner(id, 11., TEXT_4))
        .child("Setting up…")
}

/// A sidebar row's right end: its status mark, swapped for its buttons while the row is hovered or its menu is open.
pub fn row_trail(mark: Option<AnyElement>, buttons: Vec<AnyElement>, open: bool) -> Div {
    let width = buttons.len() as f32 * 24. - 2.;
    div()
        .relative()
        .h_full()
        .min_w(px(width))
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .children(mark.map(|m| div().flex().items_center().map(|d| if open { d.opacity(0.) } else { d.group_hover(ROW_GROUP, |s| s.opacity(0.)) }).child(m)))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .flex()
                .items_center()
                .gap(px(2.))
                .when(!open, |d| d.opacity(0.).group_hover(ROW_GROUP, |s| s.opacity(1.)))
                .children(buttons),
        )
}

/// The accent line a dragged row lands on: under the row when moving down, over it when moving up.
pub fn drop_line(below: bool) -> Vec<BoxShadow> {
    vec![BoxShadow { inset: true, ..shadow(ACCENT, if below { -2. } else { 2. }, 0.) }]
}

pub fn provider_label(provider: &str, faded: bool) -> Div {
    div().flex().flex_none().items_center().gap(px(6.)).when(faded, |d| d.opacity(0.5)).child(dot(6., provider_color(provider))).child(provider_name(provider))
}

/// The group of a session row; its time hides while the row is hovered, making room for a caller's hover controls.
pub const SESSION_ROW: &str = "session-row";

/// A session's card. Needs you fills the whole row; hover and selection tint over that fill.
#[allow(clippy::too_many_arguments)]
pub fn session_row(id: impl Into<ElementId>, selected: bool, lead: impl IntoElement, when: impl IntoElement, title: String, notice: Option<&'static str>, branch: Option<String>, state: Option<State>) -> Stateful<Div> {
    let id = id.into();
    let line = || div().h(px(16.)).flex().items_center().gap(px(8.)).text_size(px(12.)).text_color(TEXT_2);
    div()
        .id(id.clone())
        .group(SESSION_ROW)
        .relative()
        .px(px(10.))
        .py(px(10.))
        .flex()
        .flex_none()
        .flex_col()
        .gap(px(4.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(state == Some(State::NeedsYou), |d| d.bg(WAITING_BG))
        .child(div().absolute().inset_0().rounded(px(8.)).map(|d| if selected { d.bg(FILL_3) } else { d.group_hover(SESSION_ROW, |s| s.bg(FILL_1)) }))
        .child(line().child(div().flex_1().min_w_0().flex().child(lead)).child(div().group_hover(SESSION_ROW, |s| s.invisible()).child(when)))
        .child(div().truncate().text_size(px(14.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title))
        .children(notice.map(|n| div().truncate().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_2).child(n)))
        .child(
            line()
                .child(div().flex_1().min_w_0().flex().items_center().gap(px(6.)).when_some(branch, |d, b| d.child(icon("branch", 12., TEXT_2)).child(div().truncate().child(b))))
                .children(state.map(|s| status_label(id, s))),
        )
}

/// The "⌘n" chip a session row shows in place of its age while ⌘ is held.
pub fn jump_chip(n: usize) -> Div {
    div().h(px(16.)).px(px(4.)).flex().items_center().rounded(px(4.)).bg(FILL_4).font_family(MONO).text_size(px(10.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(format!("⌘{n}"))
}

/// A card's status as coloured text: the pill's glyph and label without its background.
pub fn status_label(id: impl Into<ElementId>, state: State) -> Div {
    let label = |color: Token| div().flex().flex_none().items_center().gap(px(5.)).font_weight(FontWeight::MEDIUM).text_color(color);
    match (state, tone(state)) {
        (State::Done(added, removed), Some(t)) => label(t.text).child(glyph(id, t)).child(word(state)).child(diffstat(added, removed)),
        (_, Some(t)) => label(t.text).child(glyph(id, t)).child(word(state)),
        (State::NotAttached, _) => label(TEXT_2).child(word(state)),
        _ => status(id, state),
    }
}

/// Git's one-letter status for a file: M, A or D.
pub fn git_color(letter: char) -> Token {
    match letter {
        'A' => SUCCESS_TEXT,
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
        .when(selected, |d| d.bg(ACCENT_BG).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.font_weight(FontWeight(450.)).hover(|s| s.bg(FILL_2)))
        .child(div().w(px(12.)).flex().when(folder, |d| d.child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))))
        .child(file_icon(&label, folder, open, 16.))
        .child(div().flex_1().truncate().child(label))
        .when(touched, |d| d.child(dot(6., AGENT_CODEX)))
        .children(git.map(|g| div().font_family(MONO).text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(git_color(g)).child(g.to_string())))
}

pub fn checkbox(on: bool) -> Div {
    div()
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .when(on, |d| d.bg(TEXT).child(icon("check", 11., ON_TEXT)))
        .when(!on, |d| d.bg(SURFACE).shadow(vec![ring(SEPARATOR_STRONG, 1.)]))
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
        .when(selected, |d| d.bg(FILL_3))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_1)))
        .child(check)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().truncate().text_size(px(13.5)).font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM }).child(file.to_string()))
                .when(!dir.is_empty(), |d| d.child(div().truncate().font_family(MONO).text_size(px(11.)).text_color(TEXT_4).child(dir.to_string()))),
        )
        .when(comments > 0, |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(WAITING_TEXT)
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
        .border_color(Token::new(0x00000040, 0xebebeb40))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2))
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
        .bg(WAITING)
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .text_color(WHITE)
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
        .bg(TEXT)
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(ON_TEXT)
        .child(initials.to_string())
}

/// Crumbs separated by slashes; the last one is the current page and truncates only once the others have. Past that, the first crumbs clip.
pub fn breadcrumb(crumbs: Vec<String>) -> Div {
    let last = crumbs.len().saturating_sub(1);
    div().flex().min_w_0().overflow_hidden().justify_end().items_center().gap(px(8.)).text_size(px(13.)).whitespace_nowrap().children(crumbs.into_iter().enumerate().flat_map(|(i, c)| {
        let crumb = div()
            .truncate()
            .when(i == last, |d| d.min_w(px(60.)).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
            .when(i != last, |d| d.min_w(px(24.)).flex_shrink(100.).text_color(TEXT_2).font_weight(FontWeight(450.)))
            .child(c);
        let slash = (i > 0).then(|| div().flex_none().text_color(TEXT_6).child("/"));
        slash.into_iter().chain([crumb])
    }))
}

pub fn page_bar() -> Div {
    div().h(px(42.)).pl(px(14.)).pr(px(12.)).flex().flex_none().items_center().gap(px(8.))
}

pub fn meta_item() -> Div {
    div().h(px(22.)).flex().flex_none().items_center().gap(px(6.)).text_size(px(12.5)).text_color(TEXT_2).whitespace_nowrap()
}

/// Meta items in the room the breadcrumb leaves; ones that don't fit wrap onto a clipped second line, so none shows half cut.
pub fn meta_row(items: Vec<AnyElement>) -> Div {
    // The empty lead holds the first line, so even the first item can wrap away.
    div().ml(px(8.)).flex_1().min_w_0().h(px(22.)).flex().flex_wrap().items_center().overflow_hidden().child(div()).children(items.into_iter().enumerate().map(|(i, item)| {
        div().flex().flex_none().items_center().gap(px(10.)).when(i > 0, |d| d.pl(px(10.)).child(dot(3., TEXT_6))).child(item)
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
        .child(div().text_color(SUCCESS_TEXT).child(format!("+{added}")))
        .child(div().text_color(FAILED).child(format!("−{removed}")))
}

pub fn meta_value(text: impl Into<SharedString>) -> Div {
    div().text_color(TEXT).font_weight(FontWeight::MEDIUM).child(text.into())
}

/// How much of the context window is left, drawn as a 32px bar.
pub fn context_bar(left: f32) -> Div {
    let used = (1. - left).clamp(0., 1.);
    div().w(px(32.)).h(px(4.)).rounded(px(2.)).bg(SEPARATOR_STRONG).child(div().h_full().w(px(32. * used)).rounded(px(2.)).bg(if used > 0.8 { WAITING } else { TEXT }))
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
        .line_height(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(label.into())
        .children(count.map(|n| div().font_weight(FontWeight::MEDIUM).child(n.to_string())))
}

pub fn swatch(color: u32, size: f32, radius: f32) -> Div {
    div().size(px(size)).flex_none().rounded(px(radius)).bg(rgba(color))
}

pub fn field_label(label: impl Into<SharedString>) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(label.into())
}

pub fn field_box() -> Div {
    div()
        .h(px(38.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(11.))
        .bg(Token::new(0xffffffd9, 0xebebeb14))
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
            .rounded(px(R_DIALOG))
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

pub fn palette_row(id: impl Into<ElementId>, selected: bool, lead: impl IntoElement, title: impl IntoElement, detail: impl IntoElement, keys: Option<&str>) -> Stateful<Div> {
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
        .when(selected, |d| d.bg(ACCENT_BG))
        .child(div().size(px(24.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_3).child(lead))
        .child(div().flex_none().max_w(px(320.)).truncate().font_weight(FontWeight::MEDIUM).child(title))
        .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT_2).child(detail))
        .children(keys.map(kbd))
}

fn menu_item(id: impl Into<ElementId>, icon_name: &str, label: &str, tint: Token, hover: Token) -> Stateful<Div> {
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
        .hover(move |s| s.bg(hover))
        .child(icon(icon_name, 14., tint))
        .child(div().flex_1().child(label.to_string()))
}

pub fn menu_row(id: impl Into<ElementId>, icon_name: &str, label: &str, keys: Option<&str>) -> Stateful<Div> {
    menu_item(id, icon_name, label, TEXT_2, FILL_2).children(keys.map(kbd))
}

/// A menu row that destroys something; menus keep it last, behind a divider.
pub fn danger_row(id: impl Into<ElementId>, icon_name: &str, label: &str) -> Stateful<Div> {
    menu_item(id, icon_name, label, FAILED, FAILED_BG).text_color(FAILED)
}

pub fn menu_divider() -> Div {
    div().h(px(0.5)).mx(px(8.)).my(px(4.)).flex_none().bg(SEPARATOR)
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
        .when(selected, |d| d.bg(FILL_4).text_color(TEXT))
        .when(!selected, |d| d.text_color(TEXT_2).hover(|s| s.bg(FILL_2)))
}

pub fn link(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex_none().cursor_pointer().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(ACCENT).whitespace_nowrap().child(label.into())
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
        .bg(Token::new(0xffffffb3, 0xebebeb0d))
        .shadow(vec![ring(HAIRLINE, 0.5)])
        .cursor_pointer()
        .text_size(px(13.5))
        .text_color(TEXT_2)
        .child(icon(icon_name, 15., TEXT_3))
        .child(div().flex_1().child(label.to_string()))
        .child(div().text_size(px(11.5)).text_color(TEXT_2).child(keys.to_string()))
}

/// Indented past the project row's chevron (4 + 14 + gap 7) so its glyph sits under the project's mark and its name under the project's.
pub fn worktree_row(id: impl Into<ElementId>, name: String, selected: bool) -> Stateful<Div> {
    sidebar_row(id, selected)
        .pl(px(25.))
        .text_size(px(13.5))
        .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight(450.) })
        .text_color(if selected { TEXT } else { TEXT_BODY })
        .child(div().w(px(22.)).flex().flex_none().justify_center().child(icon("worktree", 13., if selected { TEXT_2 } else { TEXT_4 })))
        .child(div().flex_1().min_w_0().truncate().child(name))
}

#[cfg(test)]
mod tests {
    use super::{Glyph, State, Tone, tone, word};
    use theme::{ACCENT, ACCENT_TINT};

    const STATUSES: [State; 4] = [State::NeedsYou, State::Working, State::Done(0, 0), State::Failed];

    #[test]
    fn every_alerting_state_has_its_own_glyph_and_colour() {
        let tones: Vec<Tone> = STATUSES.into_iter().filter_map(tone).collect();
        assert_eq!(tones.len(), 4);
        for (i, a) in tones.iter().enumerate() {
            for b in &tones[i + 1..] {
                assert_ne!(a.glyph, b.glyph);
                assert_ne!(a.mark, b.mark);
            }
        }
        assert!([State::Idle(0, 0), State::NotAttached, State::Sent, State::Draft].into_iter().all(|s| tone(s).is_none()));
    }

    #[test]
    fn working_reads_working_in_accent() {
        assert_eq!(tone(State::Working), Some(Tone { glyph: Glyph::Spinner, mark: ACCENT, text: ACCENT, bg: ACCENT_TINT }));
        assert_eq!(word(State::Working), "Working");
    }
}
