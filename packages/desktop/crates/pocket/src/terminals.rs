pub(crate) mod bell;
pub(crate) mod close;
pub(crate) mod link;
pub(crate) mod sessions;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::terminals::bell::Bell;
use crate::terminals::link::Link;
use crate::terminals::sessions::Sessions;
use agents::locals::is_local;
use daemon::{Info, Msg};
use hosts::session::{Lists, untag};
use gpui_kit::*;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use workspace::tree::PaneId;
use workspace::{Doc, Place, Tab};

/// A spawn awaiting its terminal: the worktree to adopt it in, and where its tab goes.
#[derive(Debug, PartialEq)]
pub(crate) struct Intent(pub(crate) String, pub(crate) Place);

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
    /// Terminals pocketd made for this window's creates, until pocketd lists them.
    pub(crate) created: HashMap<String, Created>,
    /// The creates whose Terminal was taken in, so pocketd's replay of one after a reconnect adds nothing.
    requests: HashSet<String>,
    /// Terminals whose screen was dropped on reconnect and must be attached again once pocketd lists them.
    reattach: HashSet<String>,
    pub(crate) link: Link,
    pub(crate) bell: Bell,
    /// Terminals ended from their pane, which stays open on its last screen.
    killed: HashSet<String>,
    pub(crate) lists: Lists,
}

/// A created terminal's worktree, and the pane it opens in if one was asked for.
#[derive(Debug, PartialEq)]
pub(crate) struct Created {
    pub(crate) tree: String,
    pub(crate) pane: Option<PaneId>,
}

impl Terminals {
    pub fn new(config: term::Config) -> Self {
        Self { sessions: Sessions { config, ..Sessions::default() }, intents: VecDeque::new(), closed: HashSet::new(), sized: HashMap::new(), pending: HashMap::new(), settle: None, setups: HashMap::new(), created: HashMap::new(), requests: HashSet::new(), reattach: HashSet::new(), link: Link::default(), bell: Bell::default(), killed: HashSet::new(), lists: Lists::default() }
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
        self.intents.retain(|i| hosts::split(&i.0).0.is_some());
    }

    /// pocketd answered again: its terminals may have changed, so forget the screens.
    pub(crate) fn reconnected(&mut self) {
        self.link.up();
        self.forget_screens(None);
    }

    /// A host's pocketd went away: its spawns go unanswered, and its terminals' screens are gone until it lists them again.
    pub(crate) fn host_down(&mut self, host: &str) {
        self.intents.retain(|i| hosts::split(&i.0).0 != Some(host));
        self.forget_screens(Some(host));
    }

    /// Drops the screens of the running terminals pocketd `host` holds, to attach again once it lists them.
    pub(crate) fn forget_screens(&mut self, host: Option<&str>) {
        for s in self.sessions.items.iter_mut().filter(|s| s.exit.is_none() && untag(&s.info.id).0 == host) {
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

    /// Matches a terminal pocketd `host` spawned to the oldest intent sent to it.
    pub(crate) fn spawned(&mut self, host: Option<&str>) -> Option<Intent> {
        let at = self.intents.iter().position(|i| hosts::split(&i.0).0 == host)?;
        self.intents.remove(at)
    }

    pub(crate) fn created(&mut self, request: &str, id: String, tree: String, setup: bool) {
        if !self.requests.insert(request.to_string()) {
            return;
        }
        if setup {
            self.setups.insert(id.clone(), tree.clone());
        }
        self.created.insert(id, Created { tree, pane: None });
    }

    /// Opens a created terminal in `pane` once it arrives.
    pub(crate) fn aim(&mut self, id: &str, pane: PaneId) {
        if let Some(c) = self.created.get_mut(id) {
            c.pane = Some(pane);
        }
    }

    /// Created terminals pocketd now lists, once.
    pub(crate) fn arrived(&mut self) -> Vec<(String, Created)> {
        let sessions = &self.sessions;
        let listed: Vec<String> = self.created.keys().filter(|id| sessions.get(id).is_some()).cloned().collect();
        listed.into_iter().filter_map(|id| self.created.remove_entry(&id)).collect()
    }

    /// Records an exit and returns whether its pane should close. Only a failed setup or a killed terminal stays so it can be read: a shell's `exit` passes on its last command's status, so any other code means nothing went wrong.
    pub(crate) fn exited(&mut self, m: &Msg) -> bool {
        self.sessions.apply(m);
        let setup = self.setups.remove(&m.id).is_some();
        let killed = self.killed(&m.id);
        self.sessions.get(&m.id).is_some_and(|s| !killed && !(setup && s.failed()))
    }

    pub(crate) fn killed(&self, id: &str) -> bool {
        self.killed.contains(id)
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
        self.killed.remove(id);
        running
    }

    /// Keeps the pane of a terminal about to be ended until it is closed; returns whether it still runs and pocketd must end it.
    pub(crate) fn kill(&mut self, id: &str) -> bool {
        let running = self.sessions.get(id).is_some_and(|s| s.exit.is_none());
        if running {
            self.killed.insert(id.to_string());
        }
        running
    }

    /// An observer can't end a running terminal; pocketd would refuse while the pane vanished here.
    pub(crate) fn may_close(&self, id: &str, observe: bool) -> bool {
        !observe || !self.sessions.get(id).is_some_and(|s| s.exit.is_none())
    }

    /// pocketd drops a terminal once it exits, so input to it would only come back as an error. One not listed yet was just spawned and runs.
    pub(crate) fn may_send(&self, id: &str, observe: bool) -> bool {
        !observe && !self.sessions.get(id).is_some_and(|s| s.exit.is_some())
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

    /// An error without a terminal id is pocketd `host` refusing the oldest spawn sent to it.
    pub(crate) fn errored(&mut self, host: Option<&str>, id: &str) {
        if id.is_empty() {
            self.spawned(host);
        }
    }
}

impl Desktop {
    pub(crate) fn on_msg(&mut self, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        self.on_msg_from(None, m, window, cx);
    }

    /// A message from this Mac's pocketd, or for `Some` a host's with its ids already tagged.
    pub(crate) fn on_msg_from(&mut self, host: Option<&str>, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match (m.ev.as_str(), host) {
            ("terminals", _) => {
                let items = self.terminals.lists.listed(host, m.items);
                for id in self.terminals.unclosed(&items) {
                    self.daemon.send(json!({"op": "close", "id": id}));
                }
                for id in self.terminals.listed(items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                let restoring = !self.panels.restored;
                match host {
                    None => self.restore_layouts(),
                    Some(h) => self.restore_host_layouts(h),
                }
                for (id, c) in self.terminals.arrived() {
                    let local = self.terminals.sessions.get(&id).map(|s| s.info.local.clone()).filter(|l| !l.is_empty());
                    self.adopt(id, local.unwrap_or(c.tree), Place::Pane(c.pane), window, cx);
                }
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
                // Restored docs load into the worktree on screen, which needs the project chosen above.
                if restoring {
                    self.load_active(cx);
                }
            }
            ("spawned", _) => {
                self.error = None;
                if let Some(Intent(tree, place)) = self.terminals.spawned(host) {
                    self.adopt(m.id, tree, place, window, cx);
                }
                self.daemon.send_to(host, json!({"op": "list"}));
            }
            ("error", _) => {
                self.terminals.errored(host, &m.id);
                self.error = Some(m.error);
            }
            ("exit", _) => {
                self.sign_in_ended(&m.id, cx);
                if self.terminals.exited(&m) {
                    self.close_pane(&m.id, cx);
                }
            }
            ("up", Some(h)) => {
                self.terminals.forget_screens(Some(h));
                self.daemon.send_to(host, json!({"op": "list"}));
            }
            ("down", Some(h)) => self.terminals.host_down(h),
            ("up", None) => {
                self.terminals.reconnected();
                self.terminal.selection = None;
                self.daemon.send(json!({"op": "list"}));
            }
            ("down", None) => {
                self.terminals.disconnected(Instant::now());
                if channel::is_release() {
                    cx.spawn(async |this, cx| {
                        let service = cx.background_executor().spawn(async { daemon::service::status() }).await;
                        this.update(cx, |d, _| d.terminals.link.service = Some(service))
                    })
                    .detach();
                }
                cx.spawn(async |this, cx| {
                    cx.background_executor().timer(link::HINT_AFTER).await;
                    this.update(cx, |_, cx| cx.notify())
                })
                .detach();
            }
            _ => {
                if self.terminals.sessions.apply(&m) {
                    self.ring(&m.id, cx);
                }
            }
        }
        cx.notify();
    }

    /// Opens a terminal in its worktree, focusing it only if that worktree is still on screen: it arrives after the user may have moved on.
    fn adopt(&mut self, id: String, tree: String, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace(&tree).open_term(id.clone(), place);
        if self.place().as_deref() == Some(tree.as_str()) {
            self.show_tree(&tree);
            self.load_active(cx);
            self.focus_pane(id, window, cx);
        }
        self.save_soon(cx);
    }

    pub(crate) fn send_spawn(&mut self, op: serde_json::Value, intent: Intent, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let host = hosts::split(&intent.0).0.map(str::to_string);
        let sent = self.daemon.send_to(host.as_deref(), op);
        self.terminals.spawn_sent(sent, intent);
        self.error = host.filter(|_| !sent).map(|h| format!("{} isn't connected", self.host_name(&h)));
        cx.notify();
    }

    /// Sends a create to the pocketd holding `spec`'s project; `None`, with the error shown, when that's a host that isn't connected.
    pub(crate) fn send_create(&mut self, mut spec: serde_json::Value) -> Option<String> {
        self.accounts.offer(&mut spec);
        match self.outbox.create(spec, |h| self.store.host_registry(h)) {
            Ok(request) => Some(request),
            Err(host) => {
                self.error = Some(format!("{} isn't connected", self.host_name(&host)));
                None
            }
        }
    }

    /// Opens the shell Settings › Terminal names in the worktree's folder, its tab at `place`.
    pub fn new_shell(&mut self, place: Place, cx: &mut Context<Self>) {
        let t = &self.store.terminal;
        let (shell, args) = t.shell_command(daemon::login_shell(), store::prefs::terminal::runs);
        let missing = t.skipped(&shell).map(|p| p.trim().to_string());
        self.run_in_tree(place, &[], |cwd| daemon::shell_op(&shell, args, cwd), cx);
        if let Some(missing) = missing {
            self.error = Some(format!("Can't run {missing}, so this terminal opened your login shell"));
        }
    }

    /// A shell, or the default agent when Settings › Agents says new tabs open one.
    pub(crate) fn open_tab(&mut self, place: Place, cx: &mut Context<Self>) {
        if self.store.agents.tabs_open_agent {
            return self.new_agent_tab(self.store.agents.default_agent(), place, cx);
        }
        self.new_shell(place, cx);
    }

    pub fn new_agent_tab(&mut self, provider: &str, place: Place, cx: &mut Context<Self>) {
        let argv = [provider.to_string()];
        self.run_in_tree(place, &argv, |cwd| daemon::agent_op(&argv, cwd), cx);
    }

    /// Spawns `op` in the worktree's folder; a host's folder gets its login shell running `argv` instead.
    pub(crate) fn run_in_tree(&mut self, place: Place, argv: &[String], op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        let Some(cwd) = self.folder_of(&tree) else { return };
        let mut op = match hosts::split(&cwd) {
            (Some(_), path) => hosts::session::spawn_op(path, argv),
            (None, _) => op(&cwd),
        };
        if is_local(&tree) {
            op["local"] = tree.clone().into();
        }
        self.send_spawn(op, Intent(tree, place), cx);
    }

    pub fn close_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.terminals.may_close(id, self.agents.observe_only()) {
            return;
        }
        if self.terminals.close(id) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
        if self.terminal.menu.as_ref().is_some_and(|(p, _)| p == id) {
            self.terminal.menu = None;
        }
        let ids = [id.to_string()];
        for w in self.workspaces.values_mut() {
            w.remove(&ids);
        }
        self.load_active(cx);
        self.save_soon(cx);
        cx.notify();
    }

    /// Ends a terminal's processes but leaves its pane on the last screen.
    pub fn kill_pane(&mut self, id: &str) {
        if self.terminals.may_send(id, self.agents.observe_only()) && self.terminals.kill(id) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
    }

    /// Ends a session with its terminal.
    pub fn close_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let Some(a) = self.agents.get(id) else { return };
        let term = a.terminal_id.clone();
        self.close_terminal(&term, cx);
    }

    /// Closes a terminal and its tabs, asking first if it is busy.
    pub fn close_terminal(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.terminals.may_close(id, self.agents.observe_only()) && !self.ask_close(vec![id.to_string()], cx) {
            self.close_pane(id, cx);
        }
    }

    pub fn close_tab(&mut self, pane: PaneId, i: usize, cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        let ids = match self.workspace(&tree).tree.pane(pane).and_then(|p| p.tabs.get(i)).cloned() {
            Some(Tab::Term(id)) => vec![id],
            Some(Tab::Doc(Doc::File(path))) if self.preview.dirty(&path) => {
                self.confirm = Some(Confirm::CloseFile(path));
                self.overlay = Some(Overlay::Confirm);
                cx.notify();
                return;
            }
            _ => Vec::new(),
        };
        let observe = self.agents.observe_only();
        if !ids.iter().all(|id| self.terminals.may_close(id, observe)) || self.ask_close(ids.clone(), cx) {
            return;
        }
        self.workspace(&tree).close(pane, i);
        for id in ids {
            self.close_pane(&id, cx);
        }
        self.load_active(cx);
        self.save_soon(cx);
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
    use super::{Created, Fit, Intent, Terminals};
    use daemon::{Info, Msg};
    use std::collections::HashMap;
    use workspace::Place;
    use workspace::tree::Edge;

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
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
        assert_eq!(t.spawned(None), Some(Intent("/w".into(), Place::Pane(None))));
    }

    #[test]
    fn spawned_terminals_take_intents_in_the_order_they_were_sent() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("/w".into(), Place::Split(0, Edge::Bottom)));
        t.intents.push_back(Intent("/v".into(), Place::Split(0, Edge::Right)));
        assert_eq!(t.spawned(None), Some(Intent("/w".into(), Place::Split(0, Edge::Bottom))));
        assert_eq!(t.spawned(None), Some(Intent("/v".into(), Place::Split(0, Edge::Right))));
    }

    #[test]
    fn a_terminal_spawned_with_nothing_pending_is_left_alone() {
        let mut t = Terminals::new(Default::default());
        assert_eq!(t.spawned(None), None);
        assert!(t.setups.is_empty());
    }

    #[test]
    fn a_failed_spawn_drops_its_intent_but_a_terminal_error_does_not() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
        t.intents.push_back(Intent("/v".into(), Place::Pane(None)));
        t.errored(None, "a");
        t.errored(None, "");
        assert_eq!(t.spawned(None), Some(Intent("/v".into(), Place::Pane(None))));
    }

    #[test]
    fn each_pocketd_answers_only_the_spawns_sent_to_it() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("//devbox/app".into(), Place::Pane(None)));
        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
        assert_eq!(t.spawned(None), Some(Intent("/w".into(), Place::Pane(None))));
        assert_eq!(t.spawned(Some("gpu")), None);
        assert_eq!(t.spawned(Some("devbox")), Some(Intent("//devbox/app".into(), Place::Pane(None))));
    }

    #[test]
    fn a_host_going_down_forgets_only_its_own_spawns_and_screens() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("//devbox/app".into(), Place::Pane(None)));
        t.intents.push_back(Intent("//gpu/app".into(), Place::Pane(None)));
        t.listed(vec![info("devbox/1"), info("2")]);
        t.host_down("devbox");
        assert_eq!(t.intents.iter().map(|i| i.0.as_str()).collect::<Vec<_>>(), ["//gpu/app"]);
        assert_eq!(t.listed(vec![info("devbox/1"), info("2")]), vec!["devbox/1"]);
    }

    #[test]
    fn this_macs_pocketd_going_away_keeps_the_hosts_spawns() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("//devbox/app".into(), Place::Pane(None)));
        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
        t.disconnected(std::time::Instant::now());
        assert_eq!(t.spawned(Some("devbox")), Some(Intent("//devbox/app".into(), Place::Pane(None))));
        assert_eq!(t.spawned(None), None);
    }

    #[test]
    fn closed_terminals_stay_closed_when_pocketd_still_lists_them() {
        let mut t = Terminals::new(Default::default());
        t.closed.insert("a".into());
        assert_eq!(t.listed(vec![info("a"), info("b")]), vec!["b"]);
        assert!(t.sessions.get("a").is_none());
    }

    #[test]
    fn a_setup_ends_when_its_terminal_leaves_the_list() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w".into(), true);
        t.created("r2", "b".into(), "/v".into(), true);
        t.listed(vec![info("a"), info("b")]);
        t.arrived();
        t.listed(vec![info("b")]);
        assert_eq!(t.setups, HashMap::from([("b".to_string(), "/v".to_string())]));
    }

    #[test]
    fn any_exit_closes_its_pane_but_a_failed_setup_stays_to_be_read() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "c".into(), "/w".into(), true);
        t.created("r2", "d".into(), "/w".into(), true);
        t.listed(vec![info("a"), info("b"), info("c"), info("d")]);
        assert!(t.exited(&exit("a", 0)));
        assert!(t.exited(&exit("b", 127)));
        assert!(t.exited(&exit("c", 0)));
        assert!(!t.exited(&exit("d", 1)));
        assert_eq!(t.sessions.get("d").and_then(|s| s.exit), Some(1));
    }

    #[test]
    fn a_killed_terminal_keeps_its_pane_open_on_exit() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        assert!(t.kill("a"));
        assert!(!t.exited(&exit("a", 143)));
        assert_eq!(t.sessions.get("a").and_then(|s| s.exit), Some(143));
    }

    #[test]
    fn an_exited_terminal_has_nothing_left_to_kill() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        t.exited(&exit("a", 0));
        assert!(!t.kill("a"));
    }

    #[test]
    fn the_exit_of_a_terminal_this_window_does_not_hold_closes_nothing() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        assert!(!t.exited(&exit("x", 0)));
        assert_eq!(t.sessions.get("a").map(|s| s.exit), Some(None));
    }

    #[test]
    fn a_setup_ends_when_its_terminal_exits() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        t.exited(&exit("a", 1));
        assert!(t.setups.is_empty());
    }

    #[test]
    fn closing_asks_pocketd_to_end_only_a_running_terminal() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a"), info("b")]);
        t.exited(&exit("b", 1));
        assert_eq!((t.close("a"), t.close("b"), t.close("x")), (true, false, false));
    }

    #[test]
    fn a_closed_terminal_leaves_the_window_with_its_setup() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        t.close("a");
        assert_eq!(t.listed(vec![info("a")]), Vec::<String>::new());
        assert!(t.sessions.get("a").is_none());
        assert!(t.setups.is_empty());
    }

    #[test]
    fn a_running_terminal_is_resized_once_per_size() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        assert_eq!((t.resize("a", 80, 24), t.resize("a", 80, 24), t.resize("a", 100, 24)), (true, false, true));
    }

    #[test]
    fn exited_and_unknown_terminals_are_not_resized() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        t.exited(&exit("a", 1));
        assert_eq!((t.resize("a", 80, 24), t.resize("x", 80, 24)), (false, false));
    }

    #[test]
    fn a_first_size_goes_at_once_and_later_ones_wait_to_settle() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        assert_eq!(t.fit("a", 80, 24), Fit::Now);
        assert_eq!(t.fit("a", 80, 24), Fit::Same);
        assert_eq!((t.fit("a", 90, 24), t.fit("a", 100, 24), t.fit("a", 100, 24)), (Fit::Later, Fit::Later, Fit::Same));
        assert_eq!(t.settled(), vec![("a".to_string(), 100, 24)]);
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn a_resize_that_returns_to_the_sent_size_sends_nothing() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        t.fit("a", 80, 24);
        t.fit("a", 90, 24);
        assert_eq!(t.fit("a", 80, 24), Fit::Later);
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn a_closed_terminal_drops_its_pending_size() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a")]);
        t.fit("a", 80, 24);
        t.fit("a", 90, 24);
        t.close("a");
        assert_eq!(t.settled(), vec![]);
    }

    #[test]
    fn reconnected_reattaches_every_listed_terminal() {
        let mut t = Terminals::new(Default::default());
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
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a"), info("b")]);
        t.reconnected();
        assert_eq!(t.listed(vec![info("b")]), vec!["b"]);
        assert!(t.sessions.get("a").is_none());
    }

    #[test]
    fn losing_pocketd_drops_spawns_it_never_answered() {
        let mut t = Terminals::new(Default::default());
        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
        t.disconnected(std::time::Instant::now());
        assert_eq!(t.spawned(None), None);
    }

    #[test]
    fn a_spawn_sent_just_before_up_is_still_adopted() {
        let mut t = Terminals::new(Default::default());
        t.disconnected(std::time::Instant::now());
        t.spawn_sent(true, Intent("/w".into(), Place::Pane(None)));
        t.reconnected();
        assert_eq!(t.spawned(None), Some(Intent("/w".into(), Place::Pane(None))));
    }

    #[test]
    fn a_close_that_never_arrived_is_sent_again_when_pocketd_lists_the_terminal() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a"), info("b")]);
        t.close("a");
        assert_eq!(t.unclosed(&[info("a"), info("b")]), vec!["a"]);
    }

    #[test]
    fn failed_spawn_send_records_no_intent() {
        let mut t = Terminals::new(Default::default());
        t.spawn_sent(false, Intent("/w".into(), Place::Pane(None)));
        t.spawn_sent(true, Intent("/v".into(), Place::Pane(None)));
        assert_eq!(t.spawned(None), Some(Intent("/v".into(), Place::Pane(None))));
    }

    #[test]
    fn a_creating_reply_adopts_its_terminal_and_marks_setup() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w/fix".into(), true);
        t.created("r2", "b".into(), "/w/v".into(), false);
        t.listed(vec![info("x")]);
        assert!(t.arrived().is_empty());
        assert_eq!(t.setups, HashMap::from([("a".to_string(), "/w/fix".to_string())]));
        t.listed(vec![info("x"), info("a")]);
        assert_eq!(t.arrived(), vec![("a".to_string(), Created { tree: "/w/fix".into(), pane: None })]);
        assert!(t.arrived().is_empty());
    }

    #[test]
    fn a_terminal_an_empty_pane_started_arrives_in_that_pane() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w".into(), false);
        t.aim("a", 2);
        t.listed(vec![info("a")]);
        assert_eq!(t.arrived(), vec![("a".to_string(), Created { tree: "/w".into(), pane: Some(2) })]);
    }

    #[test]
    fn a_replayed_creating_adopts_its_terminal_once() {
        let mut t = Terminals::new(Default::default());
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        assert_eq!(t.arrived(), vec![("a".to_string(), Created { tree: "/w".into(), pane: None })]);
        t.created("r1", "a".into(), "/w".into(), true);
        t.listed(vec![info("a")]);
        assert!(t.arrived().is_empty());
    }

    #[test]
    fn an_observer_can_drop_an_exited_terminal_but_not_end_a_running_one() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a"), info("b")]);
        t.exited(&exit("b", 1));
        assert_eq!((t.may_close("a", true), t.may_close("b", true), t.may_close("a", false)), (false, true, true));
    }

    #[test]
    fn input_reaches_a_running_or_just_spawned_terminal_but_not_an_exited_one() {
        let mut t = Terminals::new(Default::default());
        t.listed(vec![info("a"), info("b")]);
        t.exited(&exit("b", 1));
        assert_eq!((t.may_send("a", false), t.may_send("b", false), t.may_send("new", false), t.may_send("a", true)), (true, false, true, false));
    }
}
