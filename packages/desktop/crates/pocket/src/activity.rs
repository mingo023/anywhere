mod card;
mod column;
mod filter_bar;
mod logic;
mod page;
mod panel;

use crate::actions::OpenActivity;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::util::first_line;
use crate::status::{self, Status};
use agents::Summary;
use git::github::PrState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
pub(crate) use logic::Menu;
use logic::{COLS, Escape, Filters, Grouped, Item, escape, marks_seen, opens_preview};

pub struct ActivityState {
    pub(crate) previewed: Option<String>,
    pub(crate) filters: Filters,
    pub(crate) menu: Option<Menu>,
    grouped: Grouped,
    lists: Vec<ListState>,
    search: Entity<InputState>,
}

impl ActivityState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let subs = vec![cx.subscribe(&search, |this: &mut Desktop, s, _: &InputEvent, cx| {
            this.activity.filters.query = s.read(cx).value().to_string();
            this.activity_changed();
            cx.notify();
        })];
        let state = Self { previewed: None, filters: Filters::default(), menu: None, grouped: Grouped::default(), lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), search };
        (state, subs)
    }

    fn regroup(&mut self, items: Vec<Item>) {
        if !self.grouped.take(items, &self.filters) {
            return;
        }
        // A card's height follows its note, so the lists measure again, keeping where each was scrolled to.
        for (list, line) in self.lists.iter().zip(&self.grouped.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
        self.previewed = self.grouped.keep(self.previewed.take());
    }
}

impl Desktop {
    pub(crate) fn open_activity(&mut self, _: &OpenActivity, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.activity.previewed = None;
        self.screen = Screen::Activity;
        self.activity_changed();
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn leave_activity(&mut self, cx: &mut Context<Self>) {
        self.activity.previewed = None;
        self.screen = Screen::Sessions;
        cx.notify();
    }

    /// Esc lets go of a focused search field first, then closes the preview, then leaves the page.
    pub(crate) fn escape_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match escape(self.activity.search.focus_handle(cx).is_focused(window), self.activity.previewed.is_some()) {
            Escape::LeaveSearch => window.focus(&self.root, cx),
            Escape::ClosePreview => self.activity.previewed = None,
            Escape::LeavePage => self.leave_activity(cx),
        }
        cx.notify();
    }

    fn preview_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if marks_seen(self.agents.get(id).and_then(Status::of)) {
            self.outbox.seen(&[id.to_string()]);
        }
        self.activity.previewed = Some(id.to_string());
        cx.notify();
    }

    /// Keeps the preview when the session has no place to open in.
    fn open_previewed(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_agent(id, window, cx);
        if self.screen != Screen::Activity {
            self.activity.previewed = None;
        }
    }

    /// ↵ or ⌘↵ opens the previewed session, unless a field has the keyboard.
    pub(crate) fn activity_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let opens = opens_preview(&key.key, !key.modifiers.modified(), key.modifiers == Modifiers::command());
        if self.screen != Screen::Activity || self.overlay.is_some() || self.activity.menu.is_some() || !self.root.is_focused(window) || !opens {
            return false;
        }
        let Some(id) = self.activity.previewed.clone() else { return false };
        self.open_previewed(&id, window, cx);
        true
    }

    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn activity_changed(&mut self) {
        if self.screen == Screen::Activity {
            let items = self.activity_items();
            self.activity.regroup(items);
            if let Some(id) = self.activity.grouped.ended_in_view(self.activity.previewed.as_deref()) {
                self.outbox.seen(&[id]);
            }
        }
    }

    /// Every session with a terminal, as the page shows it.
    fn activity_items(&self) -> Vec<Item> {
        let projects = self.projects();
        self.agents
            .list
            .iter()
            .filter_map(|a| {
                let cwd = &self.terminals.sessions.get(&a.terminal_id)?.info.cwd;
                let project = self.project_of(cwd, &projects).cloned().unwrap_or_else(|| cwd.clone());
                let tree = self.worktree_of(cwd);
                let status = Status::of(a);
                let pr_open = tree.and_then(|w| self.prs.get(&w.path)).is_some_and(|pr| pr.state == PrState::Open);
                Some(Item {
                    id: a.id.clone(),
                    status,
                    pr_open,
                    at: a.updated_at,
                    title: status::title(a),
                    project_name: self.repo_name(&project),
                    project,
                    branch: tree.map(|w| w.branch.clone()).unwrap_or_default(),
                    tree: tree.map(|w| w.path.clone()).unwrap_or_default(),
                    provider: a.provider.clone(),
                    note: self.activity_note(a, status),
                })
            })
            .collect()
    }

    fn activity_note(&self, a: &Summary, status: Option<Status>) -> String {
        match status {
            Some(Status::NeedsYou) => self.agents.pending.iter().find(|p| p.agent_id == a.id).map(|p| p.ask()).unwrap_or_else(|| first_line(self.agents.last_text(&a.id).unwrap_or_default())),
            Some(Status::Failed) => first_line(self.agents.last_result(&a.id).map_or("", |r| r.error.as_str())),
            Some(Status::Working) => a.activity.clone(),
            Some(Status::Done) => first_line(self.agents.last_text(&a.id).unwrap_or_default()),
            Some(Status::Idle) | None => String::new(),
        }
    }
}
