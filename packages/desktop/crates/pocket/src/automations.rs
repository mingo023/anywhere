mod column;
mod detail;
mod editor;
mod glyph;
mod form;
pub(crate) mod logic;
mod parts;
mod rows;
mod run_detail;
mod runs;
mod trigger;

use crate::actions::OpenAutomations;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::util::now_ms;
use agents::automations::{Automation, Run};
use gpui_kit::*;
use logic::{Filter, Row, Tab, after_escape, keep_only, opening_selection, reachable, reselect, run_rows};

/// The popups the Automations screen opens, one at a time.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Menu {
    Add,
    Project,
    Provider,
    More,
}

pub struct AutomationsState {
    pub(crate) tab: Tab,
    pub(crate) filter: Filter,
    pub(crate) only: Option<String>,
    pub(crate) selected: Option<String>,
    pub(crate) selected_run: Option<String>,
    pub(crate) list: UniformListScrollHandle,
    pub(crate) runs_list: ListState,
    pub(crate) rows: Vec<Row>,
    pub(crate) form: editor::Form,
    pub(crate) menu: Option<Menu>,
}

impl AutomationsState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let (form, subs) = editor::Form::new(window, cx);
        let state = Self {
            tab: Tab::default(),
            filter: Filter::default(),
            only: None,
            selected: None,
            selected_run: None,
            list: UniformListScrollHandle::new(),
            runs_list: ListState::new(0, ListAlignment::Top, px(400.)),
            rows: Vec::new(),
            form,
            menu: None,
        };
        (state, subs)
    }

    pub(crate) fn reset(&mut self) {
        (self.tab, self.filter, self.only, self.selected, self.selected_run, self.menu) = (Tab::default(), Filter::default(), None, None, None, None);
        self.form.open = false;
    }

    fn relist(&mut self, items: &[Automation], runs: &[Run]) {
        self.rows = run_rows(items, runs, self.only.as_deref(), now_ms(), &chrono::Local);
        let top = self.runs_list.logical_scroll_top();
        self.runs_list.reset(self.rows.len());
        self.runs_list.scroll_to(top);
    }
}

impl Desktop {
    pub(crate) fn open_automations(&mut self, _: &OpenAutomations, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agents.automations_offered() {
            return;
        }
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Automations;
        self.automations.selected = opening_selection(self.automations.selected.as_deref(), &self.agents.automations.items);
        self.automations_changed();
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// Keeps the selection and the Runs tab's rows true to the latest snapshot.
    pub(crate) fn automations_changed(&mut self) {
        let a = &self.agents.automations;
        self.automations.selected = reselect(self.automations.selected.as_deref(), &a.items);
        self.automations.only = keep_only(self.automations.only.take(), &a.items);
        self.automations.relist(&a.items, &a.runs);
    }

    pub(super) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.automations.tab = tab;
        self.automations.form.open &= tab == Tab::Automations;
        self.close_menus();
        cx.notify();
    }

    pub(super) fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.automations.filter = filter;
        cx.notify();
    }

    pub(super) fn set_only(&mut self, only: Option<String>, cx: &mut Context<Self>) {
        self.automations.only = only;
        let a = &self.agents.automations;
        self.automations.relist(&a.items, &a.runs);
        cx.notify();
    }

    pub(super) fn select_automation(&mut self, id: String, cx: &mut Context<Self>) {
        self.close_menus();
        self.automations.selected = Some(id);
        self.automations.tab = Tab::Automations;
        self.automations.form.open = false;
        cx.notify();
    }

    pub(super) fn select_run(&mut self, id: String, cx: &mut Context<Self>) {
        self.automations.selected_run = Some(id);
        cx.notify();
    }

    pub(super) fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        let was = self.automations.menu;
        self.close_menus();
        self.automations.menu = (was != Some(menu)).then_some(menu);
        cx.notify();
    }

    pub(super) fn toggle_automation(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        self.outbox.automation_enable(id, enabled);
        cx.notify();
    }

    pub(super) fn run_automation(&mut self, id: &str) {
        self.outbox.automation_run(id);
    }

    pub(super) fn open_run_session(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !agent.is_empty() {
            self.focus_agent(agent, window, cx);
        }
    }

    /// What a waiting run asks: the agent's pending request, else the run's own note, else `fallback`.
    pub(super) fn run_ask(&self, run: &Run, fallback: String) -> String {
        let pending = self.agents.pending.iter().find(|p| !run.agent_id.is_empty() && p.agent_id == run.agent_id);
        pending.map(|p| p.ask()).or_else(|| Some(run.why.clone()).filter(|w| !w.is_empty())).unwrap_or(fallback)
    }

    /// Takes the user to answer `run`: its session, or its page when it has none.
    pub(super) fn answer_run(&mut self, run: &Run, window: &mut Window, cx: &mut Context<Self>) {
        if run.agent_id.is_empty() {
            self.automations.selected_run = Some(run.id.clone());
            self.set_tab(Tab::Runs, cx);
        } else {
            self.open_run_session(&run.agent_id, window, cx);
        }
    }

    /// Called when pocketd's caps change: an older one can't fill the screen.
    pub(crate) fn leave_unoffered_automations(&mut self) {
        self.screen = reachable(self.screen, self.agents.automations_offered());
    }

    pub(crate) fn escape_automations(&mut self, cx: &mut Context<Self>) {
        (self.screen, self.automations.form.open) = after_escape(self.screen, self.automations.form.open);
        cx.notify();
    }

    pub(crate) fn delete_automation(&mut self, id: &str) {
        self.outbox.automation_delete(id);
        if self.automations.selected.as_deref() == Some(id) {
            self.automations.selected = None;
        }
    }
}
