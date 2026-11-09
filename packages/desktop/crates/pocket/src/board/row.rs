use super::logic::{Col, Row, Slot, activity, in_line, opens};
use super::parts::{agent_avatar, col_heading, pill, status_glyph};
use crate::desktop::Desktop;
use crate::util::{ago_long, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    fn section_head(&self, col: Col, cx: &mut Context<Self>) -> AnyElement {
        let n = self.board.columns[col as usize].len();
        let add = (col == Col::Todo).then(|| self.add_ticket_button("board-list-add", cx));
        let bar = div().h(px(34.)).px(px(10.)).flex().items_center().justify_between().rounded(px(6.)).bg(FILL_1).child(col_heading(col, n.to_string())).children(add);
        div().w_full().px(px(16.)).pt(px(2.)).pb(px(2.)).child(bar).into_any_element()
    }

    fn ticket_row(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.shown_ticket(slot.col, slot.index) else { return div().into_any_element() };
        let col = slot.col;
        let title = div().min_w_0().flex_1().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(if col.finished() { TEXT_3 } else { TEXT }).child(t.title.clone());
        let branch = self.ticket_branch(t).map(|b| pill().child(icon("branch", 11., TEXT_4)).child(div().max_w(px(200.)).truncate().child(b)));
        let row = div()
            .id(SharedString::from(format!("ticket-row-{}", t.id)))
            .h(px(40.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(6.))
            .when(opens(t, self.ticket_reach(t)).is_some(), |d| d.hover(|s| s.bg(FILL_1)))
            .child(div().flex_none().w(px(96.)).truncate().text_size(px(12.5)).text_color(TEXT_3).child(self.repo_name(&t.folder)))
            .child(status_glyph(col, t.status))
            .child(title)
            .children((col == Col::Queued).then(|| pill().child(in_line(slot.pos))))
            .children(branch)
            .child(agent_avatar(SharedString::from(format!("ticket-row-agent-{}", t.id)), &t.provider, self.agent_name(&t.provider)))
            .child(div().flex_none().w(px(56.)).text_right().text_size(px(12.)).text_color(TEXT_3).child(ago_long(activity(t), now_ms())));
        div().w_full().px(px(16.)).child(self.ticket_shell(row, t, cx)).into_any_element()
    }

    pub(super) fn board_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = list(self.board.rows_list.clone(), cx.processor(|this, ix, _, cx| match this.board.rows.get(ix).copied() {
            Some(Row::Head(col)) => this.section_head(col, cx),
            Some(Row::Ticket(slot)) => this.ticket_row(slot, cx),
            None => div().into_any_element(),
        }))
        .size_full();
        div().flex_1().min_h_0().child(rows)
    }
}
