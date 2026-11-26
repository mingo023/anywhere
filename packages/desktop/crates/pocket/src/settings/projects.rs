use super::catalog::{Look, Setting};
use super::fields::{filter_field, filter_matches};
use super::side::Machine;
use super::{HostsAct, Settings, SettingsEvent, action, dropdown};
use crate::modals::form::home;
use crate::settings::toasts::Toast;
use crate::spaces::header::tile;
use crate::util::tilde;
use agents::hosts::Link;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use theme::*;

/// The menu that asks which machine a new project goes on.
pub(super) const ADD_MENU: &str = "add-project";

/// A list longer than this gets a filter field.
pub(super) const FILTER_ABOVE: usize = 6;

fn worktree_count(n: usize) -> String {
    format!("{n} worktree{}", if n == 1 { "" } else { "s" })
}

pub(super) const ROWS: &[Setting] = &[
    Setting::path("worktree-folder", "Locations", "Worktree folder", |s| s.worktree_root(&home()), |s, p| s.worktree.root = p)
        .hint("New worktrees go in <folder>/<project>/<name>"),
    Setting::path("clone-folder", "Locations", "Clone folder", |s| s.clone_root(&home()), |s, p| s.worktree.clone_root = p)
        .hint("Where Add project › Clone puts new repositories"),
    Setting::custom("projects", "Projects", "Projects", |d, _, cx| d.project_list(cx))
        .hint("Open one to edit its icon, accounts, agents and setup script, or remove it")
        .bare(),
    Setting::switch("delete-branch", "Deleting a worktree", "Also delete its branch", |s| s.worktree.delete_branch, |s, on| s.worktree.delete_branch = on)
        .hint("Local branch only. Remote branches are never deleted"),
    Setting::choice(
        "worktree-files",
        "Deleting a worktree",
        "Worktree files",
        &["Move to Trash", "Delete permanently"],
        Look::Segmented,
        |s| usize::from(!s.worktree.trash),
        |s, i| s.worktree.trash = i == 0,
    ),
    Setting::stepper(
        "teardown-timeout",
        "Deleting a worktree",
        "Teardown script timeout",
        (10, 600, 10),
        " s",
        |s| s.worktree.teardown_secs as i32,
        |s, n| s.worktree.teardown_secs = n as u32,
    )
    .hint("Stops the project's teardown script if it runs longer")
    .advanced(),
];

/// How reachable a machine is, as its group header says it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MachineState {
    Local,
    Connected,
    Connecting,
    Offline,
}

impl MachineState {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Connected => "Connected",
            Self::Connecting => "Connecting\u{2026}",
            Self::Offline => "Offline",
        }
    }

    /// Whether a project there can be opened and added to.
    pub(super) fn reachable(self) -> bool {
        matches!(self, Self::Local | Self::Connected)
    }
}

/// A host's state from its link; no link at all is offline.
pub(crate) fn machine_state(link: Option<&Link>) -> MachineState {
    match link {
        Some(Link::Online { .. }) => MachineState::Connected,
        Some(Link::Connecting) => MachineState::Connecting,
        // pocketd keeps trying, but until a try lands nothing there answers.
        Some(Link::Retrying(_) | Link::Failed(_) | Link::Unknown) | None => MachineState::Offline,
    }
}

/// What the list needs of a project to group and match it.
pub(super) struct Entry<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub host: Option<&'a str>,
}

/// One line of the Projects list: a machine's header, or a project under it (by its index in the cards).
#[derive(Debug, PartialEq)]
pub(super) enum Line {
    Header { host: Option<String>, open: bool, total: usize },
    Project { card: usize },
}

/// The lines to draw. With projects on one machine there are no headers; with several, each machine that holds a match gets
/// one, with all its projects counted, and a machine with none that can take one gets an empty group to add it from. A
/// folded group opens while a filter has a match in it.
pub(super) fn lines(entries: &[Entry], machines: &[Machine], folded: &HashSet<Option<String>>, query: &str) -> Vec<Line> {
    let name_of =
        |host: Option<&str>| machines.iter().find(|m| m.host.as_deref() == host).map_or_else(|| host.unwrap_or_default().to_string(), |m| m.name.clone());
    let matches = |e: &Entry| filter_matches(query, &[e.name, e.path, name_of(e.host).as_str()]);
    let mut hosts: Vec<Option<&str>> = machines.iter().map(|m| m.host.as_deref()).collect();
    for e in entries {
        if !hosts.contains(&e.host) {
            hosts.push(e.host);
        }
    }
    if hosts.iter().filter(|h| entries.iter().any(|e| e.host == **h)).count() < 2 {
        return entries.iter().enumerate().filter(|(_, e)| matches(e)).map(|(card, _)| Line::Project { card }).collect();
    }
    let searching = !query.trim().is_empty();
    let addable = |host: Option<&str>| machines.iter().any(|m| m.host.as_deref() == host && foot(host, m.state) == Some(Foot::Add));
    let mut lines = Vec::new();
    for host in hosts {
        let held: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| e.host == host).map(|(i, _)| i).collect();
        let shown: Vec<usize> = held.iter().copied().filter(|&i| matches(&entries[i])).collect();
        if shown.is_empty() && (searching || !held.is_empty() || !addable(host)) {
            continue;
        }
        let open = searching || !folded.contains(&host.map(str::to_string));
        lines.push(Line::Header { host: host.map(str::to_string), open, total: held.len() });
        if open {
            lines.extend(shown.into_iter().map(|card| Line::Project { card }));
        }
    }
    lines
}

/// The filter field's hint: how many projects, and on how many machines when there are several.
/// The filter shows past `FILTER_ABOVE` projects, and stays while it holds a query so that query can be cleared.
fn shows_filter(projects: usize, query: &str) -> bool {
    projects > FILTER_ABOVE || !query.is_empty()
}

pub(super) fn placeholder(projects: usize, machines: usize) -> String {
    if machines > 1 { format!("Filter {projects} projects on {machines} machines") } else { format!("Filter {projects} projects") }
}

/// The machines a new project can go on, as indices into `machines`: the ones that answer.
pub(super) fn add_targets(machines: &[Machine]) -> Vec<usize> {
    machines.iter().enumerate().filter(|(_, m)| m.state.reachable()).map(|(i, _)| i).collect()
}

/// The filter, the folded groups and the open menu of the Projects page.
pub(crate) struct View {
    pub filter: Entity<InputState>,
    pub hosts_filter: Entity<InputState>,
    pub folded: HashSet<Option<String>>,
    /// The host whose actions menu is open, while `Settings::menu` is `ssh_hosts::HOST_MENU`.
    pub host_menu: Option<String>,
    hint: String,
    hosts_hint: String,
}

impl View {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Settings>) -> (Self, Vec<Subscription>) {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter projects"));
        let hosts_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter hosts"));
        let subs = [&filter, &hosts_filter]
            .map(|input| {
                cx.subscribe_in(input, window, |_, _, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .into();
        (Self { filter, hosts_filter, folded: HashSet::new(), host_menu: None, hint: String::new(), hosts_hint: String::new() }, subs)
    }

    /// Words the filters' hints once the counts change; an input only takes a new one with a window.
    fn sync_hints(&mut self, hint: String, hosts_hint: String, window: &mut Window, cx: &mut Context<Settings>) {
        if self.hint != hint {
            self.filter.update(cx, |f, cx| f.set_placeholder(hint.clone(), window, cx));
            self.hint = hint;
        }
        if self.hosts_hint != hosts_hint {
            self.hosts_filter.update(cx, |f, cx| f.set_placeholder(hosts_hint.clone(), window, cx));
            self.hosts_hint = hosts_hint;
        }
    }

    pub(crate) fn toggle(&mut self, host: Option<String>) {
        if !self.folded.remove(&host) {
            self.folded.insert(host);
        }
    }
}

fn pill(text: &'static str, state: MachineState) -> Div {
    let (bg, ink) = match state {
        MachineState::Local => (FILL_3, TEXT_2),
        MachineState::Connected => (ACCENT_BG, SUCCESS_TEXT),
        MachineState::Connecting => (WAITING_BG, WAITING_TEXT),
        MachineState::Offline => (FAILED_BG, FAILED_TEXT),
    };
    div()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(5.))
        .bg(bg)
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ink)
        .whitespace_nowrap()
        .child(text)
}

impl Settings {
    /// Words the filter fields' hints for the projects, machines and hosts there are now.
    pub(super) fn sync_filter_hints(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hosts: HashSet<_> = self.side.projects.iter().map(|c| c.host.as_deref()).collect();
        let (projects, hosts) = (placeholder(self.side.projects.len(), hosts.len()), format!("Filter {} hosts", self.side.hosts.rows.len() + 1));
        self.projects.sync_hints(projects, hosts, window, cx);
    }

    fn machine(&self, host: &Option<String>) -> Option<&Machine> {
        self.side.machines.iter().find(|m| m.host == *host)
    }

    /// One card on one machine; with several, a card per machine under its header, each ending in what it can do.
    pub(super) fn project_list(&mut self, cx: &mut Context<Self>) -> Div {
        let entries: Vec<Entry> = self.side.projects.iter().map(|c| Entry { name: &c.name, path: &c.path, host: c.host.as_deref() }).collect();
        let query = self.projects.filter.read(cx).value().to_string();
        let lines = lines(&entries, &self.side.machines, &self.projects.folded, &query);
        let filter = shows_filter(self.side.projects.len(), &query).then(|| super::card(vec![filter_field(&self.projects.filter)]));
        let empty = || div().px(px(14.)).py(px(12.)).text_size(px(12.5)).text_color(TEXT_3).child("No project matches");
        let list = div().flex().flex_col().gap(px(14.)).children(filter);
        if !lines.iter().any(|l| matches!(l, Line::Header { .. })) {
            let mut rows: Vec<Div> =
                lines.iter().filter_map(|l| if let Line::Project { card } = l { Some(div().child(self.project_row(*card, true, cx))) } else { None }).collect();
            if rows.is_empty() {
                rows.push(empty());
            }
            rows.push(self.project_foot(cx));
            return list.child(super::card(rows));
        }
        let mut blocks: Vec<Div> = Vec::new();
        let mut i = 0;
        while let Some(Line::Header { host, open, total }) = lines.get(i) {
            i += 1;
            let machine = self.machine(host);
            let state = machine.map_or(MachineState::Offline, |m| m.state);
            let mut block = div().flex().flex_col().gap(px(6.)).child(self.group_header(host, machine, *open, *total, cx));
            let mut rows = Vec::new();
            while let Some(Line::Project { card }) = lines.get(i) {
                rows.push(div().child(self.project_row(*card, state.reachable(), cx)));
                i += 1;
            }
            if *open {
                rows.extend(self.machine_foot(host, machine, cx));
                block = block.child(super::card(rows));
            }
            blocks.push(block);
        }
        if blocks.is_empty() {
            return list.child(super::card(vec![empty()]));
        }
        list.children(blocks)
    }

    fn group_header(&self, host: &Option<String>, machine: Option<&Machine>, open: bool, total: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = host.clone().unwrap_or_default();
        let (name, glyph, colour, state) = match machine {
            Some(m) => (m.name.clone(), m.glyph, m.colour, m.state),
            None => (key.clone(), "server", None, MachineState::Offline),
        };
        let fold = host.clone();
        div()
            .id(SharedString::from(format!("projects-fold-{key}")))
            .h(px(24.))
            .ml(px(2.))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .tab_index(0)
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 11., TEXT_3))
            .child(tile(glyph, colour, 18.))
            .child(div().truncate().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(name))
            .child(pill(state.label(), state))
            .child(div().text_size(px(12.)).text_color(TEXT_3).child(total.to_string()))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.projects.toggle(fold.clone());
                cx.notify();
            }))
    }

    /// The last row of a machine's card: add a project there, or reconnect it.
    fn machine_foot(&self, host: &Option<String>, machine: Option<&Machine>, cx: &mut Context<Self>) -> Option<Div> {
        let key = host.clone().unwrap_or_default();
        let name = machine.map_or_else(|| key.clone(), |m| m.name.clone());
        let state = machine.map_or(MachineState::Offline, |m| m.state);
        let row = div().px(px(14.)).py(px(8.)).flex().items_center().gap(px(12.)).bg(FILL_1).rounded_b(px(12.));
        match foot(host.as_deref(), state)? {
            Foot::Add => {
                let target = host.clone();
                let add = action(SharedString::from(format!("projects-add-{key}")), "")
                    .pl(px(4.))
                    .gap(px(6.))
                    .child(icon("plus", 13., TEXT_2))
                    .child(if host.is_some() { format!("Add project on {name}\u{2026}") } else { "Add project\u{2026}".to_string() })
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(add_event(target.clone()))));
                let clone = host.is_none().then(|| {
                    ui::link("settings-clone-project", "Clone\u{2026}").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::CloneProject)))
                });
                Some(row.child(add).children(clone))
            }
            Foot::Reconnect(id) => {
                let id = id.to_string();
                let reconnect = action(SharedString::from(format!("projects-reconnect-{key}")), "Reconnect")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::Retry(id.clone())))));
                let note = div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(TEXT_3)
                    .child(format!("Can\u{2019}t reach {name}. Its projects open once it reconnects."));
                Some(row.child(note).child(reconnect))
            }
            Foot::Waiting => Some(row.child(div().text_size(px(12.)).text_color(TEXT_3).child(format!("Connecting to {name}\u{2026}")))),
        }
    }

    fn project_row(&self, i: usize, reachable: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let card = &self.side.projects[i];
        let p = card.path.clone();
        let host_name = self.machine(&card.host).map_or_else(|| card.host.clone().unwrap_or_default(), |m| m.name.clone());
        let picks = card.picks.iter().map(|(provider, name)| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(3.))
                .font_family(SANS)
                .text_color(TEXT_2)
                .child(provider_icon(provider, 10., TEXT_3))
                .child(name.clone())
        });
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(card.name.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .text_size(px(11.5))
                    .text_color(TEXT_3)
                    .child(div().min_w_0().truncate().font_family(MONO).child(match hosts::split(&card.path) {
                        (None, path) => tilde(path),
                        (Some(_), path) => path.to_string(),
                    }))
                    .children(picks.map(|pick| div().flex().flex_none().items_center().gap(px(6.)).child("\u{b7}").child(pick))),
            );
        div()
            .id(("settings-project", i))
            .px(px(14.))
            .py(px(9.))
            .flex()
            .items_center()
            .gap(px(12.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_1))
            .when(!reachable, |d| d.opacity(0.55))
            .child(card.tile(24.))
            .child(text)
            .children(card.worktrees.map(|n| div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(worktree_count(n))))
            .child(icon("chevron-right", 13., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if reachable {
                    cx.emit(SettingsEvent::EditProject(p.clone()));
                } else {
                    this.toast(Toast::tell(offline_note(&host_name)), cx);
                }
            }))
    }

    fn project_foot(&mut self, cx: &mut Context<Self>) -> Div {
        let targets = add_targets(&self.side.machines);
        let asks = targets.len() > 1;
        let open = asks && self.menu == Some(ADD_MENU);
        let add = action("settings-add-project", "").pl(px(4.)).gap(px(6.)).child(icon("plus", 13., TEXT_2)).child("Add project\u{2026}");
        let add = if asks {
            dropdown::trigger(add, ADD_MENU, open, cx)
        } else {
            add.on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::AddProject)))
        };
        let menu = open.then(|| {
            let options = targets.iter().map(|&n| {
                let m = &self.side.machines[n];
                let host = m.host.clone();
                dropdown::pick_row(("add-project-on", n), false).gap(px(8.)).child(tile(m.glyph, m.colour, 18.)).child(div().child(m.name.clone())).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.menu = None;
                        cx.emit(add_event(host.clone()));
                        cx.notify();
                    }),
                )
            });
            let options: Vec<_> = options.collect();
            ui::menu_in("add-project-menu-in", dropdown::shuts(ui::pop(div()), ADD_MENU, cx).min_w(px(180.)).p(px(4.)).flex().flex_col().children(options))
        });
        let clone = ui::link("settings-clone-project", "Clone\u{2026}").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::CloneProject)));
        div()
            .px(px(14.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(12.))
            .bg(FILL_1)
            .rounded_b(px(12.))
            .child(div().relative().child(add).children(menu.map(|m| ui::dropdown(30., m))))
            .child(clone)
    }
}

/// What a machine's card ends with.
#[derive(Debug, PartialEq)]
enum Foot<'a> {
    Add,
    Reconnect(&'a str),
    Waiting,
}

fn foot(host: Option<&str>, state: MachineState) -> Option<Foot<'_>> {
    match (host, state) {
        (_, MachineState::Local | MachineState::Connected) => Some(Foot::Add),
        (Some(id), MachineState::Offline) => Some(Foot::Reconnect(id)),
        (_, MachineState::Connecting) => Some(Foot::Waiting),
        (None, MachineState::Offline) => None,
    }
}

/// Adding on this Mac is the Finder's folder picker; on a host it's that host's folders.
fn add_event(host: Option<String>) -> SettingsEvent {
    match host {
        None => SettingsEvent::AddProject,
        Some(id) => SettingsEvent::AddProjectOn(id),
    }
}

/// What a click on an offline host's project says.
fn offline_note(host: &str) -> String {
    format!("{host} is offline. Its project settings open once it reconnects")
}

#[cfg(test)]
mod tests {
    use super::{Entry, Foot, Line, MachineState, add_targets, foot, lines, machine_state, placeholder, shows_filter, worktree_count};
    use crate::settings::side::Machine;
    use agents::hosts::{Failure, Link};
    use std::collections::HashSet;

    fn machine(host: Option<&str>, name: &str, state: MachineState) -> Machine {
        Machine { host: host.map(str::to_string), name: name.into(), glyph: "server", colour: None, state }
    }

    fn machines() -> Vec<Machine> {
        vec![
            machine(None, "This Mac", MachineState::Local),
            machine(Some("build"), "build-box", MachineState::Connected),
            machine(Some("mini"), "mac-mini", MachineState::Offline),
        ]
    }

    fn entry<'a>(name: &'a str, path: &'a str, host: Option<&'a str>) -> Entry<'a> {
        Entry { name, path, host }
    }

    fn fleet() -> Vec<Entry<'static>> {
        vec![
            entry("anywhere", "/Users/me/code/anywhere", None),
            entry("api", "/srv/api", Some("build")),
            entry("web", "/Users/mini/web", Some("mini")),
            entry("docs", "/Users/me/code/docs", None),
            entry("anywhere", "/srv/anywhere", Some("build")),
        ]
    }

    fn header(host: Option<&str>, open: bool, total: usize) -> Line {
        Line::Header { host: host.map(str::to_string), open, total }
    }

    #[test]
    fn the_filter_stays_while_it_holds_a_query_after_projects_drop_below_the_threshold() {
        assert!(!shows_filter(6, "") && shows_filter(7, "") && shows_filter(6, "api"));
    }

    #[test]
    fn one_worktree_is_singular() {
        assert_eq!([worktree_count(1), worktree_count(3)], ["1 worktree", "3 worktrees"]);
    }

    #[test]
    fn projects_on_one_machine_stay_a_flat_list() {
        let flat = [entry("a", "/a", None), entry("b", "/b", None)];
        assert_eq!(lines(&flat, &machines(), &HashSet::new(), ""), [Line::Project { card: 0 }, Line::Project { card: 1 }]);
    }

    #[test]
    fn projects_on_several_machines_group_under_a_header_each_in_machine_order() {
        assert_eq!(
            lines(&fleet(), &machines(), &HashSet::new(), ""),
            [
                header(None, true, 2),
                Line::Project { card: 0 },
                Line::Project { card: 3 },
                header(Some("build"), true, 2),
                Line::Project { card: 1 },
                Line::Project { card: 4 },
                header(Some("mini"), true, 1),
                Line::Project { card: 2 },
            ]
        );
    }

    #[test]
    fn a_folded_group_shows_its_header_and_count_but_no_projects() {
        let folded = HashSet::from([Some("build".to_string())]);
        let shown = lines(&fleet(), &machines(), &folded, "");
        assert!(shown.contains(&header(Some("build"), false, 2)));
        assert!(!shown.contains(&Line::Project { card: 1 }) && !shown.contains(&Line::Project { card: 4 }));
        assert!(shown.contains(&Line::Project { card: 0 }), "other groups stay open");
    }

    #[test]
    fn a_filter_opens_folded_groups_that_hold_a_match_and_hides_the_rest() {
        let folded = HashSet::from([Some("build".to_string()), None]);
        assert_eq!(lines(&fleet(), &machines(), &folded, "api"), [header(Some("build"), true, 2), Line::Project { card: 1 }]);
    }

    #[test]
    fn a_filter_matches_a_projects_name_path_and_host_name() {
        let only = |query| {
            lines(&fleet(), &machines(), &HashSet::new(), query)
                .into_iter()
                .filter_map(|l| if let Line::Project { card } = l { Some(card) } else { None })
                .collect::<Vec<_>>()
        };
        assert_eq!(only("docs"), [3]);
        assert_eq!(only("/srv"), [1, 4]);
        assert_eq!(only("mac-mini"), [2]);
        assert_eq!(only("anywhere build"), [4]);
        assert!(only("nothing").is_empty());
    }

    #[test]
    fn a_filter_that_leaves_one_machine_still_has_its_header_while_others_hold_projects() {
        assert_eq!(lines(&fleet(), &machines(), &HashSet::new(), "web"), [header(Some("mini"), true, 1), Line::Project { card: 2 }]);
    }

    #[test]
    fn a_project_on_a_host_pocketd_no_longer_lists_still_gets_a_group() {
        let gone = [entry("a", "/a", None), entry("b", "/b", Some("old"))];
        assert_eq!(
            lines(&gone, &machines(), &HashSet::new(), ""),
            [header(None, true, 1), Line::Project { card: 0 }, header(Some("build"), true, 0), header(Some("old"), true, 1), Line::Project { card: 1 }]
        );
    }

    #[test]
    fn a_reachable_machine_with_no_projects_gets_an_empty_group_to_add_one_from_unless_filtering() {
        let remote = [entry("api", "/srv/api", Some("build")), entry("web", "/Users/mini/web", Some("mini"))];
        let shown = lines(&remote, &machines(), &HashSet::new(), "");
        assert_eq!(shown[0], header(None, true, 0), "This Mac still offers Add and Clone");
        assert!(!lines(&remote, &machines(), &HashSet::new(), "api").contains(&header(None, true, 0)));
    }

    #[test]
    fn the_filter_hint_counts_projects_and_names_machines_only_when_there_are_several() {
        assert_eq!([placeholder(9, 1), placeholder(10, 3)], ["Filter 9 projects", "Filter 10 projects on 3 machines"]);
    }

    #[test]
    fn a_link_is_connected_connecting_or_offline() {
        let online = Link::Online { version: "1.4.2".into(), uptime: 5 };
        assert_eq!(
            [
                machine_state(Some(&online)),
                machine_state(Some(&Link::Connecting)),
                machine_state(Some(&Link::Retrying(Failure::Dropped))),
                machine_state(Some(&Link::Failed(Failure::Auth))),
                machine_state(None)
            ],
            [MachineState::Connected, MachineState::Connecting, MachineState::Offline, MachineState::Offline, MachineState::Offline]
        );
    }

    #[test]
    fn only_this_mac_and_connected_hosts_take_a_new_project() {
        assert_eq!(add_targets(&machines()), [0, 1]);
    }

    #[test]
    fn a_machine_s_card_ends_in_add_while_reachable_and_reconnect_while_offline() {
        assert_eq!(foot(None, MachineState::Local), Some(Foot::Add));
        assert_eq!(foot(Some("devbox"), MachineState::Connected), Some(Foot::Add));
        assert_eq!(foot(Some("devbox"), MachineState::Offline), Some(Foot::Reconnect("devbox")));
        assert_eq!(foot(Some("devbox"), MachineState::Connecting), Some(Foot::Waiting));
    }
}
