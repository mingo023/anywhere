use super::logic::busy;
use super::parts::action_button;
use super::values::{Values, default_folder};
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::new_session::default_first;
use agents::tickets::TicketDraft;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

pub struct Form {
    title: Entity<InputState>,
    prompt: Entity<TextareaState>,
    pub(super) v: Values,
    /// A folder's local branches, its default first, as last read.
    branches: Option<(String, Vec<String>)>,
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Ticket title"));
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Tell the agent what you want done…").auto_grow(6, 14));
        let subs = vec![cx.subscribe(&title, |_, _, _: &InputEvent, cx| cx.notify()), cx.subscribe(&prompt, |_, _, _: &InputEvent, cx| cx.notify())];
        (Self { title, prompt, v: Values::new_in(String::new()), branches: None }, subs)
    }

    pub(super) fn branches(&self) -> &[String] {
        self.branches.as_ref().filter(|(f, _)| *f == self.v.folder).map_or(&[], |(_, b)| b.as_slice())
    }

    fn texts(&self, cx: &App) -> (String, String) {
        (self.title.read(cx).value().to_string(), self.prompt.read(cx).value().to_string())
    }

    fn draft(&self, cx: &App) -> TicketDraft {
        let (title, prompt) = self.texts(cx);
        self.v.draft(&title, &prompt)
    }
}

impl Desktop {
    /// Opens the dialog on ticket `id`, or on a new one.
    pub(crate) fn edit_ticket(&mut self, id: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let existing = id.and_then(|id| self.agents.board.get(id)).cloned();
        let first = self.projects().into_iter().next();
        let new_folder = default_folder(self.board.filters.project.as_deref(), self.project.as_deref(), first.as_deref());
        let (title, prompt) = existing.as_ref().map_or_else(Default::default, |t| (t.title.clone(), t.prompt.clone()));
        let f = &mut self.board.form;
        f.v = existing.as_ref().map_or_else(|| Values::new_in(new_folder), Values::of);
        f.title.update(cx, |s, cx| s.set_value(title, window, cx));
        f.prompt.update(cx, |s, cx| s.set_value(prompt, window, cx));
        f.title.update(cx, |s, cx| s.focus(window, cx));
        let folder = f.v.folder.clone();
        self.open(Overlay::Ticket, window, cx);
        self.load_ticket_branches(folder, cx);
    }

    pub(super) fn load_ticket_branches(&mut self, folder: String, cx: &mut Context<Self>) {
        if folder.is_empty() {
            return;
        }
        let (runner, dir) = (hosts::runner_for(&folder), folder.clone());
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&runner, &dir, false).map(|r| r.branch).unwrap_or_default();
            let mut branches = git::dated_branches(&runner, &dir);
            default_first(&mut branches, "", &current);
            branches.into_iter().map(|(b, _)| b).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let branches = task.await;
            this.update(cx, |d, cx| {
                if d.board.form.v.folder == folder {
                    d.board.form.branches = Some((folder, branches));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A folder whose worktrees aren't read yet gets the benefit of the doubt.
    fn form_in_repo(&self) -> bool {
        self.worktrees.get(&self.board.form.v.folder).is_none_or(|w| !w.is_empty())
    }

    fn form_ready(&self, cx: &App) -> bool {
        let (title, prompt) = self.board.form.texts(cx);
        self.board.form.v.ready(&title, &prompt, self.form_in_repo())
    }

    fn save_ticket(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.form_ready(cx) {
            self.outbox.local.board_save(self.board.form.draft(cx), start);
            self.close_overlay(window, cx);
        }
    }

    pub(crate) fn ticket_view(&mut self, cx: &mut Context<Self>) -> Div {
        let ready = self.form_ready(cx);
        let f = &self.board.form;
        let close = ui::icon_button_sized("ticket-close", "x", 24., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let crumb = div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(TEXT_3).child(self.form_project_chip(cx)).child(icon("chevron-right", 12., TEXT_4)).child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(if f.v.editing() { "Edit ticket" } else { "New ticket" })).child(div().flex_1()).child(close);
        // The input's line box is a fixed 1.25rem, so a larger size clips descenders.
        let title = div().font_weight(FontWeight::SEMIBOLD).child(Input::new(&f.title).appearance(false).p_0().text_size(px(18.)));
        let prompt = div().min_h(px(96.)).text_size(px(14.)).child(Textarea::new(&f.prompt).appearance(false).p_0());
        let chips = div().flex().items_center().gap(px(4.)).child(self.form_agent_chip(cx)).child(self.form_base_chip(cx));
        let dim = |b: Stateful<Div>| b.when(!ready, |d| d.opacity(0.4).cursor_default());
        let delete = f.v.id.clone().map(|id| ui::button("ticket-delete", Variant::Ghost, Some("trash"), "Delete").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.ask_delete_ticket(id.clone(), window, cx))));
        let save_label = if f.v.editing() { "Save" } else { "Add to To do" };
        let save = dim(if f.v.can_start() { ui::button("ticket-save", Variant::Ghost, None, save_label).h(px(28.)) } else { action_button("ticket-save", save_label) }).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_ticket(false, window, cx)));
        let start = f.v.can_start().then(|| dim(action_button("ticket-start", "Start").child(ui::button_kbd("⌘↩"))).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_ticket(true, window, cx))));
        let hint = f.v.hint(self.form_in_repo(), busy(&self.agents.board.tickets), self.agents.board.limit);
        let foot = div().px(px(16.)).h(px(48.)).flex().items_center().gap(px(8.)).border_t_1().border_color(HAIRLINE).children(delete).child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).children(hint)).child(save).children(start);
        let head = div().p(px(16.)).pb(px(12.)).flex().flex_col().gap(px(10.)).child(crumb).child(title).child(prompt).child(chips);
        div().absolute().top(px(80.)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(720.)).flex().flex_col())
                .occlude()
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                    if e.keystroke.key == "enter" && e.keystroke.modifiers.platform {
                        cx.stop_propagation();
                        this.save_ticket(true, window, cx);
                    }
                }))
                .child(head)
                .child(foot),
        )
    }
}
