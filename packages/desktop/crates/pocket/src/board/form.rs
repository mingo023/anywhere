use super::logic::{Surface, column, page_ids, stepped};
use super::values::{Picks, Values, page_save};
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::modals::new_session::default_first;
use agents::tickets::{State, TicketDraft};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::time::Duration;

const SAVE_AFTER: Duration = Duration::from_millis(400);

/// The ticket page's fields; the ticket it edits is `picks.values.id`.
pub struct Form {
    pub(super) title: Entity<InputState>,
    pub(super) prompt: Entity<TextareaState>,
    pub(super) picks: Picks,
    saving: Option<Task<()>>,
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Ticket title"));
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Tell the agent what you want done…").auto_grow(10, 24));
        let desktop = cx.entity().downgrade();
        // Ahead of the keymap: a textarea's ⌘↵ would put a newline in the prompt first.
        let start = cx.intercept_keystrokes(move |ev, window, cx| {
            let k = &ev.keystroke;
            if k.key == "enter" && k.modifiers.secondary(cx) && desktop.update(cx, |d, cx| d.start_key(window, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        });
        let subs = vec![cx.subscribe(&title, |this, _, ev: &InputEvent, cx| this.ticket_page_typed(ev, cx)), cx.subscribe(&prompt, |this, _, ev: &InputEvent, cx| this.ticket_page_typed(ev, cx)), start];
        (Self { title, prompt, picks: Picks::new(Values::new_in(String::new())), saving: None }, subs)
    }

    fn texts(&self, cx: &App) -> (String, String) {
        (self.title.read(cx).value().to_string(), self.prompt.read(cx).value().to_string())
    }

    fn draft(&self, cx: &App) -> TicketDraft {
        let (title, prompt) = self.texts(cx);
        self.picks.values.draft(&title, &prompt)
    }
}

impl Desktop {
    /// Opens the page of a ticket that has not started.
    pub(crate) fn edit_ticket(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        let Some(t) = self.agents.board.get(id).filter(|t| column(t).lifts()).cloned() else { return };
        self.board.page = Some(page_ids(&self.agents.board.tickets, &self.board.columns));
        let f = &mut self.board.form;
        f.picks = Picks::new(Values::of(&t));
        f.title.update(cx, |s, cx| s.set_value(t.title.clone(), window, cx));
        f.prompt.update(cx, |s, cx| s.set_value(t.prompt.clone(), window, cx));
        self.close_menus();
        window.focus(&self.root, cx);
        self.load_ticket_branches(Surface::Page, t.folder, cx);
        cx.notify();
    }

    pub(super) fn load_ticket_branches(&mut self, surface: Surface, folder: String, cx: &mut Context<Self>) {
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
                match surface {
                    Surface::Composer => d.board.composer.picks.set_branches(folder, branches),
                    Surface::Page => d.board.form.picks.set_branches(folder, branches),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A folder whose worktrees aren't read yet gets the benefit of the doubt.
    pub(super) fn folder_is_repo(&self, folder: &str) -> bool {
        self.worktrees.get(folder).is_none_or(|w| !w.is_empty())
    }

    fn page_ready(&self, cx: &App) -> bool {
        let f = &self.board.form;
        let (title, prompt) = f.texts(cx);
        f.picks.values.ready(&title, &prompt, self.folder_is_repo(&f.picks.values.folder))
    }

    pub(super) fn save_ticket_page(&mut self, cx: &App) {
        if self.board.page.is_none() {
            return;
        }
        let f = &self.board.form;
        let Some(t) = f.picks.values.id.as_deref().and_then(|id| self.agents.board.get(id)) else { return };
        let (title, prompt) = f.texts(cx);
        if let Some(draft) = page_save(&f.picks.values, &title, &prompt, t) {
            self.outbox.local.board_save(draft, false);
        }
    }

    fn ticket_page_typed(&mut self, ev: &InputEvent, cx: &mut Context<Self>) {
        if matches!(ev, InputEvent::Change) {
            self.save_ticket_page_soon(cx);
        }
        cx.notify();
    }

    /// Saves once the edits pause, so leaving the page by any way keeps them.
    pub(super) fn save_ticket_page_soon(&mut self, cx: &mut Context<Self>) {
        self.board.form.saving = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            this.update(cx, |d, cx| d.save_ticket_page(cx)).ok();
        }));
    }

    pub(crate) fn leave_ticket_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        self.board.page = None;
        window.focus(&self.root, cx);
        self.board_changed();
        cx.notify();
    }

    /// ⌘↵ on a ticket's page, whichever field has the keyboard.
    fn start_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.screen != Screen::Board || self.board.page.is_none() || self.overlay.is_some() || self.board.menu.is_some() {
            return false;
        }
        self.start_from_page(window, cx);
        true
    }

    pub(super) fn start_from_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.board.form.picks.values.can_start() || !self.page_ready(cx) {
            return;
        }
        self.outbox.local.board_save(self.board.form.draft(cx), true);
        self.board.page = None;
        window.focus(&self.root, cx);
        self.board_changed();
        cx.notify();
    }

    pub(super) fn step_ticket_page(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ids), Some(at)) = (self.board.page.as_ref(), self.board.form.picks.values.id.as_deref()) else { return };
        if let Some(next) = stepped(ids, at, delta).map(str::to_string) {
            self.edit_ticket(&next, window, cx);
        }
    }

    pub(super) fn move_page_ticket_back(&mut self, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        if let Some(id) = self.board.form.picks.values.id.clone() {
            self.outbox.local.board_move(&id, State::Todo, None);
            self.board.form.picks.values.state = Some(State::Todo);
        }
        cx.notify();
    }
}
