use super::Menu;
use super::logic::{Action, Col, Slot, activity, in_line, opens};
use super::parts::{pill, status_glyph};
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::glyph as pr_glyph;
use crate::util::{ago_long, basename, now_ms};
use agents::tickets::{ClosedWhy, Status, Ticket};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

#[derive(Clone)]
pub(super) struct DragTicket {
    pub(super) id: String,
    pub(super) from: Col,
    title: String,
}

impl Render for DragTicket {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div()).w(px(264.)).p(px(10.)).rounded(px(10.)).text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).line_clamp(2).child(self.title.clone())
    }
}

fn closed_label(why: ClosedWhy) -> &'static str {
    match why {
        ClosedWhy::Merged => "Merged",
        ClosedWhy::Deleted => "Worktree deleted",
        ClosedWhy::Closed => "Closed",
    }
}

impl Desktop {
    fn ticket_note(&self, t: &Ticket) -> Option<(String, Token)> {
        match t.status {
            Status::NeedsYou => self.agents.pending.iter().find(|p| p.agent_id == t.agent_id).map(|p| (p.ask(), WAITING_TEXT)),
            Status::Failed if !t.why.is_empty() => Some((t.why.clone(), FAILED_TEXT)),
            Status::Stopped if !t.why.is_empty() => Some((t.why.clone(), TEXT_3)),
            _ => None,
        }
    }

    fn ticket_top(&self, t: &Ticket) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(self.repo_tile_for(&t.folder, 16.))
            .child(div().min_w_0().truncate().child(self.repo_name(&t.folder)))
            .child(div().flex_none().child(ui::agent_label(&t.provider, self.agent_name(&t.provider))))
            .child(div().flex_1())
            .child(div().flex_none().text_color(TEXT_4).child(ago_long(activity(t), now_ms())))
    }

    fn ticket_branch(&self, t: &Ticket) -> Option<Div> {
        let branch = self.board_tree(t).map(|w| w.branch.clone()).or_else(|| Some(basename(&t.worktree).to_string()).filter(|b| !b.is_empty()))?;
        Some(div().flex().items_center().gap(px(5.)).text_size(px(11.5)).text_color(TEXT_3).child(icon("branch", 12., TEXT_4)).child(div().min_w_0().truncate().font_family(MONO).child(branch)))
    }

    fn ticket_pills(&self, t: &Ticket, col: Col) -> Div {
        let pr = self.prs.get(&t.worktree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
        });
        let diff = self.repos.get(&t.worktree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        let closed = col.finished().then(|| pill().child(closed_label(t.closed_why)));
        div().flex().items_center().flex_wrap().gap(px(4.)).children(pr).children(diff).children(closed).child(div().flex_1()).child(status_glyph(SharedString::from(format!("ticket-glyph-{}", t.id)), col, t.status))
    }

    fn ticket_body(&self, t: &Ticket, slot: Slot, cx: &mut Context<Self>) -> Div {
        let title = div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(if slot.col.finished() { TEXT_3 } else { TEXT }).line_clamp(2).child(t.title.clone());
        let body = div().flex().flex_col().gap(px(6.)).child(self.ticket_top(t)).child(title);
        match slot.col {
            Col::Todo => {
                let id = t.id.clone();
                let start = ui::button(SharedString::from(format!("ticket-start-{id}")), Variant::Ghost, Some("play"), "Start").h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.run_action(Action::Start, &id, window, cx);
                }));
                body.child(div().flex().items_center().child(start).child(div().flex_1()))
            }
            Col::Queued => body.child(div().flex().items_center().child(pill().child(in_line(slot.pos))).child(div().flex_1()).child(status_glyph(SharedString::from(format!("ticket-glyph-{}", t.id)), slot.col, t.status))),
            _ => {
                let note = (slot.col == Col::NeedsYou).then(|| self.ticket_note(t)).flatten().map(|(text, color)| div().text_size(px(12.)).line_height(px(16.)).text_color(color).line_clamp(2).child(text));
                body.children(note).children(self.ticket_branch(t)).child(self.ticket_pills(t, slot.col))
            }
        }
    }

    /// `row` as a ticket: opens it on click and offers its menu on right click.
    pub(super) fn ticket_shell(&self, row: Stateful<Div>, t: &Ticket, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, menu_id) = (t.id.clone(), t.id.clone());
        row.when(opens(t, self.ticket_reach(t)).is_some(), |d| d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_ticket(&id, window, cx)))).on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                this.close_menus();
                this.board.menu = Some(Menu::Card { id: menu_id.clone(), at: e.position });
                cx.notify();
            }),
        )
    }

    pub(super) fn ticket_card(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.shown_ticket(slot.col, slot.index) else { return div().into_any_element() };
        let col = slot.col;
        let hoverable = opens(t, self.ticket_reach(t)).is_some();
        let card = div().id(SharedString::from(format!("ticket-{}", t.id))).p(px(10.)).rounded(px(10.)).bg(SURFACE).shadow(ui::row_shadow()).when(col == Col::Closed, |d| d.opacity(0.6)).when(hoverable, |d| d.hover(|s| s.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)]))).child(self.ticket_body(t, slot, cx));
        let card = self.ticket_shell(card, t, cx).when(col.lifts(), |d| {
            let before = t.id.clone();
            d.on_drag(DragTicket { id: t.id.clone(), from: col, title: t.title.clone() }, |d, _, _, cx| cx.new(|_| d.clone())).on_drop(cx.listener(move |this, d: &DragTicket, _, cx| {
                cx.stop_propagation();
                this.drop_ticket(&d.id, d.from, col, Some(&before), cx);
            }))
        });
        div().w_full().px(px(8.)).pb(px(8.)).child(card).into_any_element()
    }
}
