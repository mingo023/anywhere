use super::DelegatedState;
use crate::chat::sub_agent::{self, Mark};
use crate::desktop::Desktop;
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
const QUESTION_LINE: f32 = 17.;
/// Chars of a question a line of the panel holds.
const QUESTION_CHARS: usize = 40;

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
    glide: Glide,
    open: bool,
    watch: Watch,
    collapse: Option<Task<()>>,
}

impl Panel {
    pub(super) fn new() -> Self {
        Self { glide: Glide::from(0.), open: false, watch: Watch::default(), collapse: None }
    }
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

fn full_height(list: &[Delegation]) -> f32 {
    let rows: f32 = list
        .iter()
        .map(|d| match d.phase() {
            Phase::Asking => ROW_H + 6. + QUESTION_LINE * d.question.chars().count().div_ceil(QUESTION_CHARS).clamp(1, 2) as f32,
            _ => ROW_H,
        })
        .sum();
    HEAD_H + rows + 6.
}

fn capsule_width(logos: usize, text: &str) -> f32 {
    let logos = 20. + 15. * logos.saturating_sub(1) as f32;
    7. + logos + 8. + 19. + text.chars().count() as f32 * 6.9 + 12.
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn logos(list: &[Delegation]) -> Vec<&str> {
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

impl DelegatedState {
    /// Takes what a chat's delegations are now: opens its panel for news, or has it shut soon after the last ends.
    /// Returns whether to start the timer that shuts it.
    fn changed(&mut self, parent: &str, c: Counts) -> bool {
        let panel = self.panels.entry(parent.to_string()).or_insert_with(Panel::new);
        match panel.watch.observe(c) {
            Act::Open => {
                (panel.open, panel.collapse) = (true, None);
                false
            }
            Act::CollapseSoon => true,
            Act::Nothing => false,
        }
    }
}

impl Desktop {
    pub(crate) fn delegated_changed(&mut self, parent: &str, cx: &mut Context<Self>) {
        let counts = self.agents.delegations.counts(parent);
        if !self.delegated.changed(parent, counts) {
            return cx.notify();
        }
        let id = parent.to_string();
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COLLAPSE_AFTER).await;
            this.update(cx, |d, cx| {
                if let Some(p) = d.delegated.panels.get_mut(&id) {
                    p.open = false;
                }
                cx.notify();
            })
            .ok();
        });
        if let Some(p) = self.delegated.panels.get_mut(parent) {
            p.collapse = Some(timer);
        }
        cx.notify();
    }

    pub(crate) fn delegated_scrolled(&mut self, parent: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.delegated.panels.get_mut(parent).filter(|p| p.open) {
            (p.open, p.collapse) = (false, None);
            cx.notify();
        }
    }

    fn delegated_set_open(&mut self, parent: &str, open: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.delegated.panels.get_mut(parent) {
            (p.open, p.collapse) = (open, None);
        }
        cx.notify();
    }

    fn delegated_clear(&mut self, parent: &str, cx: &mut Context<Self>) {
        self.agents.delegations.clear_settled(parent);
        self.delegated.panels.remove(parent);
        cx.notify();
    }

    pub(crate) fn delegated_panel(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let list = self.agents.delegations.of(agent);
        if list.is_empty() {
            return None;
        }
        let counts = self.agents.delegations.counts(agent);
        let settled = self.agents.delegations.all_settled(agent);
        let panel = self.delegated.panels.entry(agent.to_string()).or_insert_with(Panel::new);
        let moving = panel.glide.tick(if panel.open { 1. } else { 0. }, Instant::now(), cx.reduce_motion());
        if moving {
            window.request_animation_frame();
        }
        let p = panel.glide.at().clamp(0., 1.);
        let summary = headline(&counts);
        let (full_h, cap_w) = (full_height(list), capsule_width(logos(list).len(), &summary));
        let parent = agent.to_string();
        let capsule = (p < 1.).then(|| capsule(&parent, list, &counts, summary, p < 0.5, cx));
        let full = (p > 0.).then(|| full(&parent, list, settled, p >= 0.5, cx));
        Some(
            div().absolute().top(px(8.)).right(px(16.)).child(
                ui::pop(div())
                    .relative()
                    .overflow_hidden()
                    .w(px(lerp(cap_w, FULL_W, p)))
                    .h(px(lerp(CAPSULE_H, full_h, p)))
                    .rounded(px(lerp(CAPSULE_H / 2., 15., p)))
                    .children(capsule)
                    .children(full),
            ),
        )
    }
}

fn capsule(parent: &str, list: &[Delegation], counts: &Counts, summary: String, live: bool, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let logos = logos(list);
    let shown = list.iter().find(|d| d.phase() == Phase::Asking).or_else(|| list.iter().find(|d| !d.phase().settled())).or(list.last()).map_or(Phase::Working, Delegation::phase);
    let colour = if counts.asking > 0 { WAITING_TEXT } else { TEXT_2 };
    let to = parent.to_string();
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
    if live { capsule.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.delegated_set_open(&to, true, cx))) } else { capsule }
}

fn full(parent: &str, list: &[Delegation], settled: bool, live: bool, cx: &mut Context<Desktop>) -> Div {
    let now = now_ms();
    let (clear, shut) = (parent.to_string(), parent.to_string());
    let button = |id: &'static str| div().id(id).h(px(24.)).px(px(8.)).rounded(px(7.)).flex().items_center().text_size(px(12.)).text_color(TEXT_3).hover(|s| s.bg(FILL_3).text_color(TEXT));
    let head = div()
        .h(px(HEAD_H))
        .pl(px(14.))
        .pr(px(6.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Delegated"))
        .child(div().text_size(px(12.)).text_color(TEXT_4).child(list.len().to_string()))
        .child(div().flex_1())
        .when(settled && live, |d| d.child(button("delegated-clear").cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.delegated_clear(&clear, cx))).child("Clear")))
        .child(button("delegated-collapse").w(px(24.)).px_0().justify_center().when(live, |d| d.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.delegated_set_open(&shut, false, cx)))).child(icon("chevron-up", 12., TEXT_3)));
    let rows = div().px(px(6.)).pb(px(6.)).flex().flex_col().children(list.iter().map(|d| row(d, now, live, cx)));
    div().absolute().top_0().right_0().w(px(FULL_W)).flex().flex_col().child(head).child(rows)
}

fn row(d: &Delegation, now: i64, live: bool, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let phase = d.phase();
    let colour = match phase {
        Phase::Asking => WAITING_TEXT,
        Phase::Failed => FAILED_TEXT,
        _ => TEXT_3,
    };
    let title = if d.title.is_empty() { "Session".to_string() } else { d.title.clone() };
    let open = d.child_id.clone();
    let row = div()
        .id(SharedString::from(format!("delegated-{}", d.task_id)))
        .px(px(8.))
        .py(px(7.))
        .flex()
        .items_start()
        .gap(px(10.))
        .rounded(px(9.))
        .child(div().pt(px(2.)).child(provider_icon(&d.agent, 14., TEXT)))
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
                        .child(div().text_color(colour).child(label(phase)))
                        .child("\u{b7}")
                        .child(clock(d.elapsed_ms(now))),
                )
                .when(phase == Phase::Asking, |c| c.child(div().pt(px(6.)).text_size(px(12.5)).line_height(px(QUESTION_LINE)).text_color(TEXT_2).line_clamp(2).child(d.question.clone()))),
        )
        .child(div().w(px(24.)).h(px(24.)).flex_none().flex().items_center().justify_center().child(glyph(&d.task_id, phase)));
    if live && !d.child_id.is_empty() {
        row.cursor_pointer().hover(|s| s.bg(FILL_2)).active(|s| s.bg(FILL_3)).on_click(cx.listener(move |this, _, window, cx| this.delegated_open(&open, window, cx)))
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{Act, Counts, Delegation, HEAD_H, QUESTION_LINE, ROW_H, Watch, clock, full_height, headline};

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
    fn the_open_panel_grows_by_a_row_for_each_session_and_the_lines_of_a_question() {
        let task = |q: &str| Delegation { status: "working".into(), question: q.into(), ..Default::default() };
        assert_eq!(full_height(&[task("")]), HEAD_H + ROW_H + 6.);
        assert_eq!(full_height(&[task("Which runner?")]), HEAD_H + ROW_H + 6. + QUESTION_LINE + 6.);
        assert_eq!(full_height(&[task(&"x".repeat(200))]), HEAD_H + ROW_H + 6. + 2. * QUESTION_LINE + 6.);
    }
}
