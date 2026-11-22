mod card;
mod column;
mod filter_bar;
mod logic;
mod motion;
mod page;
mod panel;
pub(crate) mod recap;
mod side;

use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::git_ui::pull_requests::glyph as pr_glyph;
use crate::status::{self, Status};
use crate::util::first_line;
use agents::{Decision, Summary};
use git::github::PrState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
pub(crate) use logic::Menu;
use logic::{COLS, Escape, Filters, Grouped, Item, escape, marks_seen, opens_preview, values};
use motion::Motion;
use side::{Drawer, Extra, Side};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub(crate) enum ActivityEvent {
    /// The search or a filter pick changed what the columns keep; `Desktop` gathers the sessions again.
    Changed,
    /// A chip was pressed: the other menus shut, then `show` opens this one.
    OpenMenu(Menu),
    CloseMenus,
    Preview(String),
    Open(String),
    Reply {
        id: String,
        text: String,
    },
    Answer {
        request: String,
        decision: Decision,
    },
    AnswerQuestion {
        request: String,
        question: String,
        label: String,
    },
    Retry(String),
    Seen(String),
    CreatePr(String),
    OpenPr {
        id: String,
        url: String,
    },
}

/// The Activity page's columns and the drawer beside them, over what `Desktop` hands down each time it draws them; the bar's filters are `filters`.
pub struct Activity {
    pub(crate) previewed: Option<String>,
    filters: Filters,
    pub(crate) menu: Option<Menu>,
    grouped: Grouped,
    lists: Vec<ListState>,
    /// Shared with the cards' prepaint, which says where each was drawn.
    motion: Rc<RefCell<Motion>>,
    search: Entity<InputState>,
    reply: Entity<InputState>,
    side: Side,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ActivityEvent> for Activity {}

impl Activity {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let reply = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe(&search, |this, s, _: &InputEvent, cx| {
                this.filters.query = s.read(cx).value().to_string();
                cx.emit(ActivityEvent::Changed);
                cx.notify();
            }),
            cx.subscribe_in(&reply, window, |this, _, e: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = e {
                    this.send_reply(window, cx);
                }
            }),
        ];
        Self {
            previewed: None,
            filters: Filters::default(),
            menu: None,
            grouped: Grouped::default(),
            lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(),
            motion: Rc::default(),
            search,
            reply,
            side: Side::default(),
            _subs: subs,
        }
    }

    pub(crate) fn items(&self) -> &[Item] {
        &self.grouped.items
    }

    pub(crate) fn item(&self, id: &str) -> Option<&Item> {
        self.grouped.item(id)
    }

    /// What the menu offers, each value with how many sessions have it.
    pub(crate) fn values(&self, menu: Menu) -> Vec<(String, usize)> {
        values(&self.grouped.items, menu)
    }

    pub(crate) fn set(&mut self, side: Side) {
        self.side = side;
    }

    /// The page opens with nothing previewed, and no place to fly cards from: where they were drawn last time it showed is none.
    pub(crate) fn open(&mut self) {
        self.previewed = None;
        self.motion.borrow_mut().clear();
    }

    pub(crate) fn leave(&mut self) {
        self.previewed = None;
    }

    /// Picks `value` in `menu`, or lets go of it when it was picked.
    pub(crate) fn pick(&mut self, menu: Menu, value: &str) {
        self.filters.toggle(menu, value);
    }

    pub(crate) fn show(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = Some(menu);
        cx.notify();
    }

    /// Esc lets go of a focused search field first, then closes the preview; leaving the page is `Desktop`'s.
    pub(crate) fn escape(&mut self, window: &Window, cx: &App) -> Escape {
        let typing = self.search.focus_handle(cx).is_focused(window) || self.reply.focus_handle(cx).is_focused(window);
        let esc = escape(typing, self.previewed.is_some());
        if esc == Escape::ClosePreview {
            self.previewed = None;
        }
        esc
    }

    /// Previews `id`, its reply field asking for what `to` answers.
    pub(crate) fn preview(&mut self, id: &str, to: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reply.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(format!("Reply to {to}…"), window, cx);
        });
        self.previewed = Some(id.to_string());
        cx.notify();
    }

    /// Sends what the drawer's reply field holds to the previewed session.
    fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.reply.read(cx).value().trim().to_string();
        let Some(id) = self.previewed.clone().filter(|_| !text.is_empty()) else { return };
        cx.emit(ActivityEvent::Reply { id, text });
        self.reply.update(cx, |s, cx| s.set_value("", window, cx));
    }

    /// Takes the sessions as they are now; the previewed one, when its turn ended while you were looking.
    pub(crate) fn gather(&mut self, items: Vec<Item>) -> Option<String> {
        self.regroup(items);
        self.grouped.ended_in_view(self.previewed.as_deref())
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
        self.close_menus(cx);
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.activity.update(cx, |a, _| a.open());
        self.screen = Screen::Activity;
        self.activity_changed(cx);
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn leave_activity(&mut self, cx: &mut Context<Self>) {
        self.activity.update(cx, |a, _| a.leave());
        self.screen = Screen::Sessions;
        cx.notify();
    }

    pub(crate) fn escape_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.activity.update(cx, |a, cx| a.escape(window, cx)) {
            Escape::LeaveSearch => window.focus(&self.root, cx),
            Escape::ClosePreview => {}
            Escape::LeavePage => self.leave_activity(cx),
        }
        cx.notify();
    }

    pub(crate) fn on_activity_event(&mut self, _: &Entity<Activity>, ev: &ActivityEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ActivityEvent::Changed => {
                self.activity_changed(cx);
                cx.notify();
            }
            ActivityEvent::OpenMenu(menu) => {
                self.close_menus(cx);
                self.activity.update(cx, |a, cx| a.show(*menu, cx));
            }
            ActivityEvent::CloseMenus => {
                self.close_menus(cx);
                cx.notify();
            }
            ActivityEvent::Preview(id) => self.preview_session(id, window, cx),
            ActivityEvent::Open(id) => self.open_previewed(id, window, cx),
            ActivityEvent::Reply { id, text } => {
                self.outbox.prompt(id, text);
                cx.notify();
            }
            ActivityEvent::Answer { request, decision } => self.answer_pending(request, *decision, cx),
            ActivityEvent::AnswerQuestion { request, question, label } => self.answer_question(request, question, label, cx),
            ActivityEvent::Retry(id) => self.retry_turn(id, cx),
            ActivityEvent::Seen(id) => self.mark_seen(id, cx),
            ActivityEvent::CreatePr(id) => self.create_pr_for(id, window, cx),
            ActivityEvent::OpenPr { id, url } => self.open_pr_link(id, url.clone(), window, cx),
        }
    }

    pub(crate) fn preview_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity.read(cx).previewed.as_deref() == Some(id) {
            return;
        }
        if marks_seen(self.agents.get(id).and_then(Status::of)) {
            self.outbox.seen(&[id.to_string()]);
        }
        let to = self.agents.get(id).map(|a| a.provider.clone()).unwrap_or_default();
        self.activity.update(cx, |a, cx| a.preview(id, &to, window, cx));
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
            self.outbox.prompt(id, &prompt);
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
            self.activity.update(cx, |a, _| a.leave());
        }
    }

    /// ↵ or ⌘↵ opens the previewed session, unless a field has the keyboard.
    pub(crate) fn activity_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let opens = opens_preview(&key.key, !key.modifiers.modified(), key.modifiers == Modifiers::command());
        if self.screen != Screen::Activity || self.overlay.is_some() || self.activity.read(cx).menu.is_some() || !self.root.is_focused(window) || !opens {
            return false;
        }
        let Some(id) = self.activity.read(cx).previewed.clone() else { return false };
        self.open_previewed(&id, window, cx);
        true
    }

    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn activity_changed(&mut self, cx: &mut Context<Self>) {
        if self.screen == Screen::Activity {
            let items = self.activity_items();
            if let Some(id) = self.activity.update(cx, |a, _| a.gather(items)) {
                self.outbox.seen(&[id]);
            }
        }
    }

    /// What the page reads of `Desktop` to draw: each session's project tile, PR, diff and ask, the open menu's names, and the drawer's session.
    pub(crate) fn activity_side(&self, cx: &App) -> Side {
        let a = self.activity.read(cx);
        let extras = a
            .items()
            .iter()
            .map(|item| {
                let tree = self.prs.get(&item.tree);
                let repo = self.repos.get(&item.tree).filter(|r| !r.files.is_empty());
                let ask = (item.col() == logic::Col::NeedsYou && !item.failed()).then(|| self.agents.pending.iter().find(|p| p.agent_id == item.id).cloned());
                let extra = Extra {
                    mark: self.project_mark(&item.project, item.project_name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()),
                    pr: tree.map(|pr| (pr_glyph(pr), pr.number)),
                    diff: repo.map(|r| r.totals()),
                    ask: ask.flatten(),
                    retry: item.failed() && self.activity_last_prompt(&item.id).is_some(),
                };
                (item.id.clone(), extra)
            })
            .collect();
        let names = a
            .menu
            .into_iter()
            .flat_map(|menu| a.values(menu).into_iter().map(move |(value, _)| (menu, value)))
            .map(|(menu, value)| {
                let name = match menu {
                    Menu::Project => self.repo_name(&value),
                    Menu::Agent => self.agent_name(&value),
                };
                (value, name)
            })
            .collect();
        let drawer = a.previewed.as_deref().and_then(|id| a.item(id)).map(|item| Drawer {
            recap: recap::recap(self.agents.timelines.get(&item.id).map_or(&[][..], Vec::as_slice)),
            files: self
                .repos
                .get(&item.tree)
                .filter(|r| !r.files.is_empty())
                .map(|r| r.files.iter().take(side::FILES_SHOWN).map(|f| (f.path.clone(), f.added, f.removed)).collect()),
            pr: self.prs.get(&item.tree).cloned(),
        });
        Side { extras, observe_only: self.agents.observe_only(), names, drawer }
    }

    /// Every session with a terminal, as the page shows it.
    fn activity_items(&self) -> Vec<Item> {
        let projects = self.projects();
        crate::status::listed(&self.agents.list)
            .into_iter()
            .filter_map(|a| {
                let cwd = &self.terminals.sessions.get(&a.terminal_id)?.info.cwd;
                let project = self.agent_project(a, cwd, &projects).cloned().unwrap_or_else(|| cwd.clone());
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
            Some(Status::NeedsYou) => self
                .agents
                .pending
                .iter()
                .find(|p| p.agent_id == a.id)
                .map(|p| p.ask())
                .unwrap_or_else(|| first_line(self.agents.last_text(&a.id).unwrap_or_default())),
            Some(Status::Failed) => first_line(self.agents.last_result(&a.id).map_or("", |r| r.error.as_str())),
            Some(Status::Working) => a.activity.clone(),
            Some(Status::Done) => first_line(self.agents.last_text(&a.id).unwrap_or_default()),
            Some(Status::Idle) | None => String::new(),
        }
    }
}
