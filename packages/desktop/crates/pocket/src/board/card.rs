use super::Menu;
use super::logic::{Action, Col, Field, Quick, Slot, Tone, column, note, opens, quick, ticket_key};
use super::parts::{CARD_RADIUS, agent_avatar, label_dot, pill, priority_glyph, prop_chip, quick_button, status_glyph};
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::glyph as pr_glyph;
use agents::Decision;
use agents::tickets::{ClosedWhy, Ticket};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone)]
pub(super) struct DragTicket {
    pub(super) id: String,
    pub(super) from: Col,
    title: String,
}

impl Render for DragTicket {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div()).w(px(264.)).p(px(10.)).rounded(px(CARD_RADIUS)).text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).line_clamp(2).child(self.title.clone())
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
    pub(super) fn ticket_key(&self, t: &Ticket) -> Option<String> {
        ticket_key(&self.repo_name(&t.folder), t.number)
    }

    fn pending_ask(&self, t: &Ticket) -> Option<&agents::Permission> {
        self.agents.pending.iter().find(|p| p.agent_id == t.agent_id)
    }

    pub(super) fn ticket_note(&self, t: &Ticket, slot: Slot) -> Option<(String, Token)> {
        let activity = self.agents.get(&t.agent_id).map(|a| a.activity.as_str());
        let (text, tone) = note(t, slot, activity, self.pending_ask(t).map(|p| p.ask()))?;
        Some((text, match tone {
            Tone::Dim => TEXT_4,
            Tone::Asking => WAITING_TEXT,
            Tone::Failed => FAILED_TEXT,
            Tone::Quiet => TEXT_3,
        }))
    }

    pub(super) fn ticket_labels(&self, t: &Ticket) -> Vec<Div> {
        t.labels.iter().filter(|_| self.board.props.labels).map(|l| prop_chip().child(label_dot(l)).child(l.clone())).collect()
    }

    pub(super) fn open_card_menu(&mut self, id: &str, at: Point<Pixels>, sub: Option<Field>, cx: &mut Context<Self>) {
        self.close_menus();
        self.board.menu = Some(Menu::Card { id: id.to_string(), at, sub });
        cx.notify();
    }

    /// Answers the ask the button was drawn for, never one that has replaced it since.
    fn answer_ticket(&mut self, request: &str, decision: Decision, cx: &mut Context<Self>) {
        if self.agents.pending.iter().any(|p| p.request_id == request) {
            self.outbox.resolve(request, decision);
        }
        cx.notify();
    }

    /// `row`, running `action` on ticket `id` without opening the ticket.
    fn quick_click(&self, row: Stateful<Div>, action: Action, id: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = id.to_string();
        row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            this.run_action(action, &id, window, cx);
        }))
    }

    fn answer_click(&self, row: Stateful<Div>, request: &str, decision: Decision, cx: &mut Context<Self>) -> Stateful<Div> {
        let request = request.to_string();
        row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.answer_ticket(&request, decision, cx);
        }))
    }

    /// Deny and Allow once for `request`, with ids `{scope}-deny-{id}` and `{scope}-allow-{id}`.
    pub(crate) fn answer_buttons(&self, scope: &str, id: &str, request: &str, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let button = |name: &str| SharedString::from(format!("{scope}-{name}-{id}"));
        vec![
            self.answer_click(quick_button(button("deny"), None, "Deny"), request, Decision::Deny, cx),
            self.answer_click(quick_button(button("allow"), None, "Allow once").bg(PRIMARY).text_color(ON_PRIMARY).hover(|s| s.bg(PRIMARY).opacity(0.9)), request, Decision::Allow, cx),
        ]
    }

    /// What the ticket offers at a glance, its buttons side by side.
    pub(super) fn ticket_quick(&self, t: &Ticket, slot: Slot, cx: &mut Context<Self>) -> Option<Div> {
        let id = &t.id;
        let button = |name: &str| SharedString::from(format!("ticket-{name}-{id}"));
        let has_pr = self.prs.get(&t.worktree).is_some();
        // A question needs its dialog, in the session.
        let ask = self.pending_ask(t).filter(|p| p.questions().is_empty() && !self.agents.observe_only());
        let found = quick(t, slot.col, ask.is_some(), self.ticket_reach(t), has_pr)?;
        let request = ask.map_or("", |p| p.request_id.as_str());
        let buttons = match found {
            Quick::Start => vec![self.quick_click(quick_button(button("start"), Some("play"), "Start"), Action::Start, id, cx)],
            Quick::StartAgain => vec![self.quick_click(quick_button(button("again"), Some("reload"), "Start again"), Action::StartAgain, id, cx)],
            Quick::CreatePr => vec![self.quick_click(quick_button(button("pr"), Some("pull-request"), "Create PR"), Action::CreatePr, id, cx)],
            Quick::Answer => self.answer_buttons("ticket", id, request, cx),
        };
        Some(div().flex().flex_none().items_center().gap(px(4.)).children(buttons))
    }

    /// The pills for what the ticket has: its PR, its diff, how it closed.
    pub(super) fn ticket_pills(&self, t: &Ticket, slot: Slot) -> Vec<Div> {
        let pr = self.prs.get(&t.worktree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
        });
        let diff = self.repos.get(&t.worktree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        let closed = (slot.col == Col::Closed).then(|| pill().child(closed_label(t.closed_why)));
        [pr, diff, closed].into_iter().flatten().collect()
    }

    pub(super) fn ticket_status(&self, t: &Ticket, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id.clone();
        div().id(SharedString::from(format!("ticket-status-{id}"))).size(px(20.)).flex().flex_none().items_center().justify_center().rounded(px(5.)).cursor_pointer().hover(|s| s.bg(FILL_3)).child(status_glyph(slot.col, t.status)).on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.close_menus();
            this.board.menu = Some(Menu::Status { id: id.clone(), at: e.position() });
            cx.notify();
        })).into_any_element()
    }

    pub(super) fn ticket_priority(&self, t: &Ticket, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = t.id.clone();
        div().id(SharedString::from(format!("ticket-priority-{id}"))).size(px(20.)).flex().flex_none().items_center().justify_center().rounded(px(5.)).when(!column(t).finished(), |d| d.cursor_pointer().hover(|s| s.bg(FILL_3))).child(priority_glyph(t.priority)).when(!column(t).finished(), |d| {
            d.on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_card_menu(&id, e.position(), Some(Field::Priority), cx);
            }))
        })
    }

    fn ticket_body(&self, t: &Ticket, slot: Slot, cx: &mut Context<Self>) -> Div {
        let props = self.board.props;
        let title = div().min_w_0().flex_1().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(if slot.col.finished() { TEXT_3 } else { TEXT }).line_clamp(2).child(t.title.clone());
        let top = div().flex().items_center().min_h(px(20.)).text_size(px(12.)).text_color(TEXT_3).children(self.ticket_key(t).filter(|_| props.id)).child(div().flex_1()).children(props.agent.then(|| agent_avatar(SharedString::from(format!("ticket-agent-{}", t.id)), &t.provider, self.agent_name(&t.provider))));
        let head = div().flex().items_start().min_h(px(20.)).gap(px(7.)).child(div().flex_none().h(px(18.)).flex().items_center().child(self.ticket_status(t, slot, cx))).child(title);
        let note = self.ticket_note(t, slot).map(|(text, color)| div().text_size(px(12.)).line_height(px(16.)).text_color(color).line_clamp(2).child(text));
        let labels = self.ticket_labels(t);
        let project = props.project.then(|| div().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(self.repo_name(&t.folder)));
        let meta = div().flex().flex_wrap().items_center().gap(px(6.)).min_h(px(20.)).children(props.priority.then(|| self.ticket_priority(t, cx))).children(labels).children(project).children(self.ticket_pills(t, slot)).children(self.ticket_quick(t, slot, cx).map(|q| q.ml_auto()));
        div().flex().flex_col().gap(px(6.)).child(top).child(head).children(note).child(meta)
    }

    /// `row` as a ticket: opens it on click and offers its menu on right click.
    pub(super) fn ticket_shell(&self, row: Stateful<Div>, t: &Ticket, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, menu_id, hover_id) = (t.id.clone(), t.id.clone(), t.id.clone());
        let cursor_at = self.board.selection.cursor_at.clone();
        let marks_cursor = self.board.selection.cursor.as_ref() == Some(&t.id);
        let mark = marks_cursor.then(|| canvas(move |b, _, _| cursor_at.set(b.bottom_left()), |_, _, _, _| {}).absolute().inset_0());
        let row = row.relative().children(mark).on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered && this.board.selection.cursor.as_ref() != Some(&hover_id) {
                this.board.selection.cursor = Some(hover_id.clone());
                cx.notify();
            }
        }));
        row.when(opens(t, self.ticket_reach(t)).is_some(), |d| d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_ticket(&id, window, cx)))).on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, e: &MouseDownEvent, _, cx| this.open_card_menu(&menu_id, e.position, None, cx)),
        )
    }

    pub(super) fn ticket_card(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.shown_ticket(slot.col, slot.index) else { return div().into_any_element() };
        let col = slot.col;
        let (picked, at_cursor) = (self.board.selection.picked.contains(&t.id), self.board.selection.cursor.as_ref() == Some(&t.id));
        let hoverable = opens(t, self.ticket_reach(t)).is_some() && !picked;
        let card = div().id(SharedString::from(format!("ticket-{}", t.id))).px(px(10.)).py(px(9.)).rounded(px(CARD_RADIUS)).bg(SURFACE).shadow(ui::row_shadow()).when(col.finished(), |d| d.opacity(0.6)).when(at_cursor, |d| d.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)])).when(picked, |d| d.bg(Hsla::from(INDIGO).opacity(0.12)).shadow(vec![ui::ring(Hsla::from(INDIGO).opacity(0.5), 1.)])).when(hoverable, |d| d.hover(|s| s.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)]))).child(self.ticket_body(t, slot, cx));
        let card = self.ticket_shell(card, t, cx).when(col.lifts(), |d| {
            let before = t.id.clone();
            d.on_drag(DragTicket { id: t.id.clone(), from: col, title: t.title.clone() }, |d, _, _, cx| cx.new(|_| d.clone())).on_drop(cx.listener(move |this, d: &DragTicket, _, cx| {
                cx.stop_propagation();
                this.drop_ticket(&d.id, d.from, col, Some(&before), cx);
            }))
        });
        div().w_full().px(px(8.)).pb(px(6.)).child(card).into_any_element()
    }
}
