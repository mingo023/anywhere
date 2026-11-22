use super::{Slot, shown};
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

    /// Takes what the chat's delegations are now: opens for news, or has the panel shut soon after the last ends.
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

/// The list's height, with each asking session's question at the lines `lines` says it wraps to.
fn full_height<'a>(list: impl Iterator<Item = &'a Delegation>, lines: impl Fn(&Delegation) -> usize) -> f32 {
    let rows: f32 = list
        .map(|d| match question(d) {
            Some(_) => ROW_H + 6. + QUESTION_LINE * lines(d).clamp(1, QUESTION_LINES) as f32,
            None => ROW_H,
        })
        .sum();
    HEAD_H + rows + 6.
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

/// A session of the panel's list, with the host it runs on while that host's link is down.
pub(super) struct Entry {
    delegation: Delegation,
    down: Option<String>,
}

pub(crate) enum PanelEvent {
    /// Open the delegated session by agent id.
    Open(String),
    /// The panel faded out after a Clear.
    Gone,
}

/// One chat's panel of the sessions it delegated to, over its chat; `Desktop` hands it the list each time it draws it.
pub(crate) struct PanelView {
    panel: Panel,
    entries: Vec<Entry>,
    capturing: bool,
}

impl EventEmitter<PanelEvent> for PanelView {}

impl PanelView {
    pub(super) fn new() -> Self {
        Self { panel: Panel::new(), entries: Vec::new(), capturing: false }
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
        let full_h = if spread > 0. { full_height(list.clone(), |d| text_lines(&d.question, QUESTION_SIZE, QUESTION_W, window)) } else { 0. };
        let scale = lerp(ENTER_SCALE, 1., appeared);
        let capsule = (capsule_opacity(spread) > 0.).then(|| capsule(list, &counts, logos, summary, spread < 0.5, cx).opacity(capsule_opacity(spread)));
        let full = (list_opacity(spread) > 0.).then(|| full(&self.entries, counts.total, settled, spread >= 0.5, cx).opacity(list_opacity(spread)));
        let material = material(reduce_transparency, cfg!(target_os = "macos"));
        div().absolute().top(px(TOP)).right(px(16.)).child(
            shell(material)
                .relative()
                .overflow_hidden()
                .opacity(appeared)
                .w(px(lerp(cap_w, FULL_W, spread) * scale))
                .h(px(lerp(CAPSULE_H, full_h, spread) * scale))
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
        self.delegated.panels.insert(parent.to_string(), Slot { view: view.clone(), _sub: sub });
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

fn full(entries: &[Entry], total: usize, settled: bool, live: bool, cx: &mut Context<PanelView>) -> Div {
    let now = now_ms();
    let button = |id: &'static str| {
        div().id(id).h(px(24.)).px(px(8.)).rounded(px(7.)).flex().items_center().text_size(px(12.)).text_color(TEXT_3).hover(|s| s.bg(FILL_3).text_color(TEXT))
    };
    let head = div()
        .h(px(HEAD_H))
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
    let rows = div().px(px(6.)).pb(px(6.)).flex().flex_col().children(entries.iter().map(|e| row(e, now, live, cx)));
    div().absolute().top_0().right_0().w(px(FULL_W)).flex().flex_col().child(head).child(rows)
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
        Act, Counts, Delegation, HEAD_H, Material, Panel, QUESTION_LINE, ROW_H, Watch, capsule_opacity, clock, full_height, headline, list_opacity, material,
        still,
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

    #[test]
    fn the_open_panel_grows_by_a_row_for_each_session_and_the_laid_out_lines_of_a_question() {
        let task = |q: &str| Delegation { status: "working".into(), question: q.into(), ..Default::default() };
        let asking = |q: &str| Delegation { status: "working".into(), question: q.into(), ..Default::default() };
        let lines = |n: usize| move |_: &Delegation| n;
        assert_eq!(full_height([task("")].iter(), lines(5)), HEAD_H + ROW_H + 6.);
        assert_eq!(full_height([asking("Which runner?")].iter(), lines(1)), HEAD_H + ROW_H + 6. + QUESTION_LINE + 6.);
        assert_eq!(full_height([asking("Which runner?")].iter(), lines(2)), HEAD_H + ROW_H + 6. + 2. * QUESTION_LINE + 6.);
        assert_eq!(
            full_height([asking("Which runner?")].iter(), lines(9)),
            HEAD_H + ROW_H + 6. + 2. * QUESTION_LINE + 6.,
            "a question shows two lines at most"
        );
    }

    #[test]
    fn only_a_session_waiting_on_an_answer_shows_its_question() {
        let with = |status: &str| Delegation { status: status.into(), question: "Which runner?".into(), ..Default::default() };
        assert_eq!(full_height([with("completed")].iter(), |_| 1), HEAD_H + ROW_H + 6., "a question left over from before it ended");
        assert_eq!(full_height([with("working")].iter(), |_| 1), HEAD_H + ROW_H + 6. + QUESTION_LINE + 6.);
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
