use super::logic::{Field, Surface, column, in_line, place_in_column};
use super::parts::{bar_button, flush_textarea, pill, prop_chip, start_kbd, status_glyph};
use crate::desktop::Desktop;
use crate::util::{ago_long, now_ms};
use agents::tickets::State;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    fn ticket_page_bar(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.board.form;
        let id = f.picks.values.id.clone().unwrap_or_default();
        let t = self.ticket(&id);
        let key = t.and_then(|t| self.ticket_key(t)).unwrap_or_default();
        let name = div().min_w_0().truncate().child(format!("{key}  {}", f.title.read(cx).value()));
        let back = ui::icon_button_sized("ticket-back", "back", 24., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.leave_ticket_page(window, cx)));
        let board = div().id("ticket-crumb").px(px(4.)).h(px(24.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(6.)).cursor_pointer().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2).text_color(TEXT)).child(icon("blocks", 13., TEXT_3)).child("Board").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.leave_ticket_page(window, cx)));
        let crumb = div().flex_1().min_w_0().flex().items_center().gap(px(4.)).child(back).child(board).child(icon("chevron-right", 11., TEXT_4)).child(name);
        let queued = f.picks.values.state == Some(State::Queued);
        let line = t.filter(|_| queued).and_then(|t| place_in_column(&self.board.columns, &self.agents.board.tickets, t));
        let action: AnyElement = if queued {
            bar_button("ticket-back-todo", Variant::Secondary, None, "Move back to To do").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.move_page_ticket_back(cx))).into_any_element()
        } else {
            bar_button("ticket-start", Variant::Primary, Some("play"), "Start").child(start_kbd()).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_from_page(window, cx))).into_any_element()
        };
        let ids = self.board.page.as_deref().unwrap_or_default();
        let at = ids.iter().position(|i| *i == id).map_or(0, |i| i + 1);
        let count = div().text_size(px(12.)).text_color(TEXT_3).child(format!("{at} / {}", ids.len()));
        let step = |btn: &'static str, glyph: &'static str, delta: isize| ui::icon_button_sized(btn, glyph, 24., TEXT_2).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.step_ticket_page(delta, window, cx)));
        let right = div().flex().items_center().gap(px(6.)).children(line.map(|p| pill().child(in_line(p)))).child(action).child(div().w(px(1.)).h(px(16.)).mx(px(6.)).bg(SEPARATOR)).child(count).child(step("ticket-prev", "chevron-up", -1)).child(step("ticket-next", "chevron-down", 1));
        self.detail_bar(crumb.ml(px(2.)), right, cx).h(px(44.))
    }

    fn page_properties(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.board.form;
        let row = |name: &'static str, value: AnyElement| div().h(px(28.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_3).child(div().w(px(84.)).flex_none().child(name)).child(value);
        let id = f.picks.values.id.as_deref().unwrap_or_default();
        let t = self.ticket(id);
        let status = t.map(|t| {
            let col = column(t);
            prop_chip().child(status_glyph(col, t.status)).child(col.name()).into_any_element()
        });
        let updated = t.map(|t| prop_chip().child(ago_long(t.changed_at, now_ms())).into_any_element());
        let chip = |this: &Self, field, cx: &mut Context<Self>| this.field_chip(Surface::Page, field, cx).into_any_element();
        div()
            .w(px(280.))
            .flex_none()
            .px(px(16.))
            .py(px(14.))
            .border_l_1()
            .border_color(SEPARATOR)
            .flex()
            .flex_col()
            .children(status.map(|s| row("Status", s)))
            .child(row("Priority", chip(self, Field::Priority, cx)))
            .child(row("Labels", chip(self, Field::Labels, cx)))
            .child(div().h(px(1.)).my(px(8.)).bg(HAIRLINE))
            .child(row("Agent", chip(self, Field::Agent, cx)))
            .child(row("Project", chip(self, Field::Project, cx)))
            .child(row("Base", chip(self, Field::Base, cx)))
            .children(updated.map(|u| row("Updated", u)))
    }

    pub(super) fn ticket_page(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.board.form;
        // The input's line box is a fixed 1.25rem, so a larger size clips descenders.
        let title = div().font_weight(FontWeight::SEMIBOLD).child(Input::new(&f.title).appearance(false).p_0().text_size(px(18.)));
        let prompt = flush_textarea(div()).flex_1().min_h_0().text_color(TEXT_2).child(Textarea::new(&f.prompt).appearance(false).text_size(px(13.)));
        let main = div().flex_1().min_w_0().px(px(24.)).py(px(18.)).flex().flex_col().gap(px(10.)).child(title).child(prompt);
        let body = div().flex_1().min_h_0().flex().child(main).child(self.page_properties(cx));
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.ticket_page_bar(cx)).child(body)
    }
}
