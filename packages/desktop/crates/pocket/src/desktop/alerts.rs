use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::inbox;
use crate::status::{self, Status};
use agents::Event;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use workspace::Tab;

pub struct Alerts {
    pub(crate) viewing: Option<Vec<String>>,
    pub(crate) statuses: HashMap<String, Status>,
    pub(crate) alerted: HashSet<String>,
}

impl Alerts {
    pub fn new() -> Self {
        Self { viewing: None, statuses: HashMap::new(), alerted: HashSet::new() }
    }
}

impl Desktop {
    pub(crate) fn on_agents(&mut self, ev: Event, cx: &mut Context<Self>) {
        if let (Event::Connected, Some(ids)) = (&ev, &self.alerts.viewing) {
            self.outbox.view(ids);
        }
        if let Event::Agents(list) = &ev {
            // Notifications outlive the app, so any listed agent may still have one from before a restart.
            self.alerts.alerted.extend(list.iter().map(|a| a.id.clone()));
        }
        self.agents.apply(ev);
        let agents = &self.agents;
        self.terminals.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
        if self.screen == Screen::Inbox {
            (self.inbox.selected, self.terminal.focused) = inbox::reselect(&inbox::notes(&self.agents), self.terminal.focused.as_deref(), self.inbox.selected);
        }
        self.sync_alerts(cx);
        cx.notify();
    }

    fn sync_alerts(&mut self, cx: &mut App) {
        let now: HashMap<String, Status> = self.agents.list.iter().filter_map(|a| Some((a.id.clone(), Status::of(a)?))).collect();
        let (show, dismiss) = status::alerts(&self.alerts.statuses, &now, &self.alerts.alerted, self.alerts.viewing.as_deref().unwrap_or_default());
        for id in dismiss {
            cx.dismiss_system_notification(&id);
            self.alerts.alerted.remove(&id);
        }
        for id in show.into_iter().filter(|_| !self.capturing) {
            let Some(a) = self.agents.get(&id) else { continue };
            let title = if a.title.is_empty() { theme::provider_name(&a.provider).to_string() } else { a.title.clone() };
            cx.show_system_notification(SystemNotification { tag: id.clone().into(), title: title.into(), body: now[&id].label().into(), actions: Vec::new() });
            self.alerts.alerted.insert(id);
        }
        self.alerts.statuses = now;
    }

    /// The terminals on screen: the worktree's active tab's.
    fn visible_panes(&mut self) -> Vec<String> {
        let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions) else { return Vec::new() };
        match self.workspace(&tree).active() {
            Some(Tab::Term(rows)) => rows.concat(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn sync_view(&mut self, window: &Window, cx: &mut App) {
        let panes = if window.is_window_active() { self.visible_panes() } else { Vec::new() };
        let ids = status::view_set(&panes, &self.agents.list);
        if self.alerts.viewing.as_ref() != Some(&ids) {
            self.outbox.view(&ids);
            self.alerts.viewing = Some(ids);
            self.sync_alerts(cx);
        }
    }
}
