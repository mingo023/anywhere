pub(crate) mod sessions;

use crate::desktop::Desktop;
use crate::terminals::sessions::Sessions;
use daemon::Msg;
use gpui_kit::*;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) enum Intent {
    Tab(String),
    Split(String, bool),
    Setup(String),
}

pub struct Terminals {
    pub(crate) sessions: Sessions,
    pub(crate) intents: VecDeque<Intent>,
    pub(crate) closed: HashSet<String>,
    pub(crate) sized: HashMap<String, (u16, u16)>,
    /// Terminal → worktree while the terminal runs the worktree's setup before its agent.
    pub(crate) setups: HashMap<String, String>,
}

impl Terminals {
    pub fn new() -> Self {
        Self { sessions: Sessions::default(), intents: VecDeque::new(), closed: HashSet::new(), sized: HashMap::new(), setups: HashMap::new() }
    }
}

impl Desktop {
    pub(crate) fn on_msg(&mut self, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match m.ev.as_str() {
            "terminals" => {
                let items = m.items.into_iter().filter(|i| !self.terminals.closed.contains(&i.id)).collect();
                for id in self.terminals.sessions.sync(items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                let sessions = &self.terminals.sessions;
                self.terminals.setups.retain(|id, _| sessions.get(id).is_some());
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
            }
            "spawned" => {
                self.error = None;
                match self.terminals.intents.pop_front() {
                    Some(Intent::Tab(tree)) => self.adopt(m.id.clone(), tree, None, window, cx),
                    Some(Intent::Split(tree, down)) => self.adopt(m.id.clone(), tree, Some(down), window, cx),
                    Some(Intent::Setup(tree)) => {
                        self.terminals.setups.insert(m.id.clone(), tree.clone());
                        self.adopt(m.id.clone(), tree, None, window, cx)
                    }
                    None => {}
                }
                self.daemon.send(json!({"op": "list"}));
            }
            "error" => {
                if m.id.is_empty() {
                    self.terminals.intents.pop_front();
                }
                self.error = Some(m.error);
            }
            "exit" => {
                self.terminals.sessions.apply(&m);
                self.terminals.setups.remove(&m.id);
                self.close_clean_exits(&m.id, cx);
            }
            _ => self.terminals.sessions.apply(&m),
        }
        cx.notify();
    }

    /// Closes panes whose shell exited cleanly, as Terminal.app does; a failed one stays so its error can be read.
    fn close_clean_exits(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.terminals.sessions.get(id).is_some_and(|s| s.exit == Some(0)) {
            self.close_pane(id, cx);
        }
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
        self.daemon.send(op);
        self.terminals.intents.push_back(intent);
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
        if self.terminals.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
        self.terminals.sessions.remove(id);
        self.terminals.closed.insert(id.to_string());
        self.terminals.sized.remove(id);
        self.terminals.setups.remove(id);
        for w in self.workspaces.values_mut() {
            w.remove(id);
        }
        self.load_active(cx);
        cx.notify();
    }

    /// Ends a session: a running agent goes with its terminal, an ended one only leaves the list.
    pub fn close_session(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(a) = self.agents.get(id) else { return };
        if a.status == "closed" {
            self.outbox.close(id);
        } else {
            let term = a.terminal_id.clone();
            self.close_pane(&term, cx);
        }
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        for id in self.workspace(&tree).close_tab(i) {
            self.close_pane(&id, cx);
        }
        self.load_active(cx);
        cx.notify();
    }

    pub fn fit(&mut self, id: &str, cols: u16, rows: u16) {
        // Sessions are shared with the user's own window; a capture must not reflow them.
        if self.capturing {
            return;
        }
        if self.terminals.sized.get(id) != Some(&(cols, rows)) && self.terminals.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "resize", "id": id, "cols": cols, "rows": rows}));
            self.terminals.sized.insert(id.to_string(), (cols, rows));
        }
    }
}
