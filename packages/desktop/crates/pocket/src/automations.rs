mod column;
mod detail;
mod editor;
mod form;
mod glyph;
pub(crate) mod logic;
mod parts;
mod rows;
mod run_detail;
mod runs;
mod side;
mod trigger;

use crate::composer::{Chips, ComposerEvent};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Screen, popup_closing};
use crate::modals::new_session::default_first;
use crate::util::now_ms;
use agents::automations::{Automation, Automations as Snapshot, Run};
use agents::{Decision, Permission};
use gpui_kit::*;
use logic::{Entry, Folds, Row, Tab, after_escape, entries, keep_only, opening_selection, reachable, reselect, run_rows};
pub(crate) use side::{Project, Side};
use std::sync::Arc;

/// The popups the Automations screen opens, one at a time.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Menu {
    Add,
    Project,
    Checkout,
    Branch,
    More,
}

pub(crate) enum AutomationsEvent {
    CloseMenus,
    ToggleMenu(Menu),
    LoadBranches(String),
    Save,
    Toggle { id: String, enabled: bool },
    Run(String),
    AskDelete(String),
    OpenSession(String),
    Resolve { request: String, decision: Decision },
}

pub struct Automations {
    pub(crate) tab: Tab,
    pub(crate) folds: Folds,
    /// The Automations list's rows, as of the latest snapshot.
    pub(crate) entries: Vec<Entry>,
    pub(crate) only: Option<String>,
    pub(crate) selected: Option<String>,
    pub(crate) selected_run: Option<String>,
    pub(crate) list: ListState,
    pub(crate) runs_list: ListState,
    pub(crate) rows: Vec<Row>,
    pub(crate) form: editor::Form,
    menu: Option<Menu>,
    data: Arc<Snapshot>,
    side: Side,
    chips: Option<Chips>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<AutomationsEvent> for Automations {}

impl Automations {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (form, _subs) = editor::Form::new(window, cx);
        Self {
            tab: Tab::default(),
            folds: Folds::default(),
            entries: Vec::new(),
            only: None,
            selected: None,
            selected_run: None,
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            runs_list: ListState::new(0, ListAlignment::Top, px(400.)),
            rows: Vec::new(),
            form,
            menu: None,
            data: Arc::default(),
            side: Side::default(),
            chips: None,
            _subs,
        }
    }

    pub(crate) fn reset(&mut self) {
        (self.tab, self.folds, self.only, self.selected, self.selected_run, self.menu) = (Tab::default(), Folds::default(), None, None, None, None);
        self.form.open = false;
    }

    pub(crate) fn set(&mut self, side: Side) {
        self.side = side;
    }

    pub(crate) fn set_chips(&mut self, chips: Option<Chips>) {
        self.chips = chips;
    }

    /// The screen opens on the snapshot's first automation unless one is already selected.
    pub(crate) fn opened(&mut self, data: Arc<Snapshot>) {
        self.selected = opening_selection(self.selected.as_deref(), &data.items);
        self.changed(data);
    }

    /// Keeps the selection and the Runs tab's rows true to the latest snapshot.
    pub(crate) fn changed(&mut self, data: Arc<Snapshot>) {
        self.data = data;
        let a = Arc::clone(&self.data);
        self.selected = reselect(self.selected.as_deref(), &a.items);
        self.only = keep_only(self.only.take(), &a.items);
        self.regroup(&a.items);
        self.relist(&a.items, &a.runs);
    }

    fn regroup(&mut self, items: &[Automation]) {
        let entries = entries(items, self.folds);
        // A heading and a row differ in height, so the list measures afresh whenever a row changes kind.
        if entries != self.entries {
            let top = self.list.logical_scroll_top();
            self.list.reset(entries.len());
            self.list.scroll_to(top);
        }
        self.entries = entries;
    }

    fn relist(&mut self, items: &[Automation], runs: &[Run]) {
        self.rows = run_rows(items, runs, self.only.as_deref(), now_ms(), &chrono::Local);
        let top = self.runs_list.logical_scroll_top();
        self.runs_list.reset(self.rows.len());
        self.runs_list.scroll_to(top);
    }

    pub(crate) fn set_editing(&mut self, editing: bool, cx: &mut Context<Self>) {
        if self.form.open != editing {
            self.form.open = editing;
            cx.notify();
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    pub(crate) fn shut_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    /// Closes the menus here, and asks `Desktop` to close the rest.
    fn shut(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        cx.emit(AutomationsEvent::CloseMenus);
    }

    pub(super) fn popup(&self, id: &'static str, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        popup_closing(id, width, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(AutomationsEvent::CloseMenus)))
    }

    pub(super) fn agent_name(&self, provider: &str) -> String {
        self.side.labels.get(provider).cloned().unwrap_or_else(|| theme::provider_name(provider).into())
    }

    pub(super) fn repo_name(&self, folder: &str) -> String {
        self.side.projects.get(folder).map(|p| p.name.clone()).unwrap_or_default()
    }

    pub(super) fn repo_mark(&self, folder: &str) -> ui::Mark {
        self.side.projects.get(folder).map_or_else(|| ui::Mark::Letters(String::new()), |p| p.mark.clone())
    }

    pub(super) fn repo_tile_for(&self, folder: &str, size: f32) -> Div {
        ui::repo_tile(&self.repo_mark(folder), size, false, None)
    }

    pub(super) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.form.open &= tab == Tab::Automations;
        self.shut(cx);
        cx.notify();
    }

    pub(super) fn fold_group(&mut self, active: bool, cx: &mut Context<Self>) {
        self.folds.toggle(active);
        let data = Arc::clone(&self.data);
        self.regroup(&data.items);
        cx.notify();
    }

    pub(super) fn set_only(&mut self, only: Option<String>, cx: &mut Context<Self>) {
        self.only = only;
        let data = Arc::clone(&self.data);
        self.relist(&data.items, &data.runs);
        cx.notify();
    }

    pub(super) fn select_automation(&mut self, id: String, cx: &mut Context<Self>) {
        self.shut(cx);
        self.selected = Some(id);
        self.tab = Tab::Automations;
        self.form.open = false;
        cx.notify();
    }

    pub(super) fn select_run(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected_run = Some(id);
        cx.notify();
    }

    pub(super) fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        cx.emit(AutomationsEvent::ToggleMenu(menu));
    }

    /// Opens `menu`, or closes the menus with `None`; the branch menu starts on an empty, focused search.
    pub(crate) fn set_menu(&mut self, menu: Option<Menu>, window: &mut Window, cx: &mut Context<Self>) {
        self.form.composer.update(cx, |c, cx| c.shut_menu(cx));
        self.menu = menu;
        if menu == Some(Menu::Branch) {
            self.form.branch_search.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn toggle_automation(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        cx.emit(AutomationsEvent::Toggle { id: id.to_string(), enabled });
        cx.notify();
    }

    pub(super) fn run_automation(&mut self, id: &str, cx: &mut Context<Self>) {
        cx.emit(AutomationsEvent::Run(id.to_string()));
        cx.notify();
    }

    pub(super) fn open_run_session(&mut self, agent: &str, cx: &mut Context<Self>) {
        if !agent.is_empty() {
            cx.emit(AutomationsEvent::OpenSession(agent.to_string()));
        }
    }

    /// What a waiting run asks: the agent's pending request, else the run's own note, else `fallback`.
    pub(super) fn run_ask(&self, run: &Run, fallback: String) -> String {
        let pending = self.pending_of(run);
        pending.map(|p| p.ask()).or_else(|| Some(run.why.clone()).filter(|w| !w.is_empty())).unwrap_or(fallback)
    }

    pub(super) fn pending_of(&self, run: &Run) -> Option<&Permission> {
        self.side.asks.get(&run.agent_id).filter(|_| !run.agent_id.is_empty())
    }

    /// Takes the user to answer `run`: its session, or its page when it has none.
    pub(super) fn answer_run(&mut self, run: &Run, cx: &mut Context<Self>) {
        if run.agent_id.is_empty() {
            self.selected_run = Some(run.id.clone());
            self.set_tab(Tab::Runs, cx);
        } else {
            self.open_run_session(&run.agent_id, cx);
        }
    }

    pub(crate) fn forget(&mut self, id: &str) {
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
    }
}

impl Desktop {
    pub(crate) fn open_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agents.automations_offered() {
            return;
        }
        self.close_menus(cx);
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Automations;
        let data = Arc::clone(&self.agents.automations);
        self.automations.update(cx, |a, _| a.opened(data));
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn automations_changed(&mut self, cx: &mut App) {
        let data = Arc::clone(&self.agents.automations);
        self.automations.update(cx, |a, _| a.changed(data));
    }

    /// What the screen draws from `Desktop`'s own state.
    pub(crate) fn automations_side(&self, cx: &App) -> Side {
        let a = self.automations.read(cx);
        let data = &self.agents.automations;
        let current = a.form.values(cx).folder;
        let folders = self.store.without_scratch(self.projects(cx));
        let wanted = data.items.iter().map(|x| x.folder.as_str()).chain(folders.iter().map(String::as_str)).chain(std::iter::once(current.as_str()));
        let projects = wanted
            .filter(|f| !f.is_empty())
            .map(|f| {
                let name = self.repo_name(f);
                (f.to_string(), Project { mark: self.project_mark(f, crate::util::initials(&name)), name })
            })
            .collect();
        let labels = agents::providers::IDS.into_iter().filter_map(|p| self.store.agents.label(p).map(|l| (p.to_string(), l))).collect();
        let sessions = data.items.iter().flat_map(|x| [x.created_by.as_str(), x.bound.as_str()]);
        let titles = sessions.filter_map(|id| self.agents.get(id).map(|s| (id.to_string(), s.title.clone()))).collect();
        let asks = data
            .waiting()
            .filter_map(|r| self.agents.pending.iter().find(|p| !r.agent_id.is_empty() && p.agent_id == r.agent_id))
            .map(|p| (p.agent_id.clone(), p.clone()))
            .collect();
        let folder = self.project.clone().filter(|p| !self.store.is_scratch(p)).or_else(|| folders.first().cloned()).unwrap_or_default();
        let lead = crate::desktop::chrome::past_lights(self.store.appearance.zoom_factor(), self.lead_width(cx), 18.);
        Side { labels, projects, folders, titles, asks, defaults: self.store.automations.clone(), folder, lead }
    }

    /// The editor's chips need the model lists and settings only `Desktop` holds.
    fn automations_chips(&self, cx: &App) -> Option<Chips> {
        let (c, pick, project) = self.automations.read(cx).chip_inputs(cx)?;
        Some(self.composer_chips(&c, &pick, project, cx))
    }

    /// The right-hand page of the Automations screen.
    pub(crate) fn automations_page(&mut self, cx: &mut Context<Self>) -> Div {
        let side = self.automations_side(cx);
        let chips = self.automations_chips(cx);
        let bar = self.automations.update(cx, |a, cx| {
            a.set(side);
            a.set_chips(chips);
            a.bar(cx)
        });
        let bar = bar.map(|(crumb, right)| self.detail_bar(crumb, right, cx));
        div().flex_1().min_w_0().flex().flex_col().children(bar).child(self.automations.clone())
    }

    pub(crate) fn on_automations_event(&mut self, _: &Entity<Automations>, ev: &AutomationsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            AutomationsEvent::CloseMenus => _ = self.close_menus(cx),
            AutomationsEvent::ToggleMenu(menu) => {
                let was = self.automations.read(cx).menu;
                self.close_menus(cx);
                self.automations.update(cx, |a, cx| a.set_menu((was != Some(*menu)).then_some(*menu), window, cx));
            }
            AutomationsEvent::LoadBranches(folder) => self.load_branches(folder.clone(), cx),
            AutomationsEvent::Save => self.save_automation(cx),
            AutomationsEvent::Toggle { id, enabled } => self.outbox.local.automation_enable(id, *enabled),
            AutomationsEvent::Run(id) => self.outbox.local.automation_run(id),
            AutomationsEvent::AskDelete(id) => self.ask_delete_automation(id.clone(), cx),
            AutomationsEvent::OpenSession(agent) => self.focus_agent(agent, window, cx),
            AutomationsEvent::Resolve { request, decision } => self.outbox.resolve(request, *decision),
        }
        cx.notify();
    }

    pub(crate) fn watch_automations(automations: &Entity<Automations>, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let composer = automations.read(cx).form.composer.clone();
        vec![
            cx.subscribe_in(automations, window, Self::on_automations_event),
            cx.observe(automations, |_, _, cx| cx.notify()),
            cx.subscribe_in(&composer, window, |this, c, ev: &ComposerEvent, window, cx| this.composer_event(c, ev, window, cx)),
            cx.observe(&composer, |_, _, cx| cx.notify()),
        ]
    }

    pub(crate) fn ask_delete_automation(&mut self, id: String, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.ask(Confirm::DeleteAutomation(id), cx);
        cx.notify();
    }

    /// Reads `folder`'s local branches off the UI thread.
    fn load_branches(&mut self, folder: String, cx: &mut Context<Self>) {
        if folder.is_empty() {
            return;
        }
        let (runner, dir) = (hosts::runner_for(&folder), folder.clone());
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&runner, &dir, false).map(|r| r.branch).unwrap_or_default();
            let mut branches = git::dated_branches(&runner, &dir);
            default_first(&mut branches, "", &current);
            branches
        });
        cx.spawn(async move |this, cx| {
            let branches = task.await;
            this.update(cx, |d, cx| d.automations.update(cx, |a, cx| a.set_branches(folder, branches, cx))).ok();
        })
        .detach();
    }

    fn save_automation(&mut self, cx: &mut Context<Self>) {
        let v = self.automations.read(cx).form.values(cx);
        let (own, efforts) = (self.store.agents.provider(v.provider), self.launch_efforts(v.provider, &v.model));
        let v = v.settled(self.store.agents.access(), &own.model, &own.effort, &efforts);
        let Ok(draft) = v.draft() else { return };
        self.outbox.local.automation_save(draft);
        self.automations.update(cx, |a, cx| a.set_editing(false, cx));
        cx.notify();
    }

    /// Opens the editor on automation `id`, or on a new one.
    pub(crate) fn edit_automation(&mut self, id: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.screen = Screen::Automations;
        let side = self.automations_side(cx);
        self.automations.update(cx, |a, cx| {
            a.set(side);
            a.edit(id, window, cx);
        });
        cx.notify();
    }

    /// Called when pocketd's caps change: an older one can't fill the screen.
    pub(crate) fn leave_unoffered_automations(&mut self) {
        self.screen = reachable(self.screen, self.agents.automations_offered());
    }

    pub(crate) fn escape_automations(&mut self, cx: &mut Context<Self>) {
        let editing = self.automations.read(cx).form.open;
        let (screen, editing) = after_escape(self.screen, editing);
        self.screen = screen;
        self.automations.update(cx, |a, cx| a.set_editing(editing, cx));
        cx.notify();
    }

    pub(crate) fn delete_automation(&mut self, id: &str, cx: &mut Context<Self>) {
        self.outbox.local.automation_delete(id);
        self.automations.update(cx, |a, _| a.forget(id));
    }
}
