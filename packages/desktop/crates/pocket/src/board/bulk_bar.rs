use super::Menu;
use super::logic::{Action, bulk_targets};
use super::parts::{bar_button, label_dot};
use crate::desktop::Desktop;
use agents::tickets::State;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const BULK_FIELDS: [(Action, &str); 4] = [(Action::SetStatus, "Status"), (Action::Priority, "Priority"), (Action::Agent, "Agent"), (Action::Labels, "Label")];

fn field_lead(action: Action) -> AnyElement {
    match action {
        Action::SetStatus => icon("status-todo", 14., TEXT_4).into_any_element(),
        Action::Priority => icon("priority-high", 14., TEXT_2).into_any_element(),
        Action::Agent => icon("bot", 13., TEXT_2).into_any_element(),
        _ => label_dot("Feature").into_any_element(),
    }
}

fn bulk_button(id: impl Into<ElementId>, lead: impl IntoElement, label: &'static str) -> Stateful<Div> {
    bar_button(id, Variant::Ghost, None, div().flex().items_center().gap(px(6.)).child(lead).child(label))
}

impl Desktop {
    /// The picked tickets `action` reaches, in the order the page shows them.
    pub(super) fn bulk_ids(&self, action: Action) -> Vec<String> {
        bulk_targets(&self.board.walk_slots(), &self.agents.board.tickets, &self.board.selection.picked, action).into_iter().map(|t| t.id.clone()).collect()
    }

    pub(super) fn bulk_status_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        [(Action::MoveBack, State::Todo, "status-todo", "To do"), (Action::Queue, State::Queued, "status-queued", "Queued")]
            .into_iter()
            .enumerate()
            .filter(|(_, (action, ..))| !self.bulk_ids(*action).is_empty())
            .map(|(i, (action, state, glyph, label))| {
                ui::menu_row(("board-bulk-status", i), glyph, label, None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        for id in this.bulk_ids(action) {
                            this.outbox.local.board_move(&id, state, None);
                        }
                        this.close_menus();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect()
    }

    fn bulk_start(&mut self, cx: &mut Context<Self>) {
        for id in self.bulk_ids(Action::Start) {
            self.outbox.local.board_start(&id);
        }
        self.board.selection.picked.clear();
        cx.notify();
    }

    fn bulk_close(&mut self, cx: &mut Context<Self>) {
        let ids = self.bulk_ids(Action::Close);
        self.close_tickets(ids, cx);
        self.board.selection.picked.clear();
        cx.notify();
    }

    pub(super) fn bulk_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let picked = &self.board.selection.picked;
        if picked.is_empty() {
            return None;
        }
        let slots = self.board.walk_slots();
        let can = |a: Action| !bulk_targets(&slots, &self.agents.board.tickets, picked, a).is_empty();
        let clear = ui::glyph_frame("board-bulk-clear", 20., 20., 7.).child(icon("x", 11., TEXT_3)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.board.selection.picked.clear();
            cx.notify();
        }));
        let fields: Vec<_> = BULK_FIELDS
            .into_iter()
            .filter(|(action, ..)| can(*action))
            .map(|(action, label)| {
                bulk_button(SharedString::from(format!("board-bulk-{label}")), field_lead(action), label).on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    this.close_menus();
                    this.board.menu = Some(Menu::Bulk { action, at: e.position() });
                    cx.notify();
                }))
            })
            .collect();
        let start = can(Action::Start).then(|| bulk_button("board-bulk-start", icon("play", 11., TEXT_2), "Start").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_start(cx))));
        let close = can(Action::Close).then(|| bulk_button("board-bulk-close", icon("status-closed", 14., TEXT_4), "Close").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_close(cx))));
        let divider = div().w(px(1.)).h(px(16.)).mx(px(5.)).bg(SEPARATOR);
        let bar = div().h(px(38.)).pl(px(12.)).pr(px(5.)).flex().items_center().gap(px(3.)).rounded(px(11.)).bg(POPOVER).shadow(vec![ui::ring(SEPARATOR_STRONG, 1.), ui::shadow(rgba(0x00000073), 14., 34.)]).text_size(px(12.5)).child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT).mr(px(2.)).child(format!("{} selected", picked.len()))).child(clear).child(divider).children(fields).children(start).children(close);
        let above_notice = self.board.hidden > 0;
        Some(div().absolute().left_0().right_0().bottom(px(if above_notice { 48. } else { 3. })).flex().justify_center().child(bar))
    }
}
