//! Terminals and agents on hosts next to this Mac's: their ids carry the host's, and ops go back to the pocketd holding them.

use agents::{Decision, Event, Outbox, Summary};
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

fn tag_summary(host: &str, a: &mut Summary) {
    a.id = tag(host, &a.id);
    if !a.terminal_id.is_empty() {
        a.terminal_id = tag(host, &a.terminal_id);
    }
    if !a.cwd.is_empty() {
        a.cwd = crate::place(host, &a.cwd);
    }
}

/// An agent event from host `host`'s pocketd in the desktop's terms, as `tag_msg`; false for news only this Mac's pocketd gives: pairing, the phone, Locals, automations.
pub fn tag_event(host: &str, ev: &mut Event) -> bool {
    let t = |id: &mut String| *id = tag(host, id);
    match ev {
        Event::Agents(list) => list.iter_mut().for_each(|a| tag_summary(host, a)),
        Event::Agent(a) => tag_summary(host, a),
        Event::Items(id, _) | Event::Resolved(id) | Event::NamingFailed(id) | Event::Progress { request: id, .. } | Event::CreateFailed { request: id, .. } => t(id),
        Event::Asked(p) => {
            t(&mut p.request_id);
            t(&mut p.agent_id);
        }
        Event::Creating { request, terminal, cwd, .. } => {
            t(request);
            t(terminal);
            *cwd = crate::place(host, cwd);
        }
        Event::Created { request, agent } => {
            t(request);
            t(agent);
        }
        Event::PromptFailed { agent, .. } | Event::InterruptFailed { agent, .. } | Event::Commands { agent, .. } => t(agent),
        Event::Names(names) => *names = std::mem::take(names).into_iter().map(|(path, name)| (crate::place(host, &path), name)).collect(),
        Event::Connected { .. } | Event::SettingsFailed(_) => {}
        Event::Host(_)
        | Event::PairCode { .. }
        | Event::Paired(_)
        | Event::PairFailed(_)
        | Event::Providers { .. }
        | Event::ConfigFailed(_)
        | Event::Locals(_)
        | Event::Automations { .. }
        | Event::AutomationError(_) => return false,
    }
    true
}

/// This Mac's pocketd's agent channel and each linked host's, sending every message to the pocketd its ids name, in that one's terms.
#[derive(Clone)]
pub struct Outboxes {
    /// For what only this Mac's pocketd does: pairing, the phone, Locals, automations.
    pub local: Outbox,
    hosts: Arc<Mutex<HashMap<String, Outbox>>>,
}

impl Outboxes {
    pub fn new(local: Outbox) -> Self {
        Self { local, hosts: Arc::default() }
    }

    pub fn add(&self, host: &str, outbox: Outbox) {
        self.hosts.lock().unwrap().insert(host.into(), outbox);
    }

    pub fn remove(&self, host: &str) {
        self.hosts.lock().unwrap().remove(host);
    }

    pub fn has(&self, host: &str) -> bool {
        self.hosts.lock().unwrap().contains_key(host)
    }

    fn to(&self, host: Option<&str>) -> Option<Outbox> {
        match host {
            None => Some(self.local.clone()),
            Some(h) => self.hosts.lock().unwrap().get(h).cloned(),
        }
    }

    /// Each pocketd hears of the agents in `ids` it holds; every one of them hears it when `all`, even of none.
    fn split_ids(&self, ids: &[String], all: bool, send: impl Fn(&Outbox, &[String])) {
        let hosts = self.hosts.lock().unwrap().clone();
        let mut by: HashMap<Option<&str>, Vec<String>> = HashMap::new();
        if all {
            by.insert(None, Vec::new());
            by.extend(hosts.keys().map(|h| (Some(h.as_str()), Vec::new())));
        }
        for id in ids {
            let (host, theirs) = untag(id);
            by.entry(host).or_default().push(theirs.to_string());
        }
        for (host, ids) in by {
            if let Some(out) = host.map_or(Some(&self.local), |h| hosts.get(h)) {
                send(out, &ids);
            }
        }
    }

    pub fn view(&self, ids: &[String]) {
        self.split_ids(ids, true, Outbox::view);
    }

    pub fn seen(&self, ids: &[String]) {
        self.split_ids(ids, false, Outbox::seen);
    }

    fn by_id(&self, id: &str, send: impl FnOnce(&Outbox, &str)) {
        let (host, theirs) = untag(id);
        if let Some(out) = self.to(host) {
            send(&out, theirs);
        }
    }

    pub fn pin(&self, agent_id: &str, pinned: bool) {
        self.by_id(agent_id, |o, id| o.pin(id, pinned));
    }

    /// The request id a refusal names, once sent.
    pub fn prompt(&self, agent_id: &str, text: &str) -> Option<String> {
        let (host, theirs) = untag(agent_id);
        self.to(host).map(|out| out.prompt(theirs, text))
    }

    pub fn interrupt(&self, agent_id: &str) {
        self.by_id(agent_id, |o, id| o.interrupt(id));
    }

    pub fn compact(&self, agent_id: &str) {
        self.by_id(agent_id, |o, id| o.compact(id));
    }

    pub fn settings(&self, agent_id: &str, model: &str, effort: &str) {
        self.by_id(agent_id, |o, id| o.settings(id, model, effort));
    }

    pub fn access(&self, agent_id: &str, access: &str) {
        self.by_id(agent_id, |o, id| o.access(id, access));
    }

    pub fn commands(&self, agent_id: &str) {
        self.by_id(agent_id, |o, id| o.commands(id));
    }

    pub fn resolve(&self, request_id: &str, decision: Decision) {
        self.by_id(request_id, |o, id| o.resolve(id, decision));
    }

    pub fn answer(&self, request_id: &str, answers: &HashMap<String, String>) {
        self.by_id(request_id, |o, id| o.answer(id, answers));
    }

    pub fn reply(&self, request_id: &str, message: &str) {
        self.by_id(request_id, |o, id| o.reply(id, message));
    }

    pub fn rename(&self, path: &str, title: &str) {
        let (host, theirs) = crate::split(path);
        if let Some(out) = self.to(host) {
            out.rename(theirs, title);
        }
    }

    /**
     * Sends a create to the pocketd holding `spec`'s project, in its paths, and returns the requestId, tagged for a host.
     * A host's pocketd only makes worktrees for projects its `desktop.json` lists, so `registry(host)` is written there first.
     * `Err` names the host when it isn't connected, so nothing was sent.
     */
    pub fn create(&self, mut spec: Value, registry: impl FnOnce(&str) -> Value) -> Result<String, String> {
        let project = spec["project"].as_str().unwrap_or_default().to_string();
        let (Some(host), path) = crate::split(&project) else { return Ok(self.local.create(spec)) };
        spec["project"] = path.into();
        if let Some(tree) = spec["checkout"]["worktree"].as_str().map(|t| crate::split(t).1.to_string()) {
            spec["checkout"]["worktree"] = tree.into();
        }
        let request = agents::request_id();
        let tagged = tag(host, &request);
        let Some(out) = self.to(Some(host)) else { return Err(host.into()) };
        let (host, registry) = (host.to_string(), registry(host));
        std::thread::spawn(move || {
            if let Some(client) = crate::remote::connected(&host) {
                _ = client.sync_registry(&registry);
            }
            out.create_as(&request, spec);
        });
        Ok(tagged)
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

    #[test]
    fn a_hosts_agent_events_are_tagged_and_its_phone_and_pairing_news_dropped() {
        let mut list = Event::Agents(vec![Summary { id: "a1".into(), terminal_id: "7f3a".into(), cwd: "/home/me/app".into(), ..Summary::default() }]);
        assert!(tag_event("devbox", &mut list));
        let Event::Agents(list) = list else { unreachable!() };
        assert_eq!((list[0].id.as_str(), list[0].terminal_id.as_str(), list[0].cwd.as_str()), ("devbox/a1", "devbox/7f3a", "//devbox/home/me/app"));

        let mut creating = Event::Creating { request: "r1".into(), terminal: "7f3a".into(), cwd: "/home/me/.worktrees/app/fox".into(), setup: true };
        assert!(tag_event("devbox", &mut creating));
        assert!(matches!(creating, Event::Creating { request, terminal, cwd, .. }
            if request == "devbox/r1" && terminal == "devbox/7f3a" && cwd == "//devbox/home/me/.worktrees/app/fox"));

        let mut asked = Event::Asked(agents::Permission { request_id: "q1".into(), agent_id: "a1".into(), ..Default::default() });
        assert!(tag_event("devbox", &mut asked));
        assert!(matches!(asked, Event::Asked(p) if p.request_id == "devbox/q1" && p.agent_id == "devbox/a1"));

        let mut names = Event::Names(HashMap::from([("/home/me/app".into(), "App".into())]));
        assert!(tag_event("devbox", &mut names));
        assert!(matches!(names, Event::Names(n) if n.contains_key("//devbox/home/me/app")));

        let mut refused = Event::PromptFailed { agent: "a1".into(), request: "prompt-1".into() };
        assert!(tag_event("devbox", &mut refused));
        assert!(matches!(refused, Event::PromptFailed { agent, .. } if agent == "devbox/a1"));

        let mut unstopped = Event::InterruptFailed { agent: "a1".into(), reason: "This session is still resuming".into() };
        assert!(tag_event("devbox", &mut unstopped));
        assert!(matches!(unstopped, Event::InterruptFailed { agent, .. } if agent == "devbox/a1"));

        assert!(!tag_event("devbox", &mut Event::Paired("iPhone".into())));
        assert!(!tag_event("devbox", &mut Event::Locals(Vec::new())));
    }

    fn outboxes() -> (Outboxes, std::sync::mpsc::Receiver<Value>, std::sync::mpsc::Receiver<Value>) {
        let (local, here) = std::sync::mpsc::channel();
        let (host, there) = std::sync::mpsc::channel();
        let out = Outboxes::new(local.into());
        out.add("devbox", host.into());
        (out, here, there)
    }

    #[test]
    fn ops_on_a_hosts_agents_reach_that_host_with_its_own_ids() {
        let (out, here, there) = outboxes();
        out.view(&["a1".into(), "devbox/b1".into()]);
        out.resolve("devbox/q1", Decision::Allow);
        out.pin("a1", true);
        out.rename("//devbox/home/me/app", "Box");
        let sent: Vec<Value> = there.try_iter().collect();
        assert_eq!(sent[0]["agentIds"], json!(["b1"]));
        assert_eq!(sent[1]["requestId"], "q1");
        assert_eq!(sent[2]["path"], "/home/me/app");
        let sent: Vec<Value> = here.try_iter().collect();
        assert_eq!((sent[0]["agentIds"].clone(), sent[1]["agentId"].clone()), (json!(["a1"]), json!("a1")));

        out.view(&[]);
        assert_eq!(there.try_iter().next().unwrap()["agentIds"], json!([]), "a host stops streaming once none of its agents is viewed");
    }

    #[test]
    fn a_create_for_a_hosts_project_goes_there_in_its_paths_under_a_tagged_request() {
        let (out, here, there) = outboxes();
        let spec = json!({"project": "//devbox/home/me/app", "checkout": {"worktree": "//devbox/home/me/.worktrees/app/fox"}});
        let request = out.create(spec, |host| json!({"projects": [host]})).unwrap();
        let sent = there.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(request, tag("devbox", sent["requestId"].as_str().unwrap()));
        assert_eq!((sent["spec"]["project"].as_str(), sent["spec"]["checkout"]["worktree"].as_str()), (Some("/home/me/app"), Some("/home/me/.worktrees/app/fox")));
        assert!(here.try_iter().next().is_none());

        let mine = out.create(json!({"project": "/src/app"}), |_| unreachable!("this Mac's pocketd reads the desktop's own desktop.json")).unwrap();
        assert_eq!(here.try_iter().next().unwrap()["requestId"], mine.as_str());
    }

    #[test]
    fn a_create_for_a_host_that_isnt_connected_sends_nothing_and_names_it() {
        let (out, here, there) = outboxes();
        assert_eq!(out.create(json!({"project": "//laptop/home/me/app"}), |_| json!({})), Err("laptop".into()));
        assert!(here.try_iter().next().is_none() && there.try_iter().next().is_none());
    }

    #[test]
    fn a_host_agents_prompt_goes_to_that_host_untagged() {
        let (out, here, there) = outboxes();
        out.prompt("devbox/a1", "hi");
        out.interrupt("devbox/a1");
        out.compact("a2");
        let sent: Vec<Value> = there.try_iter().collect();
        assert_eq!((&sent[0]["type"], &sent[0]["agentId"], &sent[0]["text"]), (&json!("agent.prompt"), &json!("a1"), &json!("hi")));
        assert_eq!((&sent[1]["type"], &sent[1]["agentId"]), (&json!("agent.interrupt"), &json!("a1")));
        assert_eq!(here.try_iter().next().unwrap()["agentId"], "a2");
    }
}
