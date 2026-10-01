pub(crate) mod close;
pub(crate) mod link;
pub(crate) mod sessions;

use crate::desktop::Desktop;
use crate::terminals::link::Link;
use crate::terminals::sessions::Sessions;
use daemon::{Info, Msg};
use gpui_kit::*;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use workspace::Tab;

pub(crate) enum Intent {
    Tab(String),
    Split(String, bool),
}

/// What a pane's measured size asks of pocketd.
#[derive(Debug, PartialEq)]
pub(crate) enum Fit {
    Now,
    /// The pane is being resized; send the size once it settles.
    Later,
    Same,
}

pub struct Terminals {
    pub(crate) sessions: Sessions,
    pub(crate) intents: VecDeque<Intent>,
    pub(crate) closed: HashSet<String>,
    pub(crate) sized: HashMap<String, (u16, u16)>,
    pending: HashMap<String, (u16, u16)>,
    settle: Option<Task<()>>,
    /// Terminal → worktree while the terminal runs the worktree's setup before its agent.
    pub(crate) setups: HashMap<String, String>,
    /// Terminals pocketd made for this window's creates, with their worktree, until pocketd lists them.
    pub(crate) created: HashMap<String, String>,
    /// The creates whose Terminal was taken in, so pocketd's replay of one after a reconnect adds nothing.
    requests: HashSet<String>,
    /// Terminals whose screen was dropped on reconnect and must be attached again once pocketd lists them.
    reattach: HashSet<String>,
    pub(crate) link: Link,
}

impl Terminals {
    pub fn new() -> Self {
        Self { sessions: Sessions::default(), intents: VecDeque::new(), closed: HashSet::new(), sized: HashMap::new(), pending: HashMap::new(), settle: None, setups: HashMap::new(), created: HashMap::new(), requests: HashSet::new(), reattach: HashSet::new(), link: Link::default() }
    }

    /// The listed terminals closed here that still run, because their close never reached pocketd.
    pub(crate) fn unclosed(&self, items: &[Info]) -> Vec<String> {
        items.iter().filter(|i| self.closed.contains(&i.id)).map(|i| i.id.clone()).collect()
    }

    /// Adopts pocketd's terminal list, minus the ones closed here; returns the ids that still need an attach.
    pub(crate) fn listed(&mut self, items: Vec<Info>) -> Vec<String> {
        let items = items.into_iter().filter(|i| !self.closed.contains(&i.id)).collect();
        let mut attach = self.sessions.sync(items);
        let sessions = &self.sessions;
        let created = &self.created;
        self.setups.retain(|id, _| sessions.get(id).is_some() || created.contains_key(id));
        attach.extend(std::mem::take(&mut self.reattach).into_iter().filter(|id| sessions.get(id).is_some()));
        attach
    }

    /// pocketd went away: the spawns it never answered will never be.
    pub(crate) fn disconnected(&mut self, now: Instant) {
        self.link.down(now);
        self.intents.clear();
    }

    /// pocketd answered again: its terminals may have changed, so forget the screens.
    pub(crate) fn reconnected(&mut self) {
        self.link.up();
        for s in self.sessions.items.iter_mut().filter(|s| s.exit.is_none()) {
            s.term = None;
            self.reattach.insert(s.info.id.clone());
        }
    }

    /// Remembers a spawn only if it reached pocketd, which is the only way it gets answered.
    pub(crate) fn spawn_sent(&mut self, sent: bool, intent: Intent) {
        if sent {
            self.intents.push_back(intent);
        }
    }

    /// Matches a spawned terminal to the oldest pending intent; returns the worktree to adopt it in and the split direction, if any.
    pub(crate) fn spawned(&mut self) -> Option<(String, Option<bool>)> {
        match self.intents.pop_front()? {
            Intent::Tab(tree) => Some((tree, None)),
            Intent::Split(tree, down) => Some((tree, Some(down))),
        }
    }

    pub(crate) fn created(&mut self, request: &str, id: String, tree: String, setup: bool) {
        if !self.requests.insert(request.to_string()) {
            return;
        }
        if setup {
            self.setups.insert(id.clone(), tree.clone());
        }
        self.created.insert(id, tree);
    }

    /// Created terminals pocketd now lists, each with its worktree, once.
    pub(crate) fn arrived(&mut self) -> Vec<(String, String)> {
        let sessions = &self.sessions;
        let listed: Vec<String> = self.created.keys().filter(|id| sessions.get(id).is_some()).cloned().collect();
        listed.into_iter().filter_map(|id| self.created.remove_entry(&id)).collect()
    }

    /// Records an exit and returns whether its pane should close: a clean exit closes, as Terminal.app does; a failed one stays so its error can be read.
    pub(crate) fn exited(&mut self, m: &Msg) -> bool {
        self.sessions.apply(m);
        self.setups.remove(&m.id);
        self.sessions.get(&m.id).is_some_and(|s| s.exit == Some(0))
    }

    /// Forgets a terminal for good, even if pocketd lists it again; returns whether it still runs and pocketd must end it.
    pub(crate) fn close(&mut self, id: &str) -> bool {
        let running = self.sessions.get(id).is_some_and(|s| s.exit.is_none());
        self.sessions.remove(id);
        self.closed.insert(id.to_string());
        self.sized.remove(id);
        self.pending.remove(id);
        self.setups.remove(id);
        self.created.remove(id);
        running
    }

    /// An observer can't end a running terminal; pocketd would refuse while the pane vanished here.
    pub(crate) fn may_close(&self, id: &str, observe: bool) -> bool {
        !observe || !self.sessions.get(id).is_some_and(|s| s.exit.is_none())
    }

    /// Records a pane's size and returns whether pocketd must resize its running terminal to it.
    pub(crate) fn resize(&mut self, id: &str, cols: u16, rows: u16) -> bool {
        if self.sized.get(id) == Some(&(cols, rows)) || !self.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            return false;
        }
        self.sized.insert(id.to_string(), (cols, rows));
        true
    }

    /// A pane's first size goes out at once so it starts at the right width; later ones wait, so a window drag reflows the program once.
    pub(crate) fn fit(&mut self, id: &str, cols: u16, rows: u16) -> Fit {
        let size = (cols, rows);
        if self.pending.get(id).or(self.sized.get(id)) == Some(&size) {
            return Fit::Same;
        }
        if !self.sized.contains_key(id) {
            return if self.resize(id, cols, rows) { Fit::Now } else { Fit::Same };
        }
        self.pending.insert(id.to_string(), size);
        Fit::Later
    }

    /// Takes the sizes that settled and returns the ones pocketd must apply.
    pub(crate) fn settled(&mut self) -> Vec<(String, u16, u16)> {
        let pending = std::mem::take(&mut self.pending);
        pending.into_iter().filter(|(id, (cols, rows))| self.resize(id, *cols, *rows)).map(|(id, (cols, rows))| (id, cols, rows)).collect()
    }

    /// An error without a terminal id is pocketd refusing the oldest spawn.
    pub(crate) fn errored(&mut self, id: &str) {
        if id.is_empty() {
            self.intents.pop_front();
        }
    }
}

impl Desktop {
    pub(crate) fn on_msg(&mut self, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match m.ev.as_str() {
            "terminals" => {
                for id in self.terminals.unclosed(&m.items) {
                    self.daemon.send(json!({"op": "close", "id": id}));
                }
                for id in self.terminals.listed(m.items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                for (id, tree) in self.terminals.arrived() {
                    self.adopt(id, tree, None, window, cx);
                }
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
            }
            "spawned" => {
                self.error = None;
                if let Some((tree, split)) = self.terminals.spawned() {
                    self.adopt(m.id, tree, split, window, cx);
                }
                self.daemon.send(json!({"op": "list"}));
            }
            "error" => {
                self.terminals.errored(&m.id);
                self.error = Some(m.error);
            }
            "exit" => {
                if self.terminals.exited(&m) {
                    self.close_pane(&m.id, cx);
                }
            }
            "up" => {
                self.terminals.reconnected();
                self.terminal.selection = None;
                self.daemon.send(json!({"op": "list"}));
            }
            "down" => {
                self.terminals.disconnected(Instant::now());
                cx.spawn(async |this, cx| {
                    cx.background_executor().timer(link::HINT_AFTER).await;
                    this.update(cx, |_, cx| cx.notify())
                })
                .detach();
            }
            _ => self.terminals.sessions.apply(&m),
        }
        cx.notify();
    }

    fn adopt(&mut self, id: String, tree: String, split: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace(&tree);
        match split {
            Some(down) => w.split(id.clone(), down),
            None => w.add_tab(id.clone()),
        }
        self.show_tree(&tree, &tree);
        self.focus_pane(id, window, cx);
    }

    pub(crate) fn send_spawn(&mut self, op: serde_json::Value, intent: Intent, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let sent = self.daemon.send(op);
        self.terminals.spawn_sent(sent, intent);
        self.error = None;
        cx.notify();
    }

    /// Opens a login shell in the worktree's folder, as a new tab or a split of the active one.
    pub fn new_shell(&mut self, split: Option<bool>, cx: &mut Context<Self>) {
        self.run_in_tree(split, daemon::shell_op, cx);
    }

    pub fn new_agent_tab(&mut self, provider: &str, cx: &mut Context<Self>) {
        self.terminal.tab_menu = false;
        let argv = [provider.to_string()];
        self.run_in_tree(None, |cwd| daemon::agent_op(&argv, cwd), cx);
    }

    fn run_in_tree(&mut self, split: Option<bool>, op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let intent = match split {
            Some(down) => Intent::Split(tree.clone(), down),
            None => Intent::Tab(tree.clone()),
        };
        self.send_spawn(op(&tree), intent, cx);
    }

    pub fn close_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.terminals.may_close(id, self.agents.observe_only()) {
            return;
        }
        if self.terminals.close(id) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
        for w in self.workspaces.values_mut() {
            w.remove(id);
        }
        self.load_active(cx);
        cx.notify();
    }

    /// Ends a session with its terminal.
    pub fn close_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let Some(a) = self.agents.get(id) else { return };
        let term = a.terminal_id.clone();
        if !self.ask_close(vec![term.clone()], cx) {
            self.close_pane(&term, cx);
        }
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let ids = match self.workspace(&tree).tabs.get(i) {
            Some(Tab::Term(rows)) => rows.concat(),
            _ => Vec::new(),
        };
        let observe = self.agents.observe_only();
        if !ids.iter().all(|id| self.terminals.may_close(id, observe)) || self.ask_close(ids, cx) {
            return;
        }
        for id in self.workspace(&tree).close_tab(i) {
            self.close_pane(&id, cx);
        }
        self.load_active(cx);
        cx.notify();
    }

    pub fn fit(&mut self, id: &str, cols: u16, rows: u16, cx: &mut Context<Self>) {
        // Sessions are shared with the user's own window; a capture must not reflow them.
        if self.capturing {
            return;
        }
        match self.terminals.fit(id, cols, rows) {
            Fit::Now => {
                self.daemon.send(json!({"op": "resize", "id": id, "cols": cols, "rows": rows}));
            }
            Fit::Later => {
                self.terminals.settle = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    this.update(cx, |d, _| {
                        for (id, cols, rows) in d.terminals.settled() {
                            d.daemon.send(json!({"op": "resize", "id": id, "cols": cols, "rows": rows}));
                        }
                    })
                    .ok();
                }));
            }
            Fit::Same => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fit, Intent, Terminals};
    use daemon::{Info, Msg};
    use std::collections::HashMap;

    fn info(id: &str) -> Info {
        Info { id: id.into(), cmd: "/bin/zsh".into(), cwd: "/w".into(), ..Default::default() }
    }

    fn exit(id: &str, code: i32) -> Msg {
        Msg { ev: "exit".into(), id: id.into(), code, ..Default::default() }
    }

    fn snapshot(id: &str) -> Msg {
        Msg { ev: "snapshot".into(), id: id.into(), cols: 80, rows: 24, ..Default::default() }
    }

    #[test]
    fn spawned_terminals_open_as_the_tab_asked_for() {
        let mut t = Terminals::new();
        t.intents.push_back(Intent::Tab("/w".into()));
        assert_eq!(t.spawned(), Some(("/w".to_string(), None)));
    }

    #[test]
    fn spawned_terminals_take_intents_in_the_order_they_were_sent() {
        let mut t = Terminals::new();
        t.intents.push_back(Intent::Split("/w".into(), true));
        t.intents.push_back(Intent::Split("/v".into(), false));
        assert_eq!(t.spawned(), Some(("/w".to_string(), Some(true))));
        assert_eq!(t.spawned(), Some(("/v".to_string(), Some(false))));
    }

    #[test]
    fn a_terminal_spawned_with_nothing_pending_is_left_alone() {
        let mut t = Terminals::new();
        assert_eq!(t.spawned(), None);
        assert!(t.setups.is_empty());
    }

    #[test]
    fn a_failed_spawn_drops_its_intent_but_a_terminal_error_does_not() {
        let mut t = Terminals::new();
        t.intents.push_back(Intent::Tab("/w".into()));
        t.intents.push_back(Intent::Tab("/v".into()));
        t.errored("a");
        t.errored("");
        assert_eq!(t.spawned(), Some(("/v".to_string(), None)));
    }

    #[test]
    fn closed_terminals_stay_closed_when_pocketd_still_lists_them() {
        let mut t = Terminals::new();
        t.closed.insert("a".into());
        assert_eq!(t.listed(vec![info("a"), info("b")]), vec!["b"]);
        assert!(t.sessions.get("a").is_none());
    }

    #[test]
    fn a_setup_ends_when_its_terminal_leaves_the_list() {
        let mut t = Terminals::new();
        t.created("r1", "a".into(), "/w".into(), true);
        t.created("r2", "b".into(), "/v".into(), true);
        t.listed(vec![info("a"), info("b")]);
        t.arrived();
        t.listed(vec![info("b")]);
        assert_eq!(t.setups, HashMap::from([("b".to_string(), "/v".to_string())]));
    }

    #[test]
    fn a_clean_exit_closes_its_pane_and_a_failed_one_stays_to_be_read() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b")]);
        assert!(t.exited(&exit("a", 0)));
        assert!(!t.exited(&exit("b", 1)));
        assert_eq!(t.sessions.get("b").and_then(|s| s.exit), Some(1));
    }

    #[test]
    fn the_exit_of_a_terminal_this_window_does_not_hold_closes_nothing() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        assert!(!t.exited(&exit("x", 0)));
        assert_eq!(t.sessions.get("a").map(|s| s.exit), Some(None));
    }

    #[test]
    fn a_setup_ends_when_its_terminal_exits() {
        let mut t = Terminals::new();
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        t.exited(&exit("a", 1));
        assert!(t.setups.is_empty());
    }

    #[test]
    fn closing_asks_pocketd_to_end_only_a_running_terminal() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b")]);
        t.exited(&exit("b", 1));
        assert_eq!((t.close("a"), t.close("b"), t.close("x")), (true, false, false));
    }

    #[test]
    fn a_closed_terminal_leaves_the_window_with_its_setup() {
        let mut t = Terminals::new();
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        t.close("a");
        assert_eq!(t.listed(vec![info("a")]), Vec::<String>::new());
        assert!(t.sessions.get("a").is_none());
        assert!(t.setups.is_empty());
    }

    #[test]
    fn a_running_terminal_is_resized_once_per_size() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        assert_eq!((t.resize("a", 80, 24), t.resize("a", 80, 24), t.resize("a", 100, 24)), (true, false, true));
    }

    #[test]
    fn exited_and_unknown_terminals_are_not_resized() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        t.exited(&exit("a", 1));
        assert_eq!((t.resize("a", 80, 24), t.resize("x", 80, 24)), (false, false));
    }

    #[test]
    fn a_first_size_goes_at_once_and_later_ones_wait_to_settle() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        assert_eq!(t.fit("a", 80, 24), Fit::Now);
        assert_eq!(t.fit("a", 80, 24), Fit::Same);
        assert_eq!((t.fit("a", 90, 24), t.fit("a", 100, 24), t.fit("a", 100, 24)), (Fit::Later, Fit::Later, Fit::Same));
        assert_eq!(t.settled(), vec![("a".to_string(), 100, 24)]);
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn a_resize_that_returns_to_the_sent_size_sends_nothing() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        t.fit("a", 80, 24);
        t.fit("a", 90, 24);
        assert_eq!(t.fit("a", 80, 24), Fit::Later);
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn a_closed_terminal_drops_its_pending_size() {
        let mut t = Terminals::new();
        t.listed(vec![info("a")]);
        t.fit("a", 80, 24);
        t.fit("a", 90, 24);
        t.close("a");
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn reconnected_reattaches_every_listed_terminal() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b"), info("c")]);
        t.sessions.apply(&snapshot("a"));
        t.exited(&exit("c", 1));
        t.reconnected();
        assert!(t.sessions.get("a").is_some_and(|s| s.term.is_none()));
        let mut attach = t.listed(vec![info("a"), info("b")]);
        attach.sort();
        assert_eq!(attach, vec!["a", "b"]);
        assert_eq!(t.listed(vec![info("a"), info("b")]), Vec::<String>::new());
    }

    #[test]
    fn a_terminal_missing_after_reconnect_is_not_running() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b")]);
        t.reconnected();
        assert_eq!(t.listed(vec![info("b")]), vec!["b"]);
        assert!(t.sessions.get("a").is_none());
    }

    #[test]
    fn losing_pocketd_drops_spawns_it_never_answered() {
        let mut t = Terminals::new();
        t.intents.push_back(Intent::Tab("/w".into()));
        t.disconnected(std::time::Instant::now());
        assert_eq!(t.spawned(), None);
    }

    #[test]
    fn a_spawn_sent_just_before_up_is_still_adopted() {
        let mut t = Terminals::new();
        t.disconnected(std::time::Instant::now());
        t.spawn_sent(true, Intent::Tab("/w".into()));
        t.reconnected();
        assert_eq!(t.spawned(), Some(("/w".to_string(), None)));
    }

    #[test]
    fn a_close_that_never_arrived_is_sent_again_when_pocketd_lists_the_terminal() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b")]);
        t.close("a");
        assert_eq!(t.unclosed(&[info("a"), info("b")]), vec!["a"]);
    }

    #[test]
    fn failed_spawn_send_records_no_intent() {
        let mut t = Terminals::new();
        t.spawn_sent(false, Intent::Tab("/w".into()));
        t.spawn_sent(true, Intent::Tab("/v".into()));
        assert_eq!(t.spawned(), Some(("/v".to_string(), None)));
    }

    #[test]
    fn a_creating_reply_adopts_its_terminal_and_marks_setup() {
        let mut t = Terminals::new();
        t.created("r1", "a".into(), "/w/fix".into(), true);
        t.created("r2", "b".into(), "/w/v".into(), false);
        t.listed(vec![info("x")]);
        assert!(t.arrived().is_empty());
        assert_eq!(t.setups, HashMap::from([("a".to_string(), "/w/fix".to_string())]));
        t.listed(vec![info("x"), info("a")]);
        assert_eq!(t.arrived(), vec![("a".to_string(), "/w/fix".to_string())]);
        assert!(t.arrived().is_empty());
    }

    #[test]
    fn a_replayed_creating_adopts_its_terminal_once() {
        let mut t = Terminals::new();
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        assert_eq!(t.arrived(), vec![("a".to_string(), "/w".to_string())]);
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        assert!(t.arrived().is_empty());
    }

    #[test]
    fn an_observer_can_drop_an_exited_terminal_but_not_end_a_running_one() {
        let mut t = Terminals::new();
        t.listed(vec![info("a"), info("b")]);
        t.exited(&exit("b", 1));
        assert_eq!((t.may_close("a", true), t.may_close("b", true), t.may_close("a", false)), (false, true, true));
    }
}
