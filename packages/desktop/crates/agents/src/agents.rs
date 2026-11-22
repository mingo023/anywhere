#[cfg(test)]
#[macro_use]
mod golden;

pub mod automations;
pub mod catalogs;
pub mod delegations;
pub mod hosts;
pub mod locals;
pub mod providers;
pub mod queue;
pub mod registry;
pub mod tickets;

mod ask;
mod caps;
mod event;
mod ids;
mod items;
mod link;
mod offered;
mod outbox;
mod summary;

pub use ask::{Choice, Permission, Question};
pub use event::{Event, Host, OUTDATED, coalesce};
pub use ids::request_id;
pub use items::{Call, Detail, DiffLine, FileDiff, Item, TaskItem, Usage};
pub use link::{connect, connect_with};
pub use offered::{Command, McpAction, McpServer, McpStatus};
pub use outbox::{CreateReply, Decision, Outbox};
pub use summary::{Goal, GoalStatus, Level, ListedModel, ModelSetting, Summary, level, model_label, model_name, percent};

use automations::Automations;
use caps::{
    ACCESS_CAP, AUTOMATIONS_CAP, BOARD_CAP, FORK_CAP, HOSTS_CAP, LOCALS_CAP, NAMES_CAP, OPEN_CAP, ORCHESTRATOR_CAP, PAIR_CAP, REGISTRY_CAP, SETTINGS_CAP,
};
use delegations::Delegations;
#[cfg(test)]
use event::Frame;
use locals::Local;
use providers::ProviderInfo;
use queue::Queues;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tickets::Board;

#[derive(Default)]
pub struct Agents {
    pub list: Vec<Summary>,
    pub timelines: HashMap<String, Vec<Item>>,
    /// Bumped each time an agent's timeline takes items, so views refold only when it moved.
    pub revisions: HashMap<String, u64>,
    pub pending: Vec<Permission>,
    /// `None` until pocketd reports it, and again after each reconnect.
    pub host: Option<Host>,
    pub scopes: Vec<String>,
    /// What each provider takes, from `agent.providers`.
    pub providers: Vec<ProviderInfo>,
    pub caps: Vec<String>,
    /// Worktree display names by path, from `worktree.names`.
    pub names: HashMap<String, String>,
    /// The Locals the user added, in the order they were made, from `local.list`.
    pub locals: Vec<Local>,
    /// pocketd's build from `hello.ok`: the release tag, or a VCS stamp in dev.
    pub version: String,
    pub automations: Arc<Automations>,
    pub board: Arc<Board>,
    pub delegations: Delegations,
    pub queues: Queues,
    /// The pocketds that hold sessions' queues: this Mac's as "", a host's by its id.
    pub holders: HashSet<String>,
    /// Whether each pocketd that said hello, this Mac's as "" and a host's by its id, takes registry changes.
    pub registries: HashMap<String, bool>,
}

impl Agents {
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn revision(&self, id: &str) -> u64 {
        self.revisions.get(id).copied().unwrap_or_default()
    }

    pub fn in_terminal(&self, terminal: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.terminal_id == terminal)
    }

    /// The sessions `roots` delegated to, and theirs in turn, all the way down; forks are the user's own and stay out.
    pub fn subagents_of(&self, roots: &[String]) -> Vec<&Summary> {
        let mut found: Vec<&Summary> = Vec::new();
        let mut parents: Vec<&str> = roots.iter().map(String::as_str).collect();
        while let Some(parent) = parents.pop() {
            for a in self.list.iter().filter(|a| a.parent == parent && a.relationship == "subagent") {
                if !roots.contains(&a.id) && !found.iter().any(|f| f.id == a.id) {
                    found.push(a);
                    parents.push(&a.id);
                }
            }
        }
        found
    }

    /// pocketd's word on provider `id`, else what this build knows.
    pub fn provider_info(&self, id: &str) -> &ProviderInfo {
        self.providers.iter().find(|p| p.id == id).unwrap_or_else(|| providers::builtin_info(id))
    }

    pub fn apply(&mut self, ev: Event) {
        self.apply_from(None, ev);
    }

    /// Drops all that host `source` sent, once it's taken off this Mac.
    pub fn forget(&mut self, source: &str) {
        self.apply_from(Some(source), Event::Agents(Vec::new()));
        self.apply_from(Some(source), Event::Connected { scopes: Vec::new(), caps: Vec::new(), version: String::new() });
        self.timelines.retain(|id, _| id.split_once('/').map(|(h, _)| h) != Some(source));
        self.delegations.forget(source);
        self.queues.forget(source);
        self.registries.remove(source);
    }

    /// Applies an event, already tagged, from this Mac's pocketd (`None`) or host `source`'s:
    /// what one pocketd sends whole replaces only its own share.
    pub fn apply_from(&mut self, source: Option<&str>, ev: Event) {
        let mine = |id: &str| id.split_once('/').map(|(h, _)| h) == source;
        let my_name = |path: &str| path.strip_prefix("//").and_then(|p| p.split_once('/')).map(|(h, _)| h) == source;
        let lineage = matches!(ev, Event::Agents(_) | Event::Agent(_) | Event::Delegations { .. } | Event::Delegated { .. });
        match ev {
            Event::Agents(list) => {
                list.iter().for_each(|a| self.drop_if_reset(a));
                self.list.retain(|a| !mine(&a.id));
                self.list.extend(list);
            }
            Event::Connected { caps, .. } if source.is_some() => {
                self.pending.retain(|p| !mine(&p.request_id));
                self.names.retain(|path, _| !my_name(path));
                self.holds_queues(source, &caps);
                self.says_registry(source, &caps);
            }
            Event::Agent(a) if a.status == "closed" => self.list.retain(|x| x.id != a.id),
            Event::Agent(a) => {
                self.drop_if_reset(&a);
                match self.list.iter_mut().find(|x| x.id == a.id) {
                    Some(x) => *x = *a,
                    None => self.list.push(*a),
                }
            }
            Event::Items(id, items) => self.upsert(id, items),
            Event::Page { agent, items, max_seq } => {
                // pocketd drops a link that fell behind, so what was held from before the page may have missed its updates.
                let from = items.iter().map(|x| x.seq).min().unwrap_or(max_seq + 1);
                if let Some(t) = self.timelines.get_mut(&agent) {
                    t.retain(|x| x.seq >= from);
                }
                self.upsert(agent, items);
            }
            Event::Asked(p) => {
                if !self.pending.iter().any(|x| x.request_id == p.request_id) {
                    self.pending.push(p);
                }
            }
            Event::Resolved(id) => self.pending.retain(|p| p.request_id != id),
            Event::Connected { scopes, caps, version } => {
                self.pending.retain(|p| !mine(&p.request_id));
                self.host = None;
                self.scopes = scopes;
                self.holds_queues(source, &caps);
                self.says_registry(source, &caps);
                self.caps = caps;
                self.version = version;
                self.names.retain(|path, _| !my_name(path));
                self.locals.clear();
            }
            Event::Host(h) => self.host = Some(h),
            Event::PairCode { .. } | Event::Paired(_) | Event::PairFailed(_) => {}
            Event::Providers { providers } => self.providers = providers,
            Event::Creating { .. }
            | Event::Progress { .. }
            | Event::Created { .. }
            | Event::CreateFailed { .. }
            | Event::SettingsFailed(_)
            | Event::NamingFailed(_) => {}
            Event::Names(names) => {
                self.names.retain(|path, _| !my_name(path));
                self.names.extend(names);
            }
            Event::Locals(locals) => self.locals = locals,
            Event::Registry(_) | Event::RegistryFailed(_) => {}
            Event::Automations { automations, runs } => {
                let mut snapshot = Automations::default();
                snapshot.apply(automations, runs);
                self.automations = Arc::new(snapshot);
            }
            Event::Board { tickets, limit } => self.board = Arc::new(Board { tickets, limit }),
            Event::Delegations { agent, items } => self.delegations.replace(&agent, items),
            Event::Delegated { agent, delegation } => self.delegations.update(&agent, delegation),
            Event::Queue { agent, entries } => self.queues.replace(&agent, entries),
            Event::AutomationError(_)
            | Event::BoardError(_)
            | Event::PromptFailed { .. }
            | Event::InterruptFailed { .. }
            | Event::QueueFailed { .. }
            | Event::Commands { .. }
            | Event::Skills { .. }
            | Event::Btw { .. }
            | Event::RemoteControl { .. }
            | Event::Mcp { .. }
            | Event::McpActFailed { .. } => {}
            Event::Hosts(_) | Event::HostUpdate { .. } | Event::HostProgress { .. } | Event::HostError { .. } | Event::HostDone(_) => {}
        }
        if lineage {
            self.adopt_children();
        }
    }

    /// A host launches a delegated session with no lineage, so only the delegation, by the child's tagged id, says whose child it is.
    fn adopt_children(&mut self) {
        for d in self.delegations.all().filter(|d| d.child_id.contains('/')) {
            if let Some(child) = self.list.iter_mut().find(|a| a.id == d.child_id && a.parent.is_empty()) {
                child.parent = d.parent_id.clone();
                child.relationship = "subagent".into();
            }
        }
    }

    fn upsert(&mut self, id: String, items: Vec<Item>) {
        *self.revisions.entry(id.clone()).or_default() += 1;
        let t = self.timelines.entry(id).or_default();
        let mut order = Vec::new();
        let mut fresh: HashMap<String, Item> = HashMap::new();
        for x in items {
            let id = x.id.clone();
            if fresh.insert(id.clone(), x).is_none() {
                order.push(id);
            }
        }
        // A streaming draft rewrites the newest item, so look from the end; an older page of a long chat adds
        // thousands at once, which one pass takes.
        for x in t.iter_mut().rev() {
            if fresh.is_empty() {
                break;
            }
            if let Some(item) = fresh.remove(&x.id) {
                *x = item;
            }
        }
        t.extend(order.iter().filter_map(|id| fresh.remove(id)));
        t.sort_by_key(|x| x.seq);
    }

    /// Drops the items held for `a` once pocketd started its timeline over, so the old conversation doesn't stay above the new one.
    fn drop_if_reset(&mut self, a: &Summary) {
        if self.get(&a.id).is_some_and(|x| x.resets != a.resets) && self.timelines.remove(&a.id).is_some() {
            *self.revisions.entry(a.id.clone()).or_default() += 1;
        }
    }

    pub fn owner(&self) -> bool {
        self.scopes.iter().any(|s| s == "owner")
    }

    pub fn pairing(&self) -> bool {
        self.caps.iter().any(|c| c == PAIR_CAP)
    }

    /// pocketd can open an existing branch as a worktree.
    pub fn opens(&self) -> bool {
        self.caps.iter().any(|c| c == OPEN_CAP)
    }

    /// pocketd names worktrees from their prompt and takes this client's renames.
    pub fn names_offered(&self) -> bool {
        self.caps.iter().any(|c| c == NAMES_CAP) && !self.observe_only()
    }

    /// pocketd keeps Locals and takes this client's changes to them.
    pub fn locals_offered(&self) -> bool {
        self.caps.iter().any(|c| c == LOCALS_CAP) && !self.observe_only()
    }

    /// Whether pocketd can rename the tree `key`, a Local's id or a worktree's path.
    pub fn renames_offered(&self, key: &str) -> bool {
        if locals::is_local(key) { self.locals_offered() } else { self.names_offered() }
    }

    pub fn local(&self, id: &str) -> Option<&Local> {
        self.locals.iter().find(|l| l.id == id)
    }

    /// The tree `key`'s name, if it has one: a Local's own, else a worktree's from renaming or naming.
    pub fn given_name(&self, key: &str) -> Option<&str> {
        self.local(key).map(|l| l.name.as_str()).or_else(|| self.names.get(key).map(String::as_str).filter(|n| !n.is_empty()))
    }

    pub fn locals_in<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a Local> {
        self.locals.iter().filter(move |l| l.project == project)
    }

    /// The ids of the Locals removing `project` deletes: none when pocketd won't take this client's deletes.
    pub fn locals_removed_with<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a str> {
        self.locals_offered().then(|| self.locals_in(project)).into_iter().flatten().map(|l| l.id.as_str())
    }

    /// Shows a new Local before pocketd lists it.
    pub fn add_local(&mut self, local: Local) {
        self.locals.push(local);
    }

    /// Shows a Local's new name before pocketd lists it; a blank one changes nothing.
    pub fn rename_local(&mut self, id: &str, title: &str) {
        if let Some(l) = self.locals.iter_mut().find(|l| l.id == id).filter(|_| !title.trim().is_empty()) {
            l.name = title.trim().to_string();
        }
    }

    pub fn remove_local(&mut self, id: &str) {
        self.locals.retain(|l| l.id != id);
    }

    /// pocketd keeps the Board and takes its messages.
    pub fn board_offered(&self) -> bool {
        self.caps.iter().any(|c| c == BOARD_CAP)
    }

    /// Notes whether the pocketd that connected as `source` (this Mac's for `None`) keeps its sessions' queues; its queues are asked for again.
    fn holds_queues(&mut self, source: Option<&str>, caps: &[String]) {
        let key = source.unwrap_or_default();
        if caps.iter().any(|c| c == ORCHESTRATOR_CAP) {
            self.holders.insert(key.to_string());
        } else {
            self.holders.remove(key);
        }
        self.queues.reconnected(source);
    }

    fn says_registry(&mut self, source: Option<&str>, caps: &[String]) {
        self.registries.insert(source.unwrap_or_default().to_string(), caps.iter().any(|c| c == REGISTRY_CAP));
    }

    /// Whether `host`'s pocketd (this Mac's for `None`) said hello without taking registry changes: it needs updating.
    pub fn registry_outdated(&self, host: Option<&str>) -> bool {
        self.registries.get(host.unwrap_or_default()) == Some(&false)
    }

    /// The pocketd holding `agent_id`'s session keeps its queue: a send while the agent works goes there, not held here.
    pub fn holds_queue(&self, agent_id: &str) -> bool {
        let host = agent_id.split_once('/').map_or("", |(h, _)| h);
        self.holders.contains(host) && !self.observe_only()
    }

    /// pocketd links the SSH hosts and takes their messages.
    pub fn hosts_offered(&self) -> bool {
        self.caps.iter().any(|c| c == HOSTS_CAP)
    }

    /// pocketd runs automations and takes their messages.
    pub fn automations_offered(&self) -> bool {
        self.caps.iter().any(|c| c == AUTOMATIONS_CAP)
    }

    /// pocketd starts a session as a copy of a provider session.
    pub fn fork_offered(&self) -> bool {
        self.caps.iter().any(|c| c == FORK_CAP)
    }

    /// pocketd switches a running agent's model or effort.
    pub fn settings_offered(&self) -> bool {
        self.caps.iter().any(|c| c == SETTINGS_CAP) && !self.observe_only()
    }

    /// pocketd switches a chat agent's access.
    pub fn access_offered(&self) -> bool {
        self.caps.iter().any(|c| c == ACCESS_CAP) && !self.observe_only()
    }

    /// Shows a rename before pocketd echoes it; a blank title clears the name.
    pub fn set_name(&mut self, path: &str, title: &str) {
        match title.trim() {
            "" => self.names.remove(path),
            t => self.names.insert(path.to_string(), t.to_string()),
        };
    }

    /// Scopes arrived and exclude owner. Before hello.ok, or from a pocketd
    /// that predates scopes, nothing is known, so nothing is disabled.
    pub fn observe_only(&self) -> bool {
        !self.scopes.is_empty() && !self.owner()
    }

    pub fn last_result(&self, id: &str) -> Option<&Item> {
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "result" && i.parent_tool_use_id.is_none())
    }

    /// The agent whose timeline last edited or wrote `path`, and when; tool paths may be absolute or relative to its cwd.
    pub fn last_edit(&self, path: &str) -> Option<(&Summary, i64)> {
        let touches = |d: &Detail, cwd: &str| {
            (d.kind == "edit" || d.kind == "write")
                && !d.path.is_empty()
                && (d.path == path || path.strip_prefix(cwd).and_then(|p| p.strip_prefix('/')).is_some_and(|rel| rel == d.path))
        };
        self.list
            .iter()
            .filter_map(|a| {
                let t = self.timelines.get(&a.id)?;
                let ts = t.iter().rev().find(|i| i.call.as_ref().is_some_and(|c| touches(&c.detail, &a.cwd)))?.ts;
                Some((a, ts))
            })
            .max_by_key(|(_, ts)| *ts)
    }

    pub fn last_text(&self, id: &str) -> Option<&str> {
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "assistant" && i.parent_tool_use_id.is_none()).map(|i| i.text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delegations::Delegation;
    use crate::event::{delegation_event, names_event};

    #[test]
    fn only_a_pocketd_with_the_orchestrator_holds_its_sessions_queues() {
        let connected =
            |caps: &[&str]| Event::Connected { scopes: vec!["owner".into()], caps: caps.iter().map(|c| c.to_string()).collect(), version: String::new() };
        let mut a = Agents::default();
        assert!(!a.holds_queue("a1"));
        a.apply_from(Some("new"), connected(&[ORCHESTRATOR_CAP]));
        a.apply_from(Some("old"), connected(&["pair.v1"]));
        assert_eq!([a.holds_queue("a1"), a.holds_queue("new/a1"), a.holds_queue("old/a1")], [false, true, false]);
        a.apply(connected(&[ORCHESTRATOR_CAP]));
        assert!(a.holds_queue("a1"));
        a.forget("new");
        assert_eq!([a.holds_queue("a1"), a.holds_queue("new/a1")], [true, false]);
    }

    #[test]
    fn delegation_events_fill_the_model_and_a_forgotten_host_takes_its_own() {
        let mut a = Agents::default();
        let Some(Event::Delegations { agent, items }) = delegation_event("agent.delegations", golden_text!("server/agent_delegations.json")) else { panic!() };
        a.apply(Event::Delegations { agent, items });
        let Some(Event::Delegated { agent, delegation }) = delegation_event("delegation.update", golden_text!("server/delegation_update.json")) else {
            panic!()
        };
        a.apply(Event::Delegated { agent, delegation });
        assert_eq!((a.delegations.of("a1").len(), a.delegations.counts("a1").asking), (2, 1));
        a.apply_from(Some("h"), Event::Delegations { agent: "h/a1".into(), items: a.delegations.of("a1").to_vec() });
        a.forget("h");
        assert_eq!((a.delegations.of("a1").len(), a.delegations.of("h/a1").len()), (2, 0));
    }

    #[test]
    fn a_child_on_a_linked_host_is_its_parents_whichever_of_its_listing_and_its_delegation_comes_first() {
        let delegation = Delegation { task_id: "t".into(), parent_id: "p".into(), child_id: "devbox/c".into(), ..Default::default() };
        let listed = || Event::Agents(vec![Summary { id: "devbox/c".into(), ..Default::default() }]);
        let lineage = |a: &Agents| a.get("devbox/c").map(|c| (c.parent.clone(), c.relationship.clone()));
        let child = Some(("p".to_string(), "subagent".to_string()));

        let mut listing_first = Agents::default();
        listing_first.apply_from(Some("devbox"), listed());
        assert_eq!(lineage(&listing_first), Some((String::new(), String::new())));
        listing_first.apply(Event::Delegated { agent: "p".into(), delegation: delegation.clone() });
        assert_eq!(lineage(&listing_first), child);

        let mut delegation_first = Agents::default();
        delegation_first.apply(Event::Delegations { agent: "p".into(), items: vec![delegation] });
        delegation_first.apply_from(Some("devbox"), listed());
        assert_eq!(lineage(&delegation_first), child);
        delegation_first.apply_from(Some("devbox"), Event::Agent(Box::new(Summary { id: "devbox/c".into(), status: "working".into(), ..Default::default() })));
        assert_eq!(lineage(&delegation_first), child, "the host's next update of it doesn't undo that");
    }

    #[test]
    fn an_exited_agent_leaves_the_list() {
        let mut a = Agents::default();
        let agent = |id: &str, status: &str| Summary { id: id.into(), status: status.into(), ..Default::default() };
        a.apply(Event::Agents(vec![agent("a", "idle"), agent("b", "working")]));
        a.apply(Event::Agent(Box::new(agent("a", "closed"))));
        a.apply(Event::Agent(Box::new(agent("c", "closed"))));
        assert_eq!(a.list, vec![agent("b", "working")]);
    }

    #[test]
    fn a_sessions_subagents_reach_down_every_generation_but_leave_forks_out() {
        let agent = |id: &str, parent: &str, relationship: &str| Summary {
            id: id.into(),
            parent: parent.into(),
            relationship: relationship.into(),
            ..Default::default()
        };
        let list = vec![agent("p", "", ""), agent("c", "p", "subagent"), agent("g", "c", "subagent"), agent("f", "p", "fork"), agent("o", "", "")];
        let a = Agents { list, ..Default::default() };
        let ids =
            |roots: &[&str]| a.subagents_of(&roots.iter().map(|r| r.to_string()).collect::<Vec<_>>()).into_iter().map(|s| s.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids(&["p"]), ["c", "g"]);
        assert_eq!(ids(&["p", "c"]), ["g"]);
        assert!(ids(&["o"]).is_empty());
    }

    #[test]
    fn an_agent_is_found_by_the_terminal_it_runs_in() {
        let agent = |id: &str, terminal: &str| Summary { id: id.into(), terminal_id: terminal.into(), ..Default::default() };
        let a = Agents { list: vec![agent("a", "t1"), agent("b", "t2")], ..Default::default() };
        assert_eq!(a.in_terminal("t2"), Some(&agent("b", "t2")));
        assert_eq!(a.in_terminal("shell"), None);
    }

    #[test]
    fn stream_items_upsert_in_seq_order() {
        let mut a = Agents::default();
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![item("2", 2, "b"), item("1", 1, "a")]));
        a.apply(Event::Items("a".into(), vec![item("2", 2, "b!")]));
        let t: Vec<_> = a.timelines["a"].iter().map(|i| i.text.as_str()).collect();
        assert_eq!(t, vec!["a", "b!"]);
    }

    #[test]
    fn a_batch_keeps_each_items_last_version_in_the_order_first_named() {
        let mut a = Agents::default();
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![item("1", 1, "held")]));
        a.apply(Event::Items("a".into(), vec![item("1", 1, "a"), item("2", 2, "b"), item("1", 1, "a!"), item("2", 2, "b!")]));
        let t: Vec<_> = a.timelines["a"].iter().map(|i| i.text.as_str()).collect();
        assert_eq!(t, vec!["a!", "b!"]);
        a.apply(Event::Items("b".into(), (0..6).map(|n| item(&format!("x{n}"), 5, "")).collect()));
        assert_eq!(
            a.timelines["b"].iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["x0", "x1", "x2", "x3", "x4", "x5"],
            "items of one seq keep their order"
        );
    }

    #[test]
    fn a_page_drops_held_items_it_no_longer_covers_and_keeps_those_streamed_since() {
        let mut a = Agents::default();
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        let texts = |a: &Agents| a.timelines["a"].iter().map(|i| i.text.clone()).collect::<Vec<_>>();
        a.apply(Event::Items("a".into(), vec![item("1", 1, "still running"), item("5", 5, "old")]));
        a.apply(Event::Items("a".into(), vec![item("8", 8, "streamed")]));
        a.apply(Event::Page { agent: "a".into(), items: vec![item("5", 5, "new"), item("6", 6, "more")], max_seq: 7 });
        assert_eq!(texts(&a), ["new", "more", "streamed"], "an item older than the page went stale while the link was down");
        a.apply(Event::Page { agent: "a".into(), items: Vec::new(), max_seq: 7 });
        assert_eq!(texts(&a), ["streamed"], "an empty page keeps only what came after it");
    }

    #[test]
    fn a_new_conversation_drops_the_old_items() {
        let mut a = Agents::default();
        let agent = |id: &str, resets| Summary { id: id.into(), resets, ..Default::default() };
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        a.apply(Event::Agents(vec![agent("a", 0), agent("b", 0)]));
        a.apply(Event::Items("a".into(), vec![item("1", 1, "old")]));
        a.apply(Event::Items("b".into(), vec![item("1", 1, "kept")]));
        a.apply(Event::Agent(Box::new(agent("a", 0))));
        assert_eq!(a.timelines["a"].len(), 1, "the same conversation keeps its items");
        let before = a.revision("a");
        a.apply(Event::Agent(Box::new(agent("a", 1))));
        assert!(!a.timelines.contains_key("a") && a.revision("a") > before, "the old conversation is gone and views refold");
        a.apply(Event::Items("a".into(), vec![item("2", 2, "new")]));
        a.apply(Event::Agents(vec![agent("a", 1), agent("b", 2)]));
        let texts = |id: &str| a.timelines.get(id).map(|t| t.iter().map(|i| i.text.clone()).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(texts("a"), ["new"]);
        assert!(texts("b").is_empty(), "a reset seen in a whole list drops too");
    }

    #[test]
    fn a_sub_agents_text_is_not_its_parents_last_word() {
        let mut a = Agents::default();
        let text = |id: &str, seq, parent: Option<&str>| Item {
            id: id.into(),
            seq,
            kind: "assistant".into(),
            text: id.into(),
            parent_tool_use_id: parent.map(Into::into),
            ..Default::default()
        };
        a.apply(Event::Items("a".into(), vec![text("mine", 1, None), text("theirs", 2, Some("tool"))]));
        assert_eq!(a.last_text("a"), Some("mine"));
    }

    #[test]
    fn permissions_stay_pending_until_resolved_or_reconnected() {
        let f: Frame = serde_json::from_str(
            r#"{"type":"permission.request","request":{"requestId":"r1","agentId":"a","toolName":"Bash","detail":{"kind":"shell","command":"pnpm test"}}}"#,
        )
        .unwrap();
        let p = f.request.unwrap();
        assert_eq!(p.ask(), "Wants to run pnpm test");
        let mut a = Agents::default();
        a.apply(Event::Asked(p.clone()));
        a.apply(Event::Asked(p));
        assert_eq!(a.pending.len(), 1);
        a.apply(Event::Resolved("r1".into()));
        assert!(a.pending.is_empty());
        a.apply(Event::Asked(Permission { request_id: "r2".into(), ..Default::default() }));
        a.apply(Event::Connected { scopes: vec![], caps: vec![], version: String::new() });
        assert!(a.pending.is_empty());
    }

    #[test]
    fn an_observer_is_offered_no_renames_or_locals_pocketd_would_refuse() {
        let mut a = Agents::default();
        let caps = vec![NAMES_CAP.to_string(), LOCALS_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        assert!(a.names_offered() && a.locals_offered());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        assert!(!a.names_offered() && !a.locals_offered());
    }

    #[test]
    fn a_tree_is_given_its_locals_name_else_its_rename() {
        let mut a = Agents::default();
        a.add_local(Local { id: "l1".into(), project: "/p".into(), name: "Review".into() });
        a.set_name("/p", "Hotfix");
        assert_eq!((a.given_name("l1"), a.given_name("/p"), a.given_name("/p-login")), (Some("Review"), Some("Hotfix"), None));
    }

    #[test]
    fn removing_a_project_deletes_its_locals_only_when_pocketd_takes_the_deletes() {
        let mut a = Agents::default();
        let locals = vec![
            Local { id: "l1".into(), project: "/p".into(), name: "Local 2".into() },
            Local { id: "l2".into(), project: "/q".into(), name: "Local 2".into() },
        ];
        let caps = vec![LOCALS_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        a.apply(Event::Locals(locals.clone()));
        assert_eq!(a.locals_removed_with("/p").collect::<Vec<_>>(), ["l1"]);
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        a.apply(Event::Locals(locals));
        assert_eq!(a.locals_removed_with("/p").count(), 0);
    }

    #[test]
    fn opening_branches_waits_for_pocketd_to_offer_it() {
        let mut a = Agents::default();
        assert!(!a.opens());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["open.v1".into()], version: String::new() });
        assert!(a.opens());
    }

    #[test]
    fn connecting_clears_the_host() {
        let mut a = Agents { host: Some(Host::default()), ..Default::default() };
        a.apply(Event::Connected { scopes: vec![], caps: vec![], version: String::new() });
        assert_eq!(a.host, None);
    }

    #[test]
    fn names_start_over_on_each_connect_and_wait_for_pocketd_to_offer_them() {
        let mut a = Agents::default();
        a.apply(Event::Names([("/w".to_string(), "Fix".to_string())].into()));
        assert!(!a.names_offered());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["names.v1".into()], version: String::new() });
        assert!(a.names.is_empty() && a.names_offered());
    }

    #[test]
    fn a_rename_shows_at_once_and_a_blank_one_clears_it() {
        let mut a = Agents::default();
        a.set_name("/w", " Mine ");
        assert_eq!(a.names["/w"], "Mine");
        a.set_name("/w", "  ");
        assert!(!a.names.contains_key("/w"));
    }

    #[test]
    fn locals_start_over_on_each_connect_and_show_a_rename_at_once() {
        let mut a = Agents::default();
        let frame = serde_json::from_str::<Frame>(
            r#"{"type":"local.list","locals":[{"id":"l1","project":"/p","name":"Local 2"},{"id":"l2","project":"/q","name":"Local 2"}]}"#,
        )
        .unwrap();
        a.apply(names_event(&frame).unwrap());
        assert_eq!(a.locals_in("/p").map(|l| l.id.as_str()).collect::<Vec<_>>(), ["l1"]);
        a.rename_local("l1", " Fix ");
        a.rename_local("l2", "  ");
        assert_eq!((a.local("l1").unwrap().name.as_str(), a.local("l2").unwrap().name.as_str()), ("Fix", "Local 2"));
        a.apply(Event::Connected { scopes: vec![], caps: vec!["locals.v1".into()], version: String::new() });
        assert!(a.locals.is_empty() && a.locals_offered());
    }

    #[test]
    fn automations_wait_for_pocketd_to_offer_them() {
        let mut a = Agents::default();
        assert!(!a.automations_offered());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["automations.v1".into()], version: String::new() });
        assert!(a.automations_offered());
    }

    #[test]
    fn each_pocketds_agents_asks_and_names_survive_the_others_snapshots_and_reconnects() {
        let agent = |id: &str| Summary { id: id.into(), ..Default::default() };
        let ask = |id: &str| Permission { request_id: id.into(), ..Default::default() };
        let mut a = Agents::default();
        a.apply(Event::Agents(vec![agent("a1")]));
        a.apply(Event::Asked(ask("q1")));
        a.apply(Event::Names(HashMap::from([("/src/app".into(), "App".into())])));
        a.apply_from(Some("devbox"), Event::Agents(vec![agent("devbox/b1")]));
        a.apply_from(Some("devbox"), Event::Asked(ask("devbox/q2")));
        a.apply_from(Some("devbox"), Event::Names(HashMap::from([("//devbox/home/me/app".into(), "Box".into())])));

        a.apply(Event::Agents(vec![agent("a2")]));
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: vec![], version: "v1".into() });
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["devbox/b1", "a2"]);
        assert_eq!(a.pending.iter().map(|p| p.request_id.as_str()).collect::<Vec<_>>(), ["devbox/q2"]);
        assert_eq!(a.names.keys().collect::<Vec<_>>(), ["//devbox/home/me/app"]);

        a.apply(Event::Asked(ask("q3")));
        a.apply_from(Some("devbox"), Event::Connected { scopes: vec![], caps: vec![], version: "v0".into() });
        a.apply_from(Some("devbox"), Event::Agents(vec![]));
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a2"]);
        assert_eq!(a.pending.iter().map(|p| p.request_id.as_str()).collect::<Vec<_>>(), ["q3"]);
        assert!(a.names.is_empty());
        assert_eq!((a.owner(), a.version.as_str()), (true, "v1"), "a host's hello says nothing about this Mac's");

        a.apply_from(Some("devbox"), Event::Agents(vec![agent("devbox/b2")]));
        a.forget("devbox");
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a2"]);
    }

    #[test]
    fn only_a_pocketd_that_said_hello_without_registry_v2_needs_updating() {
        let hello =
            |caps: &[&str]| Event::Connected { scopes: vec!["owner".into()], caps: caps.iter().map(|c| c.to_string()).collect(), version: String::new() };
        let mut a = Agents::default();
        assert!(!a.registry_outdated(Some("devbox")), "a host not yet heard from isn't known to be old");
        a.apply_from(Some("devbox"), hello(&["hosts.v1"]));
        a.apply(hello(&[REGISTRY_CAP]));
        assert!(a.registry_outdated(Some("devbox")) && !a.registry_outdated(None));
        a.apply_from(Some("devbox"), hello(&[REGISTRY_CAP]));
        assert!(!a.registry_outdated(Some("devbox")));
        a.apply_from(Some("old"), hello(&[]));
        a.forget("old");
        assert!(!a.registry_outdated(Some("old")));
    }

    #[test]
    fn an_edit_is_found_by_absolute_path_or_by_one_relative_to_the_agents_cwd() {
        let mut a = Agents::default();
        a.list.push(Summary { id: "a".into(), cwd: "/r".into(), ..Default::default() });
        let edit = |path: &str| Item {
            ts: 1,
            call: Some(Call { detail: Detail { kind: "edit".into(), path: path.into(), ..Default::default() }, ..Default::default() }),
            ..Default::default()
        };
        a.timelines.insert("a".into(), vec![edit("src/a.rs"), edit("/elsewhere/b.rs")]);
        assert!(a.last_edit("/r/src/a.rs").is_some());
        assert!(a.last_edit("/elsewhere/b.rs").is_some());
        assert!(a.last_edit("/r/src/b.rs").is_none());
        assert!(a.last_edit("/rr/src/a.rs").is_none());
    }

    #[test]
    fn applying_items_bumps_only_that_agents_revision() {
        let mut a = Agents::default();
        let item = |id: &str| Item { id: id.into(), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![item("1")]));
        a.apply(Event::Items("a".into(), vec![item("2")]));
        a.apply(Event::Agents(vec![]));
        assert_eq!((a.revision("a"), a.revision("b")), (2, 0));
    }
}
