use super::logic::{Surface, column, stepped};
use super::values::{Picks, Values, page_save};
use super::{Board, BoardEvent};
use crate::desktop::Desktop;
use crate::modals::new_session::default_first;
use agents::tickets::{State, Ticket, TicketDraft};
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
    pub fn new(window: &mut Window, cx: &mut Context<Board>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Ticket title"));
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Tell the agent what you want done…").auto_grow(10, 24));
        let subs = vec![
            cx.subscribe(&title, |this, _, ev: &InputEvent, cx| this.ticket_page_typed(ev, cx)),
            cx.subscribe(&prompt, |this, _, ev: &InputEvent, cx| this.ticket_page_typed(ev, cx)),
        ];
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

impl Board {
    pub(super) fn set_branches(&mut self, surface: Surface, folder: String, branches: Vec<String>) {
        match surface {
            Surface::Composer => self.composer.picks.set_branches(folder, branches),
            Surface::Page => self.form.picks.set_branches(folder, branches),
        }
    }

    /// What the open page would change in `ticket`, the one it edits as it is now.
    pub(super) fn page_draft(&self, ticket: Option<&Ticket>, cx: &App) -> Option<TicketDraft> {
        self.page.as_ref()?;
        let (title, prompt) = self.form.texts(cx);
        page_save(&self.form.picks.values, &title, &prompt, ticket?)
    }

    pub(super) fn page_ticket(&self) -> Option<&str> {
        self.page.as_ref().and(self.form.picks.values.id.as_deref())
    }

    /// The ticket `delta` places along the page's steps.
    pub(crate) fn stepped_page(&self, delta: isize) -> Option<String> {
        let (ids, at) = (self.page.as_ref()?, self.form.picks.values.id.as_deref()?);
        stepped(ids, at, delta).map(str::to_string)
    }

    /// The draft Start sends, when the ticket on the page can start and has what it needs.
    pub(super) fn start_draft(&self, is_repo: bool, cx: &App) -> Option<TicketDraft> {
        let (title, prompt) = self.form.texts(cx);
        let ready = self.form.picks.values.ready(&title, &prompt, is_repo);
        (self.form.picks.values.can_start() && ready).then(|| self.form.draft(cx))
    }

    pub(super) fn ticket_page_typed(&mut self, ev: &InputEvent, cx: &mut Context<Self>) {
        if matches!(ev, InputEvent::Change) {
            self.save_page_soon(cx);
        }
        cx.notify();
    }

    /// Saves once the edits pause, so leaving the page by any way keeps them.
    pub(super) fn save_page_soon(&mut self, cx: &mut Context<Self>) {
        self.form.saving = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            this.update(cx, |_, cx| cx.emit(BoardEvent::SavePage)).ok();
        }));
    }
}

impl Desktop {
    /// Opens the page of a ticket that has not started.
    pub(crate) fn edit_ticket(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        let Some(t) = self.ticket(id).filter(|t| column(t).lifts()).cloned() else { return };
        let folder = self.board.update(cx, |b, cx| b.open_page(&t, window, cx));
        self.close_menus(cx);
        window.focus(&self.root, cx);
        self.load_ticket_branches(Surface::Page, folder, cx);
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
                d.board.update(cx, |b, cx| {
                    b.set_branches(surface, folder, branches);
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    /// A folder whose worktrees aren't read yet gets the benefit of the doubt.
    pub(super) fn folder_is_repo(&self, folder: &str) -> bool {
        self.worktrees.get(folder).is_none_or(|w| !w.is_empty())
    }

    pub(super) fn save_ticket_page(&mut self, cx: &App) {
        let b = self.board.read(cx);
        let ticket = b.page_ticket().and_then(|id| self.agents.board.get(id));
        if let Some(draft) = b.page_draft(ticket, cx) {
            self.outbox.local.board_save(draft, false);
        }
    }

    pub(crate) fn leave_ticket_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        self.board.update(cx, |b, _| b.page = None);
        window.focus(&self.root, cx);
        self.board_changed(cx);
        cx.notify();
    }

    pub(super) fn start_from_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_repo = self.folder_is_repo(&self.board.read(cx).form.picks.values.folder);
        let Some(draft) = self.board.read(cx).start_draft(is_repo, cx) else { return };
        self.outbox.local.board_save(draft, true);
        self.board.update(cx, |b, _| b.page = None);
        window.focus(&self.root, cx);
        self.board_changed(cx);
        cx.notify();
    }

    pub(super) fn move_page_ticket_back(&mut self, cx: &mut Context<Self>) {
        self.save_ticket_page(cx);
        if let Some(id) = self.board.read(cx).form.picks.values.id.clone() {
            self.outbox.local.board_move(&id, State::Todo, None);
            self.board.update(cx, |b, _| b.form.picks.values.state = Some(State::Todo));
        }
        cx.notify();
    }
}
