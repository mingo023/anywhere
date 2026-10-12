//! Terminals on hosts next to this Mac's: their ids carry the host's, and ops go back to the pocketd holding them.

use daemon::{Daemon, Info, Msg};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The desktop's id for terminal `theirs` on host `host`.
pub fn tag(host: &str, theirs: &str) -> String {
    format!("{host}/{theirs}")
}

/// The host holding a desktop terminal id and that host's own id for it; `None` for this Mac's, whose ids hold no `/`.
pub fn untag(id: &str) -> (Option<&str>, &str) {
    match id.split_once('/') {
        Some((host, theirs)) => (Some(host), theirs),
        None => (None, id),
    }
}

fn tag_info(host: &str, info: &mut Info) {
    info.id = tag(host, &info.id);
    info.cwd = crate::place(host, &info.cwd);
    // Locals live on this Mac only.
    info.local.clear();
}

/// A message from host `host`'s pocketd, in the desktop's terms: its terminal ids tagged and its folders as keys.
pub fn tag_msg(host: &str, m: &mut Msg) {
    if !m.id.is_empty() {
        m.id = tag(host, &m.id);
    }
    for info in &mut m.items {
        tag_info(host, info);
    }
}

/// Opens the host's login shell in `cwd`, a path on the host, running `argv` in it when there is one.
pub fn spawn_op(cwd: &str, argv: &[String]) -> Value {
    json!({"op": "spawn", "login": true, "cwd": cwd, "argv": argv, "cols": 120, "rows": 36})
}

/// This Mac's pocketd and each linked host's, sending every op to the one that holds its terminal.
#[derive(Clone)]
pub struct Daemons {
    local: Daemon,
    hosts: Arc<Mutex<HashMap<String, Daemon>>>,
}

impl Daemons {
    pub fn new(local: Daemon) -> Self {
        Self { local, hosts: Arc::default() }
    }

    pub fn add(&self, host: &str, daemon: Daemon) {
        self.hosts.lock().unwrap().insert(host.into(), daemon);
    }

    pub fn remove(&self, host: &str) {
        self.hosts.lock().unwrap().remove(host);
    }

    pub fn has(&self, host: &str) -> bool {
        self.hosts.lock().unwrap().contains_key(host)
    }

    /// Sends `msg` to whichever pocketd holds the terminal it names, with that pocketd's id; a `list` goes to every one.
    pub fn send(&self, mut msg: Value) -> bool {
        if msg["op"] == "list" {
            let hosts: Vec<Daemon> = self.hosts.lock().unwrap().values().cloned().collect();
            hosts.iter().for_each(|d| _ = d.send(msg.clone()));
            return self.local.send(msg);
        }
        let id = msg["id"].as_str().unwrap_or_default().to_string();
        match untag(&id) {
            (Some(host), theirs) => {
                msg["id"] = theirs.into();
                self.send_to(Some(host), msg)
            }
            (None, _) => self.local.send(msg),
        }
    }

    /// Sends `msg` as is to host `host`'s pocketd, or this Mac's for `None`; false while that one is unreachable.
    pub fn send_to(&self, host: Option<&str>, msg: Value) -> bool {
        match host {
            None => self.local.send(msg),
            Some(h) => self.hosts.lock().unwrap().get(h).is_some_and(|d| d.send(msg)),
        }
    }

    pub fn input(&self, id: &str, bytes: &[u8]) -> bool {
        match untag(id) {
            (Some(host), theirs) => self.hosts.lock().unwrap().get(host).is_some_and(|d| d.input(theirs, bytes)),
            (None, _) => self.local.input(id, bytes),
        }
    }
}

/// The last terminal list each pocketd sent, so one pocketd's list doesn't drop another's terminals.
#[derive(Default)]
pub struct Lists {
    local: Vec<Info>,
    hosts: HashMap<String, Vec<Info>>,
}

impl Lists {
    /// Records `items` from `host` (this Mac's for `None`), already tagged; every pocketd's terminals together.
    pub fn listed(&mut self, host: Option<&str>, items: Vec<Info>) -> Vec<Info> {
        match host {
            None => self.local = items,
            Some(h) => _ = self.hosts.insert(h.into(), items),
        }
        self.all()
    }

    /// Forgets a host taken off this Mac; its terminals leave the list.
    pub fn forget(&mut self, host: &str) -> Vec<Info> {
        self.hosts.remove(host);
        self.all()
    }

    fn all(&self) -> Vec<Info> {
        self.local.iter().chain(self.hosts.values().flatten()).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, cwd: &str) -> Info {
        Info { id: id.into(), cwd: cwd.into(), ..Info::default() }
    }

    #[test]
    fn a_hosts_terminal_is_tagged_with_the_host_and_its_folder_keyed_under_it() {
        let mut m = Msg { ev: "terminals".into(), items: vec![Info { local: "local-1".into(), ..info("7f3a", "/home/me/app") }], ..Msg::default() };
        tag_msg("devbox", &mut m);
        assert_eq!(m.items, vec![info("devbox/7f3a", "//devbox/home/me/app")]);
        let mut out = Msg { ev: "output".into(), id: "7f3a".into(), ..Msg::default() };
        tag_msg("devbox", &mut out);
        assert_eq!(out.id, "devbox/7f3a");
    }

    #[test]
    fn untagging_gives_back_the_hosts_own_id_and_leaves_this_macs_alone() {
        assert_eq!(untag(&tag("devbox", "7f3a")), (Some("devbox"), "7f3a"));
        assert_eq!(untag("0b9c-11"), (None, "0b9c-11"));
    }

    #[test]
    fn each_pocketds_list_keeps_the_others_terminals() {
        let mut lists = Lists::default();
        lists.listed(None, vec![info("a", "/x")]);
        let all = lists.listed(Some("devbox"), vec![info("devbox/b", "//devbox/y")]);
        assert_eq!(all.len(), 2);
        let all = lists.listed(None, vec![]);
        assert_eq!(all, vec![info("devbox/b", "//devbox/y")]);
        assert!(lists.forget("devbox").is_empty());
    }

    #[test]
    fn an_op_for_an_unlinked_host_is_not_sent_here() {
        let (local, _rx) = Daemon::spawn(std::path::Path::new("/nonexistent/pocketd.sock"));
        let daemons = Daemons::new(local);
        assert!(!daemons.send(serde_json::json!({"op": "input", "id": "devbox/7f3a"})));
        assert!(!daemons.input("devbox/7f3a", b"ls"));
    }
}
