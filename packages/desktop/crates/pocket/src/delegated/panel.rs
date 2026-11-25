use super::shown;
use crate::chat::sub_agent::{self, Mark};
use crate::chat::user_bubble::text_lines;
use crate::desktop::{Desktop, ReduceTransparency};
use crate::glide::Glide;
use crate::util::now_ms;
use agents::delegations::{Counts, Delegation, Headline, Phase};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// How long the panel stays open once every session has ended.
const COLLAPSE_AFTER: Duration = Duration::from_millis(3500);

const FULL_W: f32 = 312.;
const CAPSULE_H: f32 = 34.;
const HEAD_H: f32 = 36.;
const ROW_H: f32 = 48.;
const TOP: f32 = 12.;
const BOTTOM: f32 = 12.;
const SECTION_H: f32 = 28.;
const FOLD_H: f32 = 34.;
const RULE_H: f32 = 4.5;
const PINNED_PAD: f32 = 4.;
const FOLD_ABOVE: usize = 6;
const QUESTION_SIZE: f32 = 12.5;
const QUESTION_LINE: f32 = 17.;
const QUESTION_LINES: usize = 2;
/// A question runs under its row's title: the panel less the rows' and the row's padding, the logo and its gap.
const QUESTION_W: f32 = FULL_W - 12. - 16. - 26.;

/// How long the list takes to open and the panel to appear, in seconds.
const SPREAD_RESPONSE: f32 = 0.38;
const SHOW_RESPONSE: f32 = 0.32;
/// The size the panel starts from as it appears.
const ENTER_SCALE: f32 = 0.9;

/// The panel's glass: the prototype's 72% tint over a 24 px blur.
const GLASS: Token = Token::new(0xffffffb8, 0x2a2a2ab8);
const GLASS_BLUR: f32 = 24.;
/// The half-pixel light along the glass's top edge.
const GLASS_HIGHLIGHT: Token = Token::new(0xffffffe6, 0xffffff0f);

/// What the panel saw of its chat's delegations last, to tell what's new.
#[derive(Default)]
pub(super) struct Watch {
    total: usize,
    asking: usize,
    settled: bool,
}

#[derive(Debug, PartialEq)]
pub(super) enum Act {
    Nothing,
    Open,
    CollapseSoon,
}

impl Watch {
    pub(super) fn observe(&mut self, c: Counts) -> Act {
        let settled = c.total > 0 && c.starting + c.working + c.asking == 0;
        let grew = c.total > self.total || c.asking > self.asking;
        let just_settled = settled && !self.settled;
        (self.total, self.asking, self.settled) = (c.total, c.asking, settled);
        if grew && !settled {
            Act::Open
        } else if just_settled {
            Act::CollapseSoon
        } else {
            Act::Nothing
        }
    }
}

pub(super) struct Panel {
    spread: Glide,
    shown: Glide,
    open: bool,
    leaving: bool,
    watch: Watch,
    collapse: Option<Task<()>>,
}

impl Panel {
    pub(super) fn new() -> Self {
        Self {
            spread: Glide::from(0.).response(SPREAD_RESPONSE),
            shown: Glide::from(0.).response(SHOW_RESPONSE),
            open: false,
            leaving: false,
            watch: Watch::default(),
            collapse: None,
        }
    }

    /// Where the list and the panel as a whole are heading.
    fn targets(&self) -> (f32, f32) {
        (f32::from(u8::from(self.open)), if self.leaving { 0. } else { 1. })
    }

    /// An open panel shuts when the chat leaves its end. Whether it shut.
    fn scrolled(&mut self, away: bool) -> bool {
        let shut = self.open && away;
        if shut {
            (self.open, self.collapse) = (false, None);
        }
        shut
    }

    /// A press outside an open panel shuts it to its capsule, which stays. Whether it shut.
    fn pressed_outside(&mut self) -> bool {
        self.scrolled(true)
    }

    /// Steps the list and the panel toward their targets; true while either still moves.
    fn step(&mut self, now: Instant, still: bool) -> bool {
        let (spread_to, shown_to) = self.targets();
        [self.spread.tick(spread_to, now, still), self.shown.tick(shown_to, now, still)].contains(&true)
    }

    fn leave(&mut self) {
        (self.leaving, self.collapse) = (true, None);
    }

    /// Returns whether to start the timer that shuts it.
    fn changed(&mut self, c: Counts) -> bool {
        match self.watch.observe(c) {
            Act::Open => {
                (self.open, self.leaving, self.collapse) = (true, false, None);
                false
            }
            Act::CollapseSoon => true,
            Act::Nothing => false,
        }
    }

    fn gone(&self, moving: bool) -> bool {
        self.leaving && !moving
    }
}

/// A capture draws one frame a step, which would catch the panel midway between capsule and list.
fn still(reduce_motion: bool, capturing: bool) -> bool {
    reduce_motion || capturing
}

#[derive(Debug, PartialEq)]
enum Material {
    Glass,
    Solid,
}

/// A blur only the Metal renderer draws, and not for a user who asked for less transparency.
fn material(reduce_transparency: bool, macos: bool) -> Material {
    if reduce_transparency || !macos { Material::Solid } else { Material::Glass }
}

fn shell(material: Material) -> Div {
    let shell = div().rounded(px(R_POPOVER)).shadow(vec![
        BoxShadow { inset: true, ..ui::shadow(GLASS_HIGHLIGHT, 0.5, 0.) },
        ui::ring(SEPARATOR, 0.5),
        ui::shadow(SHADOW, 18., 50.),
        ui::shadow(rgba(0x0000000f), 2., 6.),
    ]);
    match material {
        Material::Glass => shell.bg(GLASS).backdrop_blur(px(GLASS_BLUR)),
        Material::Solid => shell.bg(POPOVER),
    }
}

/// How much of the capsule shows while the list opens: gone by the time it is under half open.
fn capsule_opacity(spread: f32) -> f32 {
    (1. - spread * 2.2).clamp(0., 1.)
}

/// How much of the list shows: nothing until a third open.
fn list_opacity(spread: f32) -> f32 {
    ((spread - 0.35) / 0.65).clamp(0., 1.)
}

pub(super) fn headline(c: &Counts) -> String {
    match c.headline() {
        Headline::Asking(n) => format!("{n} needs you"),
        Headline::Working(n) => format!("{n} working"),
        Headline::Settled { done, failed: 0 } => format!("{done} done"),
        Headline::Settled { done, failed } => format!("{done} done, {failed} failed"),
    }
}

pub(super) fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    match s / 3600 {
        0 => format!("{}:{:02}", s / 60, s % 60),
        h => format!("{h}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

fn label(p: Phase) -> &'static str {
    match p {
        Phase::Starting => "Starting",
        Phase::Working => "Working",
        Phase::Asking => "Needs you",
        Phase::Done => "Done",
        Phase::Failed => "Failed",
        Phase::Stopped => "Stopped",
    }
}

fn row_height(d: &Delegation, lines: &impl Fn(&Delegation) -> usize) -> f32 {
    match question(d) {
        Some(_) => ROW_H + 6. + QUESTION_LINE * lines(d).clamp(1, QUESTION_LINES) as f32,
        None => ROW_H,
    }
}

struct Groups<'a> {
    asking: Vec<&'a Entry>,
    running: Vec<&'a Entry>,
    ended: Vec<&'a Entry>,
    folds: bool,
}

fn groups(entries: &[Entry]) -> Groups<'_> {
    let mut g = Groups { asking: Vec::new(), running: Vec::new(), ended: Vec::new(), folds: false };
    for e in entries {
        match e.delegation.phase() {
            Phase::Asking => g.asking.push(e),
            p if p.settled() => g.ended.push(e),
            _ => g.running.push(e),
        }
    }
    g.running.sort_by_key(|e| e.delegation.created_at);
    g.ended.sort_by_key(|e| std::cmp::Reverse(e.delegation.settled_at));
    g.folds = entries.len() > FOLD_ABOVE && g.ended.len() < entries.len();
    g
}

impl Groups<'_> {
    fn asking_head(&self) -> bool {
        self.asking.len() > 1
    }

    fn running_head(&self) -> bool {
        !self.running.is_empty() && !(self.asking.is_empty() && self.ended.is_empty())
    }

    fn ended_head(&self) -> bool {
        !self.folds && !self.ended.is_empty() && !(self.asking.is_empty() && self.running.is_empty())
    }

    fn fold_index(&self) -> usize {
        usize::from(self.running_head()) + self.running.len()
    }

    fn rule(&self) -> bool {
        !self.asking.is_empty() && !(self.running.is_empty() && self.ended.is_empty())
    }

    fn heights(&self, ended_open: bool, lines: impl Fn(&Delegation) -> usize) -> Heights {
        let rows = |list: &[&Entry]| list.iter().map(|e| row_height(&e.delegation, &lines)).sum::<f32>();
        let head = |shown: bool| if shown { SECTION_H } else { 0. };
        let pinned = if self.asking.is_empty() { 0. } else { head(self.asking_head()) + rows(&self.asking) + PINNED_PAD };
        let ended = match (self.ended.is_empty(), self.folds) {
            (true, _) => 0.,
            (false, true) => FOLD_H + if ended_open { rows(&self.ended) } else { 0. },
            (false, false) => head(self.ended_head()) + rows(&self.ended),
        };
        let body = head(self.running_head()) + rows(&self.running) + ended + 6.;
        Heights { pinned, full: HEAD_H + pinned + if self.rule() { RULE_H } else { 0. } + body }
    }
}

/// The pinned questions and the whole list, each laid out in full.
#[derive(Clone, Copy, Default)]
struct Heights {
    pinned: f32,
    full: f32,
}

impl Heights {
    /// The questions keep all they need while the rest still fits, and half the panel once it doesn't.
    fn pinned_in(self, open: f32) -> f32 {
        self.pinned.min((open / 2.).max(open - (self.full - self.pinned)))
    }
}

fn open_height(full: f32, chat_h: Option<f32>) -> f32 {
    chat_h.map_or(full, |h| full.min((h - TOP - BOTTOM).max(CAPSULE_H)))
}

/// What a session asks, while it waits for the answer.
fn question(d: &Delegation) -> Option<&str> {
    (d.phase() == Phase::Asking).then_some(d.question.as_str())
}

fn capsule_width(logos: usize, text: &str) -> f32 {
    let logos = 20. + 15. * logos.saturating_sub(1) as f32;
    7. + logos + 8. + 19. + text.chars().count() as f32 * 6.9 + 12.
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn logos<'a>(list: impl Iterator<Item = &'a Delegation>) -> Vec<&'a str> {
    let mut seen: Vec<&str> = Vec::new();
    for d in list {
        if !seen.contains(&d.agent.as_str()) {
            seen.push(&d.agent);
        }
    }
    seen.truncate(3);
    seen
}

fn glyph(id: &str, p: Phase) -> AnyElement {
    match p {
        Phase::Starting => sub_agent::glyph(id, Mark::Starting),
        Phase::Working => sub_agent::glyph(id, Mark::Running),
        Phase::Asking => div().size(px(14.)).flex_none().flex().items_center().justify_center().child(ui::dot(7., WAITING_DOT)).into_any_element(),
        Phase::Done => sub_agent::glyph(id, Mark::Done),
        Phase::Failed => sub_agent::glyph(id, Mark::Failed),
        Phase::Stopped => icon("status-stopped", 14., TEXT_4).into_any_element(),
    }
}

pub(super) struct Entry {
    delegation: Delegation,
    down: Option<String>,
}

pub(crate) enum PanelEvent {
    Open(String),
    Gone,
}

pub(crate) struct PanelView {
    panel: Panel,
    entries: Vec<Entry>,
    capturing: bool,
    chat_h: Option<f32>,
    ended_open: bool,
    list: ScrollHandle,
}

impl EventEmitter<PanelEvent> for PanelView {}

impl PanelView {
    pub(super) fn new() -> Self {
        Self { panel: Panel::new(), entries: Vec::new(), capturing: false, chat_h: None, ended_open: false, list: ScrollHandle::new() }
    }

    pub(super) fn set<'a>(&mut self, list: impl Iterator<Item = (&'a Delegation, Option<String>)> + Clone, capturing: bool) {
        self.capturing = capturing;
        let same =
            self.entries.len() == list.clone().count() && self.entries.iter().zip(list.clone()).all(|(e, (d, down))| e.delegation == *d && e.down == down);
        if !same {
            self.entries = list.map(|(d, down)| Entry { delegation: d.clone(), down }).collect();
        }
    }

    pub(super) fn changed(&mut self, c: Counts, cx: &mut Context<Self>) {
        if !self.panel.changed(c) {
            return;
        }
        self.panel.collapse = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COLLAPSE_AFTER).await;
            this.update(cx, |v, cx| {
                v.panel.open = false;
                cx.notify();
            })
            .ok();
        }));
    }

    pub(super) fn scrolled(&mut self, away: bool, cx: &mut Context<Self>) {
        if self.panel.scrolled(away) {
            cx.notify();
        }
    }

    pub(super) fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        (self.panel.open, self.panel.collapse) = (open, None);
        cx.notify();
    }

    fn pressed_outside(&mut self, cx: &mut Context<Self>) {
        if self.panel.pressed_outside() {
            cx.notify();
        }
    }

    fn toggle_ended(&mut self, fold_index: usize, cx: &mut Context<Self>) {
        self.ended_open = !self.ended_open;
        if self.ended_open {
            self.list.scroll_to_top_of_item(fold_index);
        }
        cx.notify();
    }

    fn measured(&mut self, chat_h: f32, cx: &mut Context<Self>) {
        if self.chat_h.is_none_or(|h| (h - chat_h).abs() > 0.5) {
            self.chat_h = Some(chat_h);
            crate::region::notify_after_draw(cx);
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.panel.leave();
        cx.notify();
    }
}

impl Render for PanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let reduce_transparency = cx.try_global::<ReduceTransparency>().is_some_and(|r| r.0);
        let still = still(cx.reduce_motion(), self.capturing);
        let moving = self.panel.step(Instant::now(), still);
        if self.panel.gone(moving) {
            cx.emit(PanelEvent::Gone);
            return div().absolute();
        }
        if moving {
            window.request_animation_frame();
        }
        let list = self.entries.iter().map(|e| &e.delegation);
        let counts = Counts::of(list.clone());
        let settled = counts.settled();
        let (spread, appeared) = (self.panel.spread.at().clamp(0., 1.), self.panel.shown.at().clamp(0., 1.));
        let summary = headline(&counts);
        let logos = logos(list.clone());
        let cap_w = capsule_width(logos.len(), &summary);
        let g = groups(&self.entries);
        let heights = if spread > 0. { g.heights(self.ended_open, |d| text_lines(&d.question, QUESTION_SIZE, QUESTION_W, window)) } else { Heights::default() };
        let open_h = open_height(heights.full, self.chat_h);
        let scale = lerp(ENTER_SCALE, 1., appeared);
        let capsule = (capsule_opacity(spread) > 0.).then(|| capsule(list, &counts, logos, summary, spread < 0.5, cx).opacity(capsule_opacity(spread)));
        let layout = Layout { open_h, pinned_h: heights.pinned_in(open_h), ended_open: self.ended_open };
        let full = (list_opacity(spread) > 0.).then(|| full(&g, layout, &self.list, counts.total, settled, spread >= 0.5, cx).opacity(list_opacity(spread)));
        let material = material(reduce_transparency, cfg!(target_os = "macos"));
        let this = cx.weak_entity();
        let measure = canvas(
            move |b, _, cx| {
                this.update(cx, |v, cx| v.measured(f32::from(b.size.height), cx)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div().absolute().inset_0().child(measure).child(
            shell(material)
                .absolute()
                .top(px(TOP))
                .right(px(16.))
                .occlude()
                .overflow_hidden()
                .opacity(appeared)
                .w(px(lerp(cap_w, FULL_W, spread) * scale))
                .h(px(lerp(CAPSULE_H, open_h, spread) * scale))
                .rounded(px(lerp(CAPSULE_H / 2., 15., spread) * scale))
                .when(self.panel.open, |d| d.on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.pressed_outside(cx))))
                .children(capsule)
                .children(full),
        )
    }
}

impl Desktop {
    fn delegated_view(&mut self, parent: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<PanelView> {
        if let Some(slot) = self.delegated.panels.get(parent) {
            return slot.view.clone();
        }
        let view = cx.new(|_| PanelView::new());
        let id = parent.to_string();
        let sub = cx.subscribe_in(&view, window, move |this, _, ev, window, cx| this.on_delegated_panel(&id, ev, window, cx));
        self.delegated.panels.insert(parent.to_string(), super::Slot { view: view.clone(), _sub: sub });
        view
    }

    fn on_delegated_panel(&mut self, parent: &str, ev: &PanelEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PanelEvent::Open(agent) => self.delegated_open(agent, window, cx),
            PanelEvent::Gone => {
                self.delegated.clear_settled(self.agents.delegations.of(parent));
                self.delegated.panels.remove(parent);
                cx.notify();
            }
        }
    }

    pub(crate) fn delegated_changed(&mut self, parent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let counts = Counts::of(shown(self.agents.delegations.of(parent), &self.delegated.cleared));
        let view = self.delegated_view(parent, window, cx);
        view.update(cx, |v, cx| v.changed(counts, cx));
        cx.notify();
    }

    pub(crate) fn delegated_scrolled(&mut self, parent: &str, away: bool, cx: &mut Context<Self>) {
        if let Some(slot) = self.delegated.panels.get(parent) {
            slot.view.update(cx, |v, cx| v.scrolled(away, cx));
        }
    }

    pub(crate) fn delegated_set_open(&mut self, parent: &str, open: bool, cx: &mut Context<Self>) {
        if let Some(slot) = self.delegated.panels.get(parent) {
            slot.view.update(cx, |v, cx| v.set_open(open, cx));
        }
        cx.notify();
    }

    pub(crate) fn delegated_panel(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<PanelView>> {
        shown(self.agents.delegations.of(agent), &self.delegated.cleared).next()?;
        let view = self.delegated_view(agent, window, cx);
        let hosts = &self.fleet.hosts;
        let list = shown(self.agents.delegations.of(agent), &self.delegated.cleared).map(|d| (d, super::unreachable_host(d, &hosts.list, &hosts.links)));
        let capturing = self.capturing;
        view.update(cx, |v, _| v.set(list, capturing));
        Some(view)
    }
}

fn capsule<'a>(
    list: impl Iterator<Item = &'a Delegation> + Clone,
    counts: &Counts,
    logos: Vec<&str>,
    summary: String,
    live: bool,
    cx: &mut Context<PanelView>,
) -> Stateful<Div> {
    let shown = list
        .clone()
        .find(|d| d.phase() == Phase::Asking)
        .or_else(|| list.clone().find(|d| !d.phase().settled()))
        .or(list.last())
        .map_or(Phase::Working, Delegation::phase);
    let colour = if counts.asking > 0 { WAITING_TEXT } else { TEXT_2 };
    let capsule = div()
        .id("delegated-capsule")
        .absolute()
        .top_0()
        .right_0()
        .h(px(CAPSULE_H))
        .pl(px(7.))
        .pr(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .whitespace_nowrap()
        .text_size(px(13.))
        .child(div().flex().children(logos.into_iter().enumerate().map(|(i, agent)| {
            div()
                .size(px(20.))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(FILL_4)
                .border_2()
                .border_color(CUTOUT)
                .when(i > 0, |d| d.ml(px(-5.)))
                .relative()
                .child(provider_icon(agent, 10., TEXT))
        })))
        .child(div().flex().items_center().gap(px(6.)).text_color(colour).child(glyph("delegated-capsule", shown)).child(summary));
    if live { capsule.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.set_open(true, cx))) } else { capsule }
}

#[derive(Clone, Copy)]
struct Layout {
    open_h: f32,
    pinned_h: f32,
    ended_open: bool,
}

fn section(label: &'static str, n: usize, colour: Token) -> Div {
    div()
        .h(px(SECTION_H))
        .flex_none()
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colour)
        .child(label)
        .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_4).child(n.to_string()))
}

fn fold(g: &Groups, open: bool, live: bool, cx: &mut Context<PanelView>) -> Stateful<Div> {
    let (ended, ix) = (&g.ended, g.fold_index());
    let failed = ended.iter().filter(|e| e.delegation.phase() == Phase::Failed).count();
    let mark = if failed > 0 { Phase::Failed } else { Phase::Done };
    let chevron = icon("chevron-right", 12., TEXT_4).when(open, |i| i.with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2))));
    div()
        .id("delegated-fold")
        .h(px(FOLD_H))
        .flex_none()
        .px(px(8.))
        .rounded(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.5))
        .text_color(TEXT_2)
        .child(glyph("delegated-fold", mark))
        .when(failed < ended.len(), |d| d.child(format!("{} done", ended.len() - failed)))
        .when(failed > 0 && failed < ended.len(), |d| d.child(div().text_color(FAILED_TEXT).child("\u{b7}")))
        .when(failed > 0, |d| d.child(div().text_color(FAILED_TEXT).child(format!("{failed} failed"))))
        .child(div().flex_1())
        .child(chevron)
        .when(live, |d| {
            d.cursor_pointer().hover(|s| s.bg(FILL_2)).active(|s| s.bg(FILL_3)).on_click(cx.listener(move |this, _, _, cx| this.toggle_ended(ix, cx)))
        })
}

fn full(g: &Groups, layout: Layout, list: &ScrollHandle, total: usize, settled: bool, live: bool, cx: &mut Context<PanelView>) -> Div {
    let now = now_ms();
    let button = |id: &'static str| {
        div().id(id).h(px(24.)).px(px(8.)).rounded(px(7.)).flex().items_center().text_size(px(12.)).text_color(TEXT_3).hover(|s| s.bg(FILL_3).text_color(TEXT))
    };
    let head = div()
        .h(px(HEAD_H))
        .flex_none()
        .pl(px(14.))
        .pr(px(6.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Delegated"))
        .child(div().text_size(px(12.)).text_color(TEXT_4).child(total.to_string()))
        .child(div().flex_1())
        .when(settled && live, |d| d.child(button("delegated-clear").cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.clear(cx))).child("Clear")))
        .child(
            button("delegated-collapse")
                .w(px(24.))
                .px_0()
                .justify_center()
                .when(live, |d| d.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.set_open(false, cx))))
                .child(icon("chevron-up", 12., TEXT_3)),
        );
    let mut rows = |list: &[&Entry]| list.iter().map(|e| row(e, now, live, cx)).collect::<Vec<_>>();
    let pinned = (!g.asking.is_empty()).then(|| {
        div()
            .id("delegated-asking")
            .flex_none()
            .max_h(px(layout.pinned_h))
            .overflow_y_scroll()
            .px(px(6.))
            .pb(px(PINNED_PAD))
            .flex()
            .flex_col()
            .when(g.asking_head(), |d| d.child(section("Needs you", g.asking.len(), WAITING_TEXT)))
            .children(rows(&g.asking))
    });
    let rule = g.rule().then(|| div().flex_none().h(px(0.5)).mx(px(14.)).mb(px(RULE_H - 0.5)).bg(SEPARATOR));
    let running = rows(&g.running);
    let ended = if g.folds && !layout.ended_open { Vec::new() } else { rows(&g.ended) };
    let body = div()
        .id("delegated-list")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(list)
        .px(px(6.))
        .pb(px(6.))
        .flex()
        .flex_col()
        .when(g.running_head(), |d| d.child(section("Working", g.running.len(), TEXT_3)))
        .children(running)
        .when(g.ended_head(), |d| d.child(section("Ended", g.ended.len(), TEXT_3)))
        .when(g.folds && !g.ended.is_empty(), |d| d.child(fold(g, layout.ended_open, live, cx)))
        .children(ended);
    div().absolute().top_0().right_0().w(px(FULL_W)).h(px(layout.open_h)).flex().flex_col().child(head).children(pinned).children(rule).child(body)
}

fn row(entry: &Entry, now: i64, live: bool, cx: &mut Context<PanelView>) -> Stateful<Div> {
    let d = &entry.delegation;
    let phase = d.phase();
    let down = entry.down.clone();
    let colour = match phase {
        Phase::Asking => WAITING_TEXT,
        Phase::Failed => FAILED_TEXT,
        _ if down.is_some() => WAITING_TEXT,
        _ => TEXT_3,
    };
    let status = down.map_or_else(|| label(phase).to_string(), |host| super::unreachable_label(&host));
    let title = if d.title.is_empty() { "Session".to_string() } else { d.title.clone() };
    let open = d.child_id.clone();
    let opens = live && !d.child_id.is_empty();
    let end = div()
        .size(px(24.))
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .child(div().when(opens, |d| d.group_hover("delegated-row", |s| s.invisible())).child(glyph(&d.task_id, phase)))
        .when(opens, |d| {
            d.child(div().absolute().inset_0().flex().items_center().justify_center().invisible().group_hover("delegated-row", |s| s.visible()).child(icon(
                "chevron-right",
                12.,
                TEXT_3,
            )))
        });
    let top = div()
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().w(px(16.)).flex_none().pt(px(2.)).child(provider_icon(&d.agent, 14., TEXT)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().h(px(18.)).truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
                .child(
                    div()
                        .h(px(16.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(12.))
                        .text_color(TEXT_3)
                        .child(div().min_w_0().truncate().text_color(colour).child(status))
                        .child("\u{b7}")
                        .child(clock(d.elapsed_ms(now))),
                ),
        )
        .child(end);
    let asks = question(d).map(|q| {
        div()
            .ml(px(26.))
            .mt(px(6.))
            .text_size(px(QUESTION_SIZE))
            .line_height(px(QUESTION_LINE))
            .text_color(TEXT_2)
            .line_clamp(QUESTION_LINES)
            .child(q.to_string())
    });
    let row = div()
        .id(SharedString::from(format!("delegated-{}", d.task_id)))
        .group("delegated-row")
        .px(px(8.))
        .py(px(7.))
        .flex()
        .flex_col()
        .rounded(px(9.))
        .child(top)
        .children(asks);
    if opens {
        row.cursor_pointer().hover(|s| s.bg(FILL_2)).active(|s| s.bg(FILL_3)).on_click(cx.listener(move |_, _, _, cx| cx.emit(PanelEvent::Open(open.clone()))))
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Act, BOTTOM, CAPSULE_H, Counts, Delegation, Entry, FOLD_H, HEAD_H, Heights, Material, PINNED_PAD, Panel, QUESTION_LINE, ROW_H, RULE_H, SECTION_H, TOP,
        Watch, capsule_opacity, clock, groups, headline, list_opacity, material, open_height, still,
    };
    use std::time::Instant;

    fn counts(starting: usize, working: usize, asking: usize, done: usize, failed: usize) -> Counts {
        Counts { total: starting + working + asking + done + failed, starting, working, asking, done, failed, stopped: 0 }
    }

    #[test]
    fn the_panel_opens_for_a_new_delegation_or_a_new_question() {
        let mut w = Watch::default();
        assert_eq!(w.observe(counts(1, 0, 0, 0, 0)), Act::Open);
        assert_eq!(w.observe(counts(0, 1, 0, 0, 0)), Act::Nothing);
        assert_eq!(w.observe(counts(0, 2, 0, 0, 0)), Act::Open);
        assert_eq!(w.observe(counts(0, 1, 1, 0, 0)), Act::Open);
        assert_eq!(w.observe(counts(0, 2, 0, 0, 0)), Act::Nothing, "an answered question is no news");
    }

    #[test]
    fn the_panel_shuts_soon_after_the_last_session_ends_and_not_for_a_list_that_began_settled() {
        let mut w = Watch::default();
        w.observe(counts(0, 2, 0, 0, 0));
        assert_eq!(w.observe(counts(0, 1, 0, 1, 0)), Act::Nothing);
        assert_eq!(w.observe(counts(0, 0, 0, 1, 1)), Act::CollapseSoon);
        assert_eq!(w.observe(counts(0, 0, 0, 1, 1)), Act::Nothing, "once");
        assert_eq!(w.observe(counts(0, 1, 0, 1, 2)), Act::Open, "a new session reopens it");
        let mut late = Watch::default();
        assert_eq!(late.observe(counts(0, 0, 0, 2, 0)), Act::CollapseSoon);
    }

    #[test]
    fn the_capsule_says_what_needs_the_user_first_then_what_works_then_how_it_ended() {
        assert_eq!(headline(&counts(0, 1, 1, 1, 0)), "1 needs you");
        assert_eq!(headline(&counts(1, 1, 0, 1, 0)), "2 working");
        assert_eq!(headline(&counts(0, 0, 0, 2, 0)), "2 done");
        assert_eq!(headline(&counts(0, 0, 0, 2, 1)), "2 done, 1 failed");
    }

    #[test]
    fn elapsed_time_reads_as_a_clock() {
        assert_eq!([clock(-5), clock(7_000), clock(125_000), clock(3_723_000)], ["0:00", "0:07", "2:05", "1:02:03"]);
    }

    fn entry(id: &str, status: &str, question: &str, created_at: i64, settled_at: i64) -> Entry {
        let delegation = Delegation { task_id: id.into(), status: status.into(), question: question.into(), created_at, settled_at, ..Default::default() };
        Entry { delegation, down: None }
    }

    fn ids(list: &[&Entry]) -> Vec<String> {
        list.iter().map(|e| e.delegation.task_id.clone()).collect()
    }

    fn height(entries: &[Entry], lines: usize) -> f32 {
        groups(entries).heights(false, |_| lines).full
    }

    #[test]
    fn the_open_panel_grows_by_a_row_for_each_session_and_the_laid_out_lines_of_a_question() {
        let working = [entry("a", "working", "", 0, 0)];
        let asking = [entry("a", "working", "Which runner?", 0, 0)];
        let pinned = |lines: f32| HEAD_H + ROW_H + 6. + lines * QUESTION_LINE + PINNED_PAD + 6.;
        assert_eq!(height(&working, 5), HEAD_H + ROW_H + 6.);
        assert_eq!(height(&asking, 1), pinned(1.));
        assert_eq!(height(&asking, 2), pinned(2.));
        assert_eq!(height(&asking, 9), pinned(2.), "a question shows two lines at most");
    }

    #[test]
    fn only_a_session_waiting_on_an_answer_shows_its_question() {
        assert_eq!(height(&[entry("a", "completed", "Which runner?", 0, 1)], 1), HEAD_H + ROW_H + 6., "a question left over from before it ended");
        assert_eq!(height(&[entry("a", "working", "Which runner?", 0, 0)], 1), HEAD_H + ROW_H + 6. + QUESTION_LINE + PINNED_PAD + 6.);
    }

    #[test]
    fn questions_pin_on_top_running_sessions_go_oldest_first_and_ended_ones_latest_first() {
        let entries = [
            entry("late", "working", "", 30, 0),
            entry("done-early", "completed", "", 1, 10),
            entry("asks", "working", "Vitest or Jest?", 20, 0),
            entry("early", "starting", "", 10, 0),
            entry("failed-late", "failed", "", 2, 40),
            entry("stopped", "cancelled", "", 3, 20),
        ];
        let g = groups(&entries);
        assert_eq!(ids(&g.asking), ["asks"]);
        assert_eq!(ids(&g.running), ["early", "late"]);
        assert_eq!(ids(&g.ended), ["failed-late", "stopped", "done-early"]);
    }

    #[test]
    fn ended_sessions_fold_into_one_row_only_past_six_sessions() {
        let list = |n: usize| (0..n).map(|i| entry(&i.to_string(), if i == 0 { "working" } else { "completed" }, "", 0, 1)).collect::<Vec<_>>();
        let six = list(6);
        assert!(!groups(&six).folds);
        assert_eq!(height(&six, 1), HEAD_H + 2. * SECTION_H + 6. * ROW_H + 6., "named Working and Ended, every row shown");
        let seven = list(7);
        assert!(groups(&seven).folds);
        assert_eq!(groups(&seven).fold_index(), 2, "under Working and its one row");
        assert_eq!(height(&seven, 1), HEAD_H + SECTION_H + ROW_H + FOLD_H + 6., "one row stands for the six that ended");
        assert_eq!(groups(&seven).heights(true, |_| 1).full, HEAD_H + SECTION_H + 7. * ROW_H + FOLD_H + 6., "opened, they show under it");
        let all_ended = (0..7).map(|i| entry(&i.to_string(), "completed", "", 0, 1)).collect::<Vec<_>>();
        assert!(!groups(&all_ended).folds, "with nothing else to show, the ended ones show");
    }

    #[test]
    fn a_section_is_named_only_beside_another_and_questions_are_named_once_there_are_two() {
        let g = |e: &[Entry]| {
            let g = groups(e);
            [g.asking_head(), g.running_head(), g.ended_head(), g.rule()]
        };
        assert_eq!(g(&[entry("a", "working", "", 0, 0), entry("b", "working", "", 0, 0)]), [false; 4]);
        assert_eq!(g(&[entry("a", "working", "Q?", 0, 0), entry("b", "working", "", 0, 0)]), [false, true, false, true]);
        assert_eq!(g(&[entry("a", "working", "Q?", 0, 0), entry("b", "working", "Q?", 0, 0)]), [true, false, false, false]);
        assert_eq!(g(&[entry("a", "working", "", 0, 0), entry("b", "completed", "", 0, 1)]), [false, true, true, false]);
        let rule = groups(&[entry("a", "working", "Q?", 0, 0), entry("b", "completed", "", 0, 1)]).heights(false, |_| 1).full;
        assert_eq!(rule, HEAD_H + ROW_H + 6. + QUESTION_LINE + PINNED_PAD + RULE_H + SECTION_H + ROW_H + 6.);
    }

    #[test]
    fn the_open_panel_takes_all_it_needs_until_the_chat_runs_out_of_room_then_scrolls() {
        assert_eq!(open_height(300., Some(800.)), 300.);
        assert_eq!(open_height(1900., Some(800.)), 800. - TOP - BOTTOM);
        assert_eq!(open_height(1900., None), 1900., "a chat not measured yet");
        assert_eq!(open_height(1900., Some(20.)), CAPSULE_H, "never shorter than its capsule");
    }

    #[test]
    fn questions_take_all_they_need_until_the_panel_is_short_of_room_then_half_of_it() {
        let h = Heights { pinned: 212., full: 334. };
        assert_eq!(h.pinned_in(334.), 212., "a panel with room for all of it");
        assert_eq!(h.pinned_in(300.), 178., "the rest keeps what it needs while the questions give way");
        assert_eq!(h.pinned_in(200.), 100., "past that, half the panel");
        assert_eq!(Heights { pinned: 60., full: 900. }.pinned_in(400.), 60., "questions under half stay whole");
    }

    #[test]
    fn an_open_panel_stays_open_while_the_chat_follows_its_end_and_shuts_once_it_is_scrolled_away() {
        let mut panel = Panel::new();
        panel.open = true;
        assert!(!panel.scrolled(false), "the chat scrolling itself to its end");
        assert!(panel.open);
        assert!(panel.scrolled(true));
        assert!(!panel.open);
        assert!(!panel.scrolled(true), "a shut panel has nothing to shut");
    }

    #[test]
    fn a_press_outside_an_open_panel_shuts_it_to_its_capsule_and_clears_nothing() {
        let mut panel = Panel::new();
        panel.open = true;
        assert!(panel.pressed_outside());
        assert!(!panel.open);
        assert!(!panel.leaving, "the capsule stays");
        assert!(!panel.pressed_outside(), "a press outside the capsule has nothing to shut");
    }

    #[test]
    fn a_cleared_panel_heads_for_nothing_and_is_gone_once_it_stops_moving() {
        let mut panel = Panel::new();
        panel.open = true;
        assert_eq!(panel.targets(), (1., 1.));
        panel.leave();
        assert_eq!(panel.targets(), (1., 0.));
        assert!(!panel.gone(true), "still fading");
        assert!(panel.gone(false));
        assert!(!Panel::new().gone(false), "a panel nobody cleared stays");
    }

    #[test]
    fn a_capture_shows_an_opening_panel_open_where_a_live_window_starts_it_closed() {
        let open = |still: bool| {
            let mut panel = Panel::new();
            panel.open = true;
            let moving = panel.step(Instant::now(), still);
            (moving, panel.spread.at(), panel.shown.at())
        };
        assert_eq!(open(still(false, true)), (false, 1., 1.));
        assert_eq!(open(still(true, false)), (false, 1., 1.));
        assert_eq!(open(still(false, false)), (true, 0., 0.));
    }

    #[test]
    fn a_new_delegation_brings_back_a_cleared_panel_before_it_is_gone() {
        let mut panel = Panel::new();
        panel.changed(counts(0, 0, 0, 1, 0));
        panel.leave();
        panel.changed(counts(1, 0, 0, 1, 0));
        assert_eq!((panel.leaving, panel.open), (false, true));
    }

    #[test]
    fn the_capsule_gives_way_to_the_list_as_it_opens() {
        assert_eq!((capsule_opacity(0.), list_opacity(0.)), (1., 0.));
        assert_eq!((capsule_opacity(1.), list_opacity(1.)), (0., 1.));
        assert!(capsule_opacity(0.2) > 0. && list_opacity(0.2) == 0., "the capsule goes first");
        assert!(capsule_opacity(0.5) == 0. && list_opacity(0.5) > 0., "then the list");
    }

    #[test]
    fn glass_gives_way_to_a_solid_panel_where_blur_is_not_drawn_or_transparency_is_reduced() {
        assert_eq!(
            [material(false, true), material(true, true), material(false, false), material(true, false)],
            [Material::Glass, Material::Solid, Material::Solid, Material::Solid]
        );
    }
}
