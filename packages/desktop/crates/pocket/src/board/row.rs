use super::logic::{Col, Field, Row, Slot, activity, column, opens, stuck_head};
use super::parts::{agent_avatar, col_heading, prop_chip};
use crate::desktop::Desktop;
use crate::util::{ago_long, now_ms};
use agents::tickets::Ticket;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    fn section_bar(&self, col: Col, cx: &mut Context<Self>) -> Div {
        let n = self.board.columns[col as usize].len();
        let add = (col == Col::Todo).then(|| self.add_ticket_button("board-list-add", cx));
        let glyph = if self.board.folded[col as usize] { "chevron-right" } else { "chevron-down" };
        let fold = ui::icon_button_sized(("board-fold", col as usize), glyph, 18., TEXT_4).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.board.folded[col as usize] = !this.board.folded[col as usize];
            this.board_changed();
            cx.notify();
        }));
        div().h(px(32.)).pl(px(4.)).pr(px(6.)).flex().items_center().gap(px(8.)).rounded(px(7.)).bg(FILL_1).child(fold).child(col_heading(col, n.to_string())).child(div().flex_1()).children(add)
    }

    fn section_head(&self, col: Col, first: bool, cx: &mut Context<Self>) -> AnyElement {
        div().w_full().px(px(8.)).when(!first, |d| d.pt(px(6.))).child(self.section_bar(col, cx)).into_any_element()
    }

    /// `CANVAS` is clear in dark mode, so a head stuck over rows needs a solid fill or they show through it.
    fn stuck_section(&self, col: Col, cx: &mut Context<Self>) -> Div {
        div().absolute().top_0().left_0().right_0().px(px(8.)).child(div().rounded(px(7.)).bg(POPOVER).occlude().child(self.section_bar(col, cx)))
    }

    /// `face`, opening `field`'s choices on a click while the ticket has no session yet.
    fn row_prop(&self, t: &Ticket, field: Field, face: impl IntoElement, cx: &mut Context<Self>) -> AnyElement {
        if !column(t).lifts() {
            return face.into_any_element();
        }
        let id = t.id.clone();
        div()
            .id(SharedString::from(format!("ticket-row-{field:?}-{id}")))
            .flex_none()
            .rounded(px(11.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .child(face)
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_card_menu(&id, e.position(), Some(field), cx);
            }))
            .into_any_element()
    }

    fn ticket_check(&self, t: &Ticket, picked: bool, at_cursor: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = t.id.clone();
        div()
            .id(SharedString::from(format!("ticket-check-{id}")))
            .absolute()
            .left(px(5.))
            .top(px(10.5))
            .size(px(15.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .when(!picked && !at_cursor, |d| d.opacity(0.).group_hover("ticket-row", |s| s.opacity(1.)))
            .when(picked, |d| d.bg(INDIGO).child(icon("check", 11., ON_INDIGO)))
            .when(!picked, |d| d.border_1().border_color(SEPARATOR_STRONG))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.board.selection.toggle(&id);
                cx.notify();
            }))
    }

    fn ticket_row(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.shown_ticket(slot.col, slot.index) else { return div().into_any_element() };
        let col = slot.col;
        let props = self.board.props;
        let title = div().min_w_0().flex_1().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(if col.finished() { TEXT_3 } else { TEXT }).child(t.title.clone());
        let note = self.ticket_note(t, slot).map(|(text, color)| div().flex_shrink(1.).min_w_0().max_w(px(320.)).truncate().text_size(px(12.)).text_color(color).child(text));
        let quick = self.ticket_quick(t, slot, cx);
        let labels = self.ticket_labels(t);
        let (picked, at_cursor) = (self.board.selection.picked.contains(&t.id), self.board.selection.cursor.as_ref() == Some(&t.id));
        let quick = quick.map(|q| if at_cursor { q } else { q.opacity(0.).group_hover("ticket-row", |s| s.opacity(1.)) });
        let project = props.project.then(|| self.row_prop(t, Field::Project, prop_chip().child(self.repo_tile_for(&t.folder, 14.)).child(div().min_w_0().truncate().child(self.repo_name(&t.folder))), cx));
        let agent = props.agent.then(|| self.row_prop(t, Field::Agent, agent_avatar(SharedString::from(format!("ticket-row-agent-{}", t.id)), &t.provider, self.agent_name(&t.provider)), cx));
        let row = div()
            .id(SharedString::from(format!("ticket-row-{}", t.id)))
            .group("ticket-row")
            .relative()
            .h(px(36.))
            .pl(px(26.))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(7.))
            .when(opens(t, self.ticket_reach(t)).is_some(), |d| d.hover(|s| s.bg(FILL_1)))
            .when(at_cursor, |d| d.bg(FILL_1))
            .when(picked, |d| d.bg(Hsla::from(INDIGO).opacity(0.12)))
            .children((!col.finished()).then(|| self.ticket_check(t, picked, at_cursor, cx)))
            .children(props.priority.then(|| self.ticket_priority(t, cx)))
            .children(self.ticket_key(t).filter(|_| props.id).map(|k| div().flex_none().min_w(px(46.)).text_size(px(12.)).text_color(TEXT_3).child(k)))
            .child(self.ticket_status(t, slot, cx))
            .child(title)
            .children(note)
            .children(quick)
            .children(self.ticket_pills(t, slot))
            .children(labels)
            .children(project)
            .children(agent)
            .children(props.updated.then(|| div().flex_none().w(px(52.)).text_right().text_size(px(12.)).text_color(TEXT_3).child(ago_long(activity(t), now_ms()))));
        div().w_full().px(px(8.)).child(self.ticket_shell(row, t, cx)).into_any_element()
    }

    pub(super) fn board_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = list(self.board.rows_list.clone(), cx.processor(|this, ix, _, cx| match this.board.rows.get(ix).copied() {
            Some(Row::Head(col)) => this.section_head(col, ix == 0, cx),
            Some(Row::Ticket(slot)) => this.ticket_row(slot, cx),
            None => div().into_any_element(),
        }))
        .size_full();
        let top = self.board.rows_list.logical_scroll_top();
        let stuck = stuck_head(&self.board.rows, top.item_ix, top.offset_in_item > px(0.)).map(|col| self.stuck_section(col, cx));
        div().relative().flex_1().min_h_0().child(rows).children(stuck)
    }
}
