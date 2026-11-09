use super::logic::{Col, Row, Slot, activity, in_line, opens};
use super::parts::{pill, status_glyph};
use crate::desktop::Desktop;
use crate::util::{ago_long, now_ms};
use agents::tickets::Status;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    fn section_head(&self, col: Col) -> AnyElement {
        let n = self.board.columns[col as usize].len();
        div().w_full().h(px(32.)).px(px(24.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(status_glyph(("board-head", col as usize), col, Status::None)).child(col.name()).child(div().text_color(TEXT_4).child(n.to_string())).into_any_element()
    }

    fn ticket_row(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.shown_ticket(slot.col, slot.index) else { return div().into_any_element() };
        let col = slot.col;
        let title = div().min_w_0().flex_1().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(if col.finished() { TEXT_3 } else { TEXT }).child(t.title.clone());
        let row = div()
            .id(SharedString::from(format!("ticket-row-{}", t.id)))
            .h(px(40.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(8.))
            .when(opens(t, self.ticket_reach(t)).is_some(), |d| d.hover(|s| s.bg(FILL_2)))
            .child(status_glyph(SharedString::from(format!("ticket-row-glyph-{}", t.id)), col, t.status))
            .child(self.repo_tile_for(&t.folder, 16.))
            .child(title)
            .children((col == Col::Queued).then(|| pill().child(in_line(slot.pos))))
            .child(div().flex_none().child(ui::agent_label(&t.provider, self.agent_name(&t.provider))))
            .child(div().flex_none().w(px(72.)).text_right().text_size(px(11.5)).text_color(TEXT_4).child(ago_long(activity(t), now_ms())));
        div().w_full().px(px(16.)).py(px(1.)).child(self.ticket_shell(row, t, cx)).into_any_element()
    }

    pub(super) fn board_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = list(self.board.rows_list.clone(), cx.processor(|this, ix, _, cx| match this.board.rows.get(ix).copied() {
            Some(Row::Head(col)) => this.section_head(col),
            Some(Row::Ticket(slot)) => this.ticket_row(slot, cx),
            None => div().into_any_element(),
        }))
        .size_full();
        div().flex_1().min_h_0().child(rows)
    }
}
