mod card;
mod column;
mod filter_bar;
mod logic;
mod motion;
mod page;
mod panel;
mod recap;

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
use motion::Motion;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub struct ActivityState {
    pub(crate) previewed: Option<String>,
    pub(crate) filters: Filters,
    pub(crate) menu: Option<Menu>,
    grouped: Grouped,
    lists: Vec<ListState>,
    /// Shared with the cards' prepaint, which says where each was drawn.
    motion: Rc<RefCell<Motion>>,
    search: Entity<InputState>,
    reply: Entity<InputState>,
}

impl ActivityState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let reply = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe(&search, |this: &mut Desktop, s, _: &InputEvent, cx| {
                this.activity.filters.query = s.read(cx).value().to_string();
                this.activity_changed();
                cx.notify();
            }),
            cx.subscribe_in(&reply, window, |this: &mut Desktop, _, e: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = e {
                    this.reply_to_previewed(window, cx);
                }
            }),
        ];
        let state = Self { previewed: None, filters: Filters::default(), menu: None, grouped: Grouped::default(), lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), motion: Rc::default(), search, reply };
        (state, subs)
    }

    fn regroup(&mut self, items: Vec<Item>) {
        let before = self.grouped.places();
        if !self.grouped.take(items, &self.filters) {
            return;
        }
        self.motion.borrow_mut().shift(&before, &self.grouped.places(), Instant::now());
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
    pub(crate) fn open_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.activity.previewed = None;
        // Where cards were drawn last time the page showed is no place to fly from.
        self.activity.motion.borrow_mut().clear();
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
        let typing = self.activity.search.focus_handle(cx).is_focused(window) || self.activity.reply.focus_handle(cx).is_focused(window);
        match escape(typing, self.activity.previewed.is_some()) {
            Escape::LeaveSearch => window.focus(&self.root, cx),
            Escape::ClosePreview => self.activity.previewed = None,
            Escape::LeavePage => self.leave_activity(cx),
        }
        cx.notify();
    }

    pub(crate) fn preview_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity.previewed.as_deref() == Some(id) {
            return;
        }
        if marks_seen(self.agents.get(id).and_then(Status::of)) {
            self.outbox.seen(&[id.to_string()]);
        }
        let to = self.agents.get(id).map(|a| a.provider.clone()).unwrap_or_default();
        self.activity.reply.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(format!("Reply to {to}…"), window, cx);
        });
        self.activity.previewed = Some(id.to_string());
        cx.notify();
    }

    /// Sends what the drawer's reply field holds to the previewed session.
    fn reply_to_previewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.activity.reply.read(cx).value().trim().to_string();
        let Some(id) = self.activity.previewed.clone().filter(|_| !text.is_empty()) else { return };
        self.outbox.prompt(&id, &text, false);
        self.activity.reply.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    fn mark_seen(&mut self, id: &str, cx: &mut Context<Self>) {
        self.outbox.seen(&[id.to_string()]);
        cx.notify();
    }

    /// The prompt that started the session's last turn.
    fn activity_last_prompt(&self, id: &str) -> Option<String> {
        Some(recap::recap(self.agents.timelines.get(id)?).asked).filter(|p| !p.is_empty())
    }

    /// Sends the failed turn's prompt again.
    fn retry_turn(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(prompt) = self.activity_last_prompt(id) {
            self.outbox.prompt(id, &prompt, false);
        }
        cx.notify();
    }

    fn answer_question(&mut self, request: &str, question: &str, label: &str, cx: &mut Context<Self>) {
        if self.agents.pending.iter().any(|p| p.request_id == request) {
            self.outbox.answer(request, &[(question.to_string(), label.to_string())].into());
        }
        cx.notify();
    }

    /// Opens the session with the PR composer up, as the Board's Create PR does.
    fn create_pr_for(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_agent(id, window, cx);
        self.open_pr_composer(window, cx);
    }

    /// Opens the session's PR, or a check's log, where Settings say; an in-app tab opens in the session, so it goes there first.
    fn open_pr_link(&mut self, id: &str, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.git.in_app {
            self.open_previewed(id, window, cx);
        }
        self.open_pr(url, window, cx);
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
