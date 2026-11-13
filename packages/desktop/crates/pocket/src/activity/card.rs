use super::logic::{Col, Item};
use crate::board::parts::{CARD_RADIUS, agent_avatar, pill};
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::glyph as pr_glyph;
use crate::status::Status;
use crate::util::{ago, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// A column's mark, and its cards' too.
pub(super) fn col_glyph(col: Col) -> Svg {
    let (name, color) = match col {
        Col::Idle => ("status-todo", TEXT_4),
        Col::Working => ("status-half", WORKING),
        Col::NeedsYou => ("status-needs", WAITING_DOT),
        Col::Done => ("status-three-quarters", SUCCESS),
        Col::InReview => ("pull-request", GOAL_TEXT),
    };
    icon(name, 14., color)
}

/// A card's mark: its column's, but a failed turn shows as failed.
fn card_glyph(col: Col, item: &Item) -> Svg {
    if item.status == Some(Status::Failed) { icon("status-closed", 14., FAILED) } else { col_glyph(col) }
}

impl Desktop {
    pub(super) fn activity_card(&self, col: Col, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.activity.shown(col, ix) else { return div().into_any_element() };
        let id = item.id.clone();
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_h(px(18.))
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(div().min_w_0().truncate().child(item.project_name.clone()))
            .child(div().flex_1())
            .child(agent_avatar(SharedString::from(format!("activity-agent-{id}")), &item.provider, self.agent_name(&item.provider)))
            .child(div().flex_none().child(ago(item.at, now_ms())));
        let title = div().min_w_0().flex_1().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).line_clamp(2).child(item.title.clone());
        let head = div().flex().items_start().gap(px(7.)).child(div().flex_none().h(px(18.)).flex().items_center().child(card_glyph(col, item))).child(title);
        let branch = (!item.branch.is_empty()).then(|| div().flex().items_center().gap(px(5.)).text_size(px(11.)).font_family(MONO).text_color(TEXT_3).child(icon("branch", 11., TEXT_4)).child(div().min_w_0().truncate().child(item.branch.clone())));
        let tone = match (col, item.status) {
            (_, Some(Status::Failed)) => FAILED_TEXT,
            (Col::NeedsYou, _) => WAITING_TEXT,
            _ => TEXT_3,
        };
        let note = (!item.note.is_empty()).then(|| div().text_size(px(12.)).line_height(px(16.)).text_color(tone).line_clamp(2).child(item.note.clone()));
        let meta = div().flex().flex_wrap().items_center().gap(px(6.)).children(self.activity_pills(item)).children(self.activity_answers(&item.id, cx).map(|a| a.ml_auto()));
        let body = div().flex().flex_col().gap(px(6.)).child(top).child(head).children(branch).children(note).child(meta);
        let previewed = self.activity.previewed.as_deref() == Some(item.id.as_str());
        let card = div()
            .id(SharedString::from(format!("activity-{id}")))
            .px(px(10.))
            .py(px(9.))
            .rounded(px(CARD_RADIUS))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .cursor_pointer()
            .hover(|s| s.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)]))
            .when(previewed, |d| d.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.5)]))
            .child(body)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.preview_session(&id, cx)));
        div().w_full().px(px(8.)).pb(px(6.)).child(card).into_any_element()
    }

    /// Deny and Allow once for the session's permission ask; `None` for a question, which needs its dialog.
    pub(super) fn activity_answers(&self, id: &str, cx: &mut Context<Self>) -> Option<Div> {
        let ask = self.agents.pending.iter().find(|p| p.agent_id == id).filter(|p| p.questions().is_empty() && !self.agents.observe_only())?;
        Some(div().flex().flex_none().items_center().gap(px(4.)).children(self.answer_buttons("activity", id, &ask.request_id, cx)))
    }

    /// Its PR and its uncommitted diff, when it has them.
    fn activity_pills(&self, item: &Item) -> Vec<Div> {
        let pr = self.prs.get(&item.tree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number))
        });
        let diff = self.repos.get(&item.tree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        [pr, diff].into_iter().flatten().collect()
    }
}
