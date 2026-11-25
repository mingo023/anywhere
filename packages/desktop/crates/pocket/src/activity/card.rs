use super::logic::{Col, Item, Quick, quick};
use super::side::Extra;
use super::{Activity, ActivityEvent};
use crate::desktop::alerts::answer_row;
use crate::util::{ago, now_ms};
use agents::Permission;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

/// A column's mark; Working's spins while `busy`, as a spinner that never stops redraws the window for nothing.
pub(super) fn col_glyph(col: Col, busy: bool) -> AnyElement {
    let (name, color) = match col {
        Col::Idle => ("status-todo", TEXT_4),
        Col::Working if !busy => {
            return div().size(px(14.)).flex().flex_none().items_center().justify_center().child(icon("spinner-ring", 12., WORKING)).into_any_element();
        }
        Col::Working => {
            let ring = icon("spinner-ring", 12., WORKING).with_animation(
                "activity-working",
                Animation::new(std::time::Duration::from_millis(900)).repeat_synced().with_max_fps(20.),
                |s, t| s.with_transformation(Transformation::rotate(percentage(t))),
            );
            return div().size(px(14.)).flex().flex_none().items_center().justify_center().child(ring).into_any_element();
        }
        Col::NeedsYou => return matrix(WAITING_DOT, MATRIX_FULL),
        Col::Done => return matrix(SUCCESS, MATRIX_FULL),
        Col::InReview => ("pull-request", GOAL_TEXT),
    };
    icon(name, 14., color).into_any_element()
}

/// A session's mark: its column's, but a failed turn shows as failed.
pub(super) fn item_glyph(item: &Item) -> AnyElement {
    if item.failed() { matrix(FAILED, MATRIX_CROSS) } else { col_glyph(item.col(), true) }
}

fn matrix(color: Token, lit: u16) -> AnyElement {
    div().size(px(14.)).flex().flex_none().items_center().justify_center().child(dot_matrix(11., color, lit)).into_any_element()
}

/// A small outlined tag: a PR, a diff.
pub(super) fn pill() -> Div {
    div()
        .h(px(20.))
        .px(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .rounded(px(6.))
        .border(px(0.5))
        .border_color(SEPARATOR_STRONG)
        .text_size(px(11.))
        .text_color(TEXT_3)
}

/// A card's small button: primary is filled, the rest are tinted.
fn sm_button(id: SharedString, lead: Option<Svg>, label: impl Into<SharedString>, primary: bool) -> Stateful<Div> {
    let b = div()
        .id(id)
        .h(px(24.))
        .px(px(9.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .children(lead)
        .child(label.into());
    if primary { b.bg(PRIMARY).text_color(ON_PRIMARY).hover(|s| s.opacity(0.9)) } else { b.bg(FILL_3).text_color(TEXT).hover(|s| s.bg(FILL_4)) }
}

/// A permission ask as "Bash wants to run" over its command.
pub(super) fn ask_lines(p: &Permission, size: f32) -> Div {
    let (verb, what) = p.parts();
    let lead = div().flex().gap(px(4.)).child(div().font_weight(FontWeight::SEMIBOLD).child(p.tool_name.clone())).child(verb.replacen("Wants", "wants", 1));
    let code =
        div().mt(px(4.)).px(px(6.)).py(px(2.)).rounded(px(5.)).bg(rgba(0x00000040)).font_family(MONO).text_size(px(11.5)).line_height(px(14.)).child(what);
    div().flex().flex_col().items_start().text_size(px(size)).line_height(px(size * 1.45)).text_color(TEXT).child(lead).child(code)
}

impl Activity {
    /// The project's lettered tile, as the sidebar shows it.
    pub(super) fn activity_badge(&self, mark: &ui::Mark) -> Div {
        ui::repo_tile(mark, 16., false, None).bg(rgb(0x555555)).text_color(WHITE).shadow(Vec::new())
    }

    /// The agent's logo and id.
    pub(super) fn activity_agent(&self, provider: &str) -> Div {
        div().flex().flex_none().items_center().gap(px(4.)).child(provider_icon(provider, 11., TEXT_3)).child(provider.to_string())
    }

    pub(super) fn activity_card(&self, col: Col, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.grouped.shown(col, ix) else {
            return div().into_any_element();
        };
        let Some(extra) = self.side.extras.get(&item.id) else {
            // Scrolled past what was asked for last frame; the next one asks again.
            window.request_animation_frame();
            return div().into_any_element();
        };
        let (motion, id) = (self.motion.clone(), item.id.clone());
        let card = self.activity_card_body(item, extra, cx).opacity(motion.borrow().opacity(&id, Instant::now()));
        // The list clips at its top edge, which would cut the first card's hover ring.
        div()
            .w_full()
            .px(px(8.))
            .pb(px(8.))
            .when(ix == 0, |d| d.pt(px(4.)))
            .child(card)
            .on_children_prepainted(move |drawn, window, _| {
                if let Some(&bounds) = drawn.first()
                    && motion.borrow_mut().drew(&id, bounds, Instant::now())
                {
                    window.request_animation_frame();
                }
            })
            .into_any_element()
    }

    /// A card on its own, for its place in a column or its copy in flight.
    pub(super) fn activity_card_body(&self, item: &Item, extra: &Extra, cx: &mut Context<Self>) -> Stateful<Div> {
        let (col, id) = (item.col(), item.id.clone());
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(16.))
            .mb(px(6.))
            .text_size(px(11.))
            .text_color(TEXT_3)
            .whitespace_nowrap()
            .child(self.activity_badge(&extra.mark))
            .child(div().min_w_0().truncate().child(item.project_name.clone()))
            .child(self.activity_agent(&item.provider))
            .child(div().flex_none().ml_auto().child(ago(item.at, now_ms())));
        let weight = if col == Col::Done { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM };
        let title = div().text_size(px(13.)).line_height(px(18.)).font_weight(weight).text_color(TEXT).line_clamp(2).child(item.title.clone());
        let branch = (!item.branch.is_empty()).then(|| {
            div()
                .mt(px(4.))
                .h(px(13.))
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(11.))
                .line_height(px(12.8))
                .font_family(MONO)
                .text_color(TEXT_3)
                .child(icon("branch", 11., TEXT_3))
                .child(div().min_w_0().truncate().child(item.branch.clone()))
        });
        let ask = self.activity_ask_box(item, extra, 12., cx).map(|b| b.mt(px(8.)).px(px(10.)).py(px(8.)).gap(px(7.)));
        let note = (ask.is_none() && !item.note.is_empty())
            .then(|| div().mt(px(6.)).text_size(px(12.)).line_height(px(16.8)).text_color(TEXT_3).line_clamp(2).child(item.note.clone()));
        let pills = self.activity_pills(extra);
        let quick = self.activity_quick(item, extra, cx);
        let meta = (!pills.is_empty() || quick.is_some()).then(|| {
            div().mt(px(8.)).h(px(20.)).flex().items_center().gap(px(6.)).children(pills).children(quick.map(|q| q.ml_auto().my(px(-2.)).mr(px(-4.))))
        });
        let previewed = self.previewed.as_deref() == Some(item.id.as_str());
        let rest = vec![ui::ring(SEPARATOR_STRONG, 0.5)];
        div()
            .id(SharedString::from(format!("activity-{id}")))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(10.))
            .bg(SURFACE_SOLID)
            .shadow(if previewed { vec![ui::ring(Hsla::from(TEXT).opacity(0.4), 1.5)] } else { rest })
            .cursor_pointer()
            .when(!previewed, |d| d.hover(|s| s.shadow(vec![ui::ring(FILL_1, 4.), ui::ring(SEPARATOR_STRONG, 0.5)])))
            .child(top)
            .child(title)
            .children(branch)
            .children(ask)
            .children(note)
            .children(meta)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                // The columns close the drawer on a click; this one opens it instead.
                cx.stop_propagation();
                cx.emit(ActivityEvent::Preview(id.clone()));
            }))
    }

    /// The tinted box a session that needs you shows: its permission ask with Deny and Allow once, its question, or its failed turn.
    pub(super) fn activity_ask_box(&self, item: &Item, extra: &Extra, size: f32, cx: &mut Context<Self>) -> Option<Div> {
        if item.col() != Col::NeedsYou {
            return None;
        }
        let tinted = |bg: Token| div().rounded(px(10.)).bg(bg).flex().flex_col();
        let id = item.id.clone();
        // The design pads the answer row 8px all round, past the box's own padding.
        let row = || div().p(px(8.)).flex().flex_wrap().items_center().gap(px(6.));
        let text = || div().text_size(px(size)).line_height(px(size * 1.45)).text_color(TEXT);
        if item.failed() {
            let says = text()
                .flex()
                .gap(px(4.))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(FAILED_TEXT).child("Turn failed."))
                .children((!item.note.is_empty()).then(|| item.note.clone()));
            let again = extra.retry.then(|| {
                let id = id.clone();
                sm_button(format!("activity-retry-{id}").into(), None, "Retry", true).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ActivityEvent::Retry(id.clone()));
                }))
            });
            let dismiss = sm_button(format!("activity-dismiss-{id}").into(), None, "Dismiss", false).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(ActivityEvent::Seen(id.clone()));
            }));
            return Some(tinted(FAILED_BG).child(says).child(row().children(again).child(dismiss)));
        }
        let p = extra.ask.as_ref()?;
        let questions = p.questions();
        let (ask, answers) = match questions.as_slice() {
            [] => (ask_lines(p, size), self.activity_answers(&id, p, cx).map(|a| a.p(px(8.)).into_any_element())),
            [q] if !q.multi_select && !q.options.is_empty() && !self.side.observe_only => {
                let options = q.options.iter().enumerate().map(|(i, o)| {
                    let (request, question, label) = (p.request_id.clone(), q.question.clone(), o.label.clone());
                    sm_button(format!("activity-option-{id}-{i}").into(), None, o.label.clone(), i == 0).on_click(cx.listener(
                        move |_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(ActivityEvent::AnswerQuestion { request: request.clone(), question: question.clone(), label: label.clone() });
                        },
                    ))
                });
                (text().child(q.question.clone()), Some(row().children(options).into_any_element()))
            }
            [q, ..] => (text().child(q.question.clone()), None),
        };
        let answers = answers.unwrap_or_else(|| div().pt(px(4.)).text_size(px(12.)).text_color(TEXT_3).child("Answer it in the session.").into_any_element());
        Some(tinted(WAITING_BG).child(ask).child(answers))
    }

    /// Seen on a turn that ended unseen, Create PR for a branch's changes.
    fn activity_quick(&self, item: &Item, extra: &Extra, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let id = item.id.clone();
        Some(match quick(item.col(), &item.branch, extra.diff.is_some(), extra.pr.is_some())? {
            Quick::Seen => sm_button(format!("activity-seen-{id}").into(), Some(icon("check", 11., TEXT)), "Seen", false).on_click(cx.listener(
                move |_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ActivityEvent::Seen(id.clone()));
                },
            )),
            Quick::CreatePr => sm_button(format!("activity-pr-{id}").into(), Some(icon("pull-request", 12., TEXT)), "Create PR", false).on_click(cx.listener(
                move |_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ActivityEvent::CreatePr(id.clone()));
                },
            )),
        })
    }

    fn activity_answers(&self, id: &str, ask: &Permission, cx: &mut Context<Self>) -> Option<Div> {
        if self.side.observe_only {
            return None;
        }
        let buttons = answer_row("activity", id).into_iter().map(|(b, decision)| {
            let request = ask.request_id.clone();
            b.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(ActivityEvent::Answer { request: request.clone(), decision });
            }))
            .h(px(24.))
            .px(px(9.))
            .rounded(px(7.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
        });
        Some(div().flex().flex_none().items_center().gap(px(6.)).children(buttons))
    }

    /// Its PR and its uncommitted diff, when it has them.
    fn activity_pills(&self, extra: &Extra) -> Vec<Div> {
        let pr = extra.pr.map(|((name, color), number)| pill().child(icon(name, 12., color)).child(format!("#{number}")));
        let diff = extra.diff.map(|(added, removed)| pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.)));
        [pr, diff].into_iter().flatten().collect()
    }
}
