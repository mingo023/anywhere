//! Spaces: each machine, this Mac or an SSH host, is a space. The sidebar shows one at a time and a switcher at its foot moves between them.

pub(crate) mod cards;
pub(crate) mod header;
pub(crate) mod offline;
pub(crate) mod open;
pub(crate) mod switcher;
pub(crate) mod theme_card;
pub(crate) mod wash;

use crate::actions::SwitchSpace;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::status::{self, Status};
use crate::util::{LOCAL_ICON, folder};
use agents::hosts::Link;
use agents::locals::is_local;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::ACCENT_SWATCHES;

/// A machine: this Mac, or an SSH host named by its id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub(crate) enum Space {
    #[default]
    Local,
    Host(String),
}

impl Space {
    /// The space of the machine `hosts::split` names: a host's id, or none for this Mac.
    pub(crate) fn on(machine: Option<&str>) -> Self {
        machine.map_or(Space::Local, |id| Space::Host(id.to_string()))
    }

    pub(crate) fn host(&self) -> Option<&str> {
        match self {
            Space::Local => None,
            Space::Host(id) => Some(id),
        }
    }
}

/// The space a project key lives in. A folder on two machines is two keys, so two projects.
pub(crate) fn space_of(project: &str) -> Space {
    Space::on(hosts::split(project).0)
}

/// Whether a session on `machine` (none when unknown) may show in `on_screen`, unless the space is locked.
pub(crate) fn shows_session(machine: Option<&Space>, on_screen: &Space, locked: bool) -> bool {
    !locked && machine.is_none_or(|m| m == on_screen)
}

/// `projects` in `space`, in their order.
pub(crate) fn in_space(projects: &[String], space: &Space) -> Vec<String> {
    projects.iter().filter(|p| space_of(p) == *space).cloned().collect()
}

/// This Mac, then the hosts in the order Settings lists them.
pub(crate) fn order<'a>(hosts: impl IntoIterator<Item = &'a str>) -> Vec<Space> {
    std::iter::once(Space::Local).chain(hosts.into_iter().map(|id| Space::Host(id.to_string()))).collect()
}

/// `saved` while its machine is still a space, else This Mac. A host's list arrives after launch, so this is read each time and the saved choice kept until its host is really gone.
pub(crate) fn resolve(saved: &Space, order: &[Space]) -> Space {
    if order.contains(saved) { saved.clone() } else { Space::Local }
}

/// How many of the sessions waiting on the user, each named by its project, are in `space`.
pub(crate) fn waiting<'a>(projects: impl IntoIterator<Item = &'a str>, space: &Space) -> usize {
    projects.into_iter().filter(|p| space_of(p) == *space).count()
}

/// The space `by` places from `current`; none past either end.
pub(crate) fn step(order: &[Space], current: &Space, by: isize) -> Option<Space> {
    let at = order.iter().position(|s| s == current)?;
    order.get(at.checked_add_signed(by)?).cloned()
}

/// The `n`th space, counting from 1 as ⌃1 does.
pub(crate) fn nth(order: &[Space], n: usize) -> Option<Space> {
    order.get(n.checked_sub(1)?).cloned()
}

/// The project in `space` with `project`'s folder name, the one copy of it there.
pub(crate) fn twin<'a>(project: &str, space: &Space, projects: &'a [String]) -> Option<&'a String> {
    let name = folder(hosts::split(project).1);
    projects.iter().filter(|p| space_of(p) == *space).find(|p| folder(hosts::split(p).1) == name)
}

/// The project to show on arriving in `space` from `current`: its twin there, else the space's first; none when `current` is already there.
pub(crate) fn arrival(current: Option<&str>, space: &Space, projects: &[String]) -> Option<String> {
    if current.is_some_and(|p| space_of(p) == *space) {
        return None;
    }
    current.and_then(|p| twin(p, space, projects)).or(projects.first()).cloned()
}

/// What `Spaces::flash` is keyed by for a space's switcher button.
pub(crate) fn flash_key(space: &Space) -> String {
    format!("space:{}", space.host().unwrap_or_default())
}

/// `space`'s colour as RGB: the saved one, else the palette's by `position` in `order`. This Mac has none.
pub(crate) fn colour(space: &Space, saved: &HashMap<String, u32>, position: usize) -> Option<u32> {
    let id = space.host()?;
    Some(saved.get(id).copied().unwrap_or(ACCENT_SWATCHES[position.saturating_sub(1) % ACCENT_SWATCHES.len()]))
}

/// The icons a host's space can take; the first is the one it has until the user picks another.
pub(crate) const HOST_ICONS: [&str; 8] = ["server", "cloud", "globe", "terminal", "code", "bot", "bolt", "star"];

/// `space`'s icon: This Mac's, else the host's saved one while it is still offered, else the first.
pub(crate) fn glyph(space: &Space, saved: &HashMap<String, String>) -> &'static str {
    let Some(id) = space.host() else { return LOCAL_ICON };
    saved.get(id).and_then(|s| HOST_ICONS.into_iter().find(|i| i == s)).unwrap_or(HOST_ICONS[0])
}

/// The name a host's rename saves: none, so the name it was added with shows, when `typed` is blank or that name.
pub(crate) fn kept_name(typed: &str, added: &str) -> Option<String> {
    let typed = typed.trim();
    (!typed.is_empty() && typed != added).then(|| typed.to_string())
}

/// A project's name with the machine it is on, as the palette lists it.
pub(crate) fn labelled(name: &str, machine: Option<&str>) -> String {
    machine.map_or_else(|| name.to_string(), |m| format!("{name} on {m}"))
}

/// `label` ending in an ellipsis, naming the host when the action is on one.
pub(crate) fn on_host(label: &str, host: Option<&str>) -> String {
    host.map_or_else(|| format!("{label}\u{2026}"), |h| format!("{label} on {h}\u{2026}"))
}

/// A page's crumbs, led by the machine's name unless it is This Mac.
pub(crate) fn crumbs(machine: Option<String>, rest: impl IntoIterator<Item = String>) -> Vec<String> {
    machine.into_iter().chain(rest).collect()
}

/// Whether a machine can be worked on now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reach {
    Online,
    Connecting,
    Offline,
}

pub(crate) fn reach(space: &Space, link: Option<&Link>) -> Reach {
    if *space == Space::Local {
        return Reach::Online;
    }
    match link {
        Some(Link::Online { .. }) => Reach::Online,
        Some(Link::Connecting) => Reach::Connecting,
        _ => Reach::Offline,
    }
}

/// The offline page's title.
pub(crate) fn offline_title(reach: Reach, name: &str) -> String {
    if reach == Reach::Connecting { format!("{name} is connecting\u{2026}") } else { format!("{name} is offline") }
}

/// What the offline page says under its title.
pub(crate) fn offline_note(link: Option<&Link>, name: &str) -> String {
    match link {
        Some(Link::Connecting) => "Signing in over SSH.".into(),
        Some(Link::Failed(f)) => format!("{} Agents there keep going.", hosts::failure_message(*f, name)),
        _ => "Lost the connection. Agents there keep going.".into(),
    }
}

/// The side a space slides in from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Heading {
    /// Toward the end of the switcher: in from the right.
    Forward,
    Back,
}

impl Heading {
    /// The sign a slide's offset takes: positive starts right of its place.
    pub(crate) fn sign(self) -> f32 {
        if self == Heading::Forward { 1. } else { -1. }
    }
}

pub(crate) fn heading(order: &[Space], from: &Space, to: &Space) -> Heading {
    let at = |s: &Space| order.iter().position(|o| o == s);
    if at(to) < at(from) { Heading::Back } else { Heading::Forward }
}

/// How far a two-finger swipe must travel sideways to switch, as the design's prototype has it.
const SWIPE: f32 = 80.;

/// A two-finger swipe over the sidebar, summed until it moves a space. It moves one a gesture: the trackpad's momentum after the fingers lift would carry a flick across the next.
#[derive(Default)]
pub(crate) struct Swipe {
    sum: f32,
    spent: bool,
}

impl Swipe {
    /// Takes a scroll of `dx`, `dy` px; `started` when fingers touch down anew. Returns the spaces to move by: fingers left go on to the next.
    pub(crate) fn scroll(&mut self, dx: f32, dy: f32, started: bool) -> Option<isize> {
        if started {
            *self = Self::default();
        }
        if self.spent || dx.abs() < dy.abs() {
            return None;
        }
        self.sum += dx;
        if self.sum.abs() < SWIPE {
            return None;
        }
        self.spent = true;
        Some(if self.sum < 0. { 1 } else { -1 })
    }
}

const SLIDE: Duration = Duration::from_millis(220);
/// How long a sign-in may take before its space locks.
const GRACE: Duration = Duration::from_millis(500);
/// How long a row or button a change just made stays lit.
pub(crate) const FLASH: Duration = Duration::from_millis(1900);

#[derive(Clone, Copy)]
struct Slide {
    n: u64,
    heading: Heading,
    at: Instant,
}

/// Which space the sidebar shows, and the transient state of its switching.
pub(crate) struct Spaces {
    /// As saved; may name a host that is gone, see `resolve`.
    shown: Space,
    pub(crate) swipe: Swipe,
    slide: Option<Slide>,
    switches: u64,
    /// What a change just made, a project's key or a space's `flash_key`, and when.
    flash: Option<(String, Instant)>,
    /// The session each space showed when the sidebar left it.
    left: HashMap<Space, Spot>,
    /// The appearance the space on screen's look last asked for; `None` left the app's.
    pub(crate) look_dark: Option<bool>,
    unreached: HashMap<Space, Unreached>,
}

/// A space seen on screen while its machine couldn't be reached: since when, and whether its link failed meanwhile.
struct Unreached {
    since: Instant,
    failed: bool,
}

/// What noting a space's reach calls for.
#[derive(Debug, PartialEq)]
pub(crate) enum Tracked {
    /// A sign-in just began: draw again once this runs out, as the space may lock.
    Grace(Duration),
    /// The space came back from unreachable.
    Back,
}

/// A session on screen: its project, and its tree, `None` for the project's main worktree.
pub(crate) type Spot = (String, Option<String>);

impl Spaces {
    pub(crate) fn new(saved: Option<&str>) -> Self {
        Self {
            shown: Space::on(saved),
            swipe: Swipe::default(),
            slide: None,
            switches: 0,
            flash: None,
            left: HashMap::new(),
            look_dark: None,
            unreached: HashMap::new(),
        }
    }

    pub(crate) fn current(&self, order: &[Space]) -> Space {
        resolve(&self.shown, order)
    }

    /// Shows `to`, sliding in from the side it lies on; false when it is already shown. Keeps `leaving` for its space; none forgets the space's.
    pub(crate) fn go(&mut self, to: Space, order: &[Space], now: Instant, leaving: Option<Spot>) -> bool {
        let from = self.current(order);
        if from == to {
            return false;
        }
        match leaving {
            Some(spot) => {
                self.left.insert(space_of(&spot.0), spot);
            }
            None => {
                self.left.remove(&from);
            }
        }
        self.switches += 1;
        self.slide = Some(Slide { n: self.switches, heading: heading(order, &from, &to), at: now });
        self.shown = to;
        true
    }

    /// The session `space` showed when the sidebar last left it.
    pub(crate) fn left_in(&self, space: &Space) -> Option<&Spot> {
        self.left.get(space)
    }

    /// The slide in flight: a number for each switch, so the animation restarts, and its side.
    pub(crate) fn sliding(&self, now: Instant) -> Option<(u64, Heading)> {
        self.slide.filter(|s| now.duration_since(s.at) < SLIDE).map(|s| (s.n, s.heading))
    }

    /// The spaces last seen unreachable.
    pub(crate) fn unreached(&self) -> impl Iterator<Item = &Space> {
        self.unreached.keys()
    }

    /// Notes the reach of `space`.
    pub(crate) fn track(&mut self, space: &Space, reach: Reach, now: Instant) -> Option<Tracked> {
        if reach == Reach::Online {
            return self.unreached.remove(space).map(|_| Tracked::Back);
        }
        if let Some(u) = self.unreached.get_mut(space) {
            u.failed |= reach == Reach::Offline;
            return None;
        }
        self.unreached.insert(space.clone(), Unreached { since: now, failed: reach == Reach::Offline });
        (reach == Reach::Connecting).then_some(Tracked::Grace(GRACE))
    }

    /// Whether `space` shows its offline page and locks its lists. A first sign-in gets `GRACE`, so a quick one never
    /// flashes them; once its link has failed they stay until it's back.
    pub(crate) fn locked(&self, space: &Space, reach: Reach, now: Instant) -> bool {
        match reach {
            Reach::Online => false,
            Reach::Offline => true,
            Reach::Connecting => self.unreached.get(space).is_some_and(|u| u.failed || now.duration_since(u.since) >= GRACE),
        }
    }

    /// Lights `key` until `FLASH` has passed.
    pub(crate) fn flash(&mut self, key: String, now: Instant) {
        self.flash = Some((key, now));
    }

    pub(crate) fn flashed(&self, key: &str, now: Instant) -> bool {
        self.flash.as_ref().is_some_and(|(k, at)| k == key && now.duration_since(*at) < FLASH)
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(None);
    }
}

pub(crate) const SLIDE_MS: u64 = SLIDE.as_millis() as u64;

impl Desktop {
    pub(crate) fn space_order(&self) -> Vec<Space> {
        order(self.fleet.hosts.list.iter().map(|h| h.id.as_str()))
    }

    pub(crate) fn space(&self) -> Space {
        self.spaces.current(&self.space_order())
    }

    /// `space`'s name: This Mac, or the host's.
    pub(crate) fn space_name(&self, space: &Space) -> String {
        self.space_host_name(space).unwrap_or_else(|| "This Mac".to_string())
    }

    /// `space`'s host's name; none for This Mac.
    pub(crate) fn space_host_name(&self, space: &Space) -> Option<String> {
        space.host().map(|id| self.host_name(id))
    }

    /// The name of the host a project is on; none for one on this Mac.
    pub(crate) fn project_host_name(&self, project: &str) -> Option<String> {
        hosts::split(project).0.map(|id| self.host_name(id))
    }

    pub(crate) fn space_reach(&self, space: &Space) -> Reach {
        reach(space, space.host().and_then(|id| self.fleet.hosts.links.get(id)))
    }

    /// A project's name, with its host's when it is on one.
    pub(crate) fn labelled_project(&self, project: &str) -> String {
        labelled(&self.repo_name(project), self.project_host_name(project).as_deref())
    }

    /// The projects the sidebar lists in `space`: not its Scratch folders.
    pub(crate) fn space_projects(&self, space: &Space) -> Vec<String> {
        in_space(&self.store.without_scratch(self.store.projects.clone()), space)
    }

    pub(crate) fn loose_folders(&self, space: &Space, cx: &App) -> Vec<String> {
        in_space(&unadded(self.projects(cx), &self.store.projects), space)
    }

    /// The projects of the sessions waiting on the user, one entry each.
    pub(crate) fn waiting_projects(&self, cx: &App) -> Vec<String> {
        let projects = self.projects(cx);
        status::listed(&self.agents.list)
            .into_iter()
            .filter(|a| Status::of(a) == Some(Status::NeedsYou))
            .filter_map(|a| self.agent_project(a, &self.terminals.read(cx).sessions.get(&a.terminal_id)?.info.cwd, &projects).cloned())
            .collect()
    }

    /// Shows `to` with the session it was left on, else the new-session page on the twin of the project before, else its first.
    pub(crate) fn switch_space(&mut self, to: Space, window: &mut Window, cx: &mut Context<Self>) {
        self.enter_space(to, true, window, cx);
    }

    /// Shows `to` with the new-session page and its draft moved there.
    pub(crate) fn switch_space_with_draft(&mut self, to: Space, window: &mut Window, cx: &mut Context<Self>) {
        self.enter_space(to, false, window, cx);
    }

    fn enter_space(&mut self, to: Space, back_to_session: bool, window: &mut Window, cx: &mut Context<Self>) {
        // An unreachable space's status shows on Sessions, and its rows can't lead there.
        if self.screen == Screen::Settings || self.space_reach(&to) != Reach::Online {
            self.screen = Screen::Sessions;
        }
        if !self.go_space(&to, cx) {
            return cx.notify();
        }
        self.remember_space(&to, cx);
        if self.space_reach(&to) == Reach::Online {
            self.land(&to, back_to_session, window, cx);
        }
        self.land_draft(window, cx);
        cx.notify();
    }

    /// Opens the session `to` showed when the sidebar left it, else the place `arrival` picks.
    pub(crate) fn land(&mut self, to: &Space, back_to_session: bool, window: &mut Window, cx: &mut Context<Self>) {
        match self.spot_left_in(to, cx).filter(|_| back_to_session) {
            Some((project, tree)) => self.select_tree(project, tree, cx),
            None => {
                let projects = in_space(&self.form_projects(cx), to);
                let from = self.drafted_project(cx).or_else(|| self.project.clone());
                if let Some(target) = arrival(from.as_deref(), to, &projects) {
                    self.new_place(target, window, cx);
                }
            }
        }
    }

    /// Keeps the sidebar in the space of the project now shown, as a palette pick or a notification can show one anywhere.
    pub(crate) fn follow_space(&mut self, project: &str, cx: &mut Context<Self>) {
        let to = space_of(project);
        if self.go_space(&to, cx) {
            self.remember_space(&to, cx);
        }
    }

    fn go_space(&mut self, to: &Space, cx: &mut Context<Self>) -> bool {
        let order = self.space_order();
        self.sidebar.read(cx).space_header.clone().update(cx, |h, cx| h.shut(cx));
        self.spaces.go(to.clone(), &order, Instant::now(), self.spot(cx))
    }

    /// The session on screen; none while the new-session page is up.
    fn spot(&self, cx: &App) -> Option<Spot> {
        Some((self.project.clone().filter(|_| !self.new_form.read(cx).shown())?, self.worktree.clone()))
    }

    /// The session `space` showed when it was left, while its tree is still listed.
    fn spot_left_in(&self, space: &Space, cx: &App) -> Option<Spot> {
        let (project, tree) = self.spaces.left_in(space)?;
        let listed = match tree {
            None => self.projects(cx).contains(project),
            Some(t) if is_local(t) => self.agents.local(t).is_some_and(|l| l.project == *project),
            Some(t) => self.listed_trees(project).is_some_and(|trees| trees.iter().any(|w| w.path == *t)),
        };
        listed.then(|| (project.clone(), tree.clone()))
    }

    fn remember_space(&mut self, to: &Space, cx: &mut Context<Self>) {
        self.store.sidebar.space = to.host().map(str::to_string);
        self.save_soon(cx);
    }

    pub(crate) fn step_space(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.space_order();
        if let Some(to) = step(&order, &self.space(), by) {
            self.switch_space(to, window, cx);
        }
    }

    pub(crate) fn switch_to_nth_space(&mut self, a: &SwitchSpace, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            return;
        }
        if let Some(to) = nth(&self.space_order(), a.0) {
            self.switch_space(to, window, cx);
        }
    }

    /// A two-finger swipe over the sidebar.
    pub(crate) fn swipe_spaces(&mut self, e: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ScrollDelta::Pixels(delta) = e.delta else { return };
        if self.overlay.is_some() {
            return;
        }
        if let Some(by) = self.spaces.swipe.scroll(delta.x.into(), delta.y.into(), e.touch_phase == TouchPhase::Started) {
            self.step_space(by, window, cx);
        }
    }
}

/// The folders in `projects` the user never added: those only a terminal brought.
fn unadded(projects: Vec<String>, added: &[String]) -> Vec<String> {
    projects.into_iter().filter(|p| !added.contains(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        FLASH, GRACE, HOST_ICONS, Heading, Reach, SLIDE, Space, Spaces, Swipe, Tracked, arrival, colour, crumbs, flash_key, glyph, heading, in_space,
        kept_name, labelled, nth, offline_note, offline_title, on_host, order, reach, resolve, shows_session, space_of, step, twin, unadded, waiting,
    };
    use agents::hosts::{Failure, Link};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use theme::ACCENT_SWATCHES;

    fn projects() -> Vec<String> {
        ["/Users/me/code/app-ios", "/Users/me/code/app-android", "//devbox/home/dev/code/app-ios", "//devbox/home/dev/code/api", "//build/src/app-ios"]
            .map(String::from)
            .to_vec()
    }

    fn host(id: &str) -> Space {
        Space::Host(id.into())
    }

    fn spaces() -> Vec<Space> {
        order(["devbox", "build"])
    }

    #[test]
    fn a_project_is_in_the_space_of_its_machine() {
        assert_eq!([space_of("/Users/me/code/api"), space_of("//devbox/home/dev/code/api")], [Space::Local, host("devbox")]);
        assert_eq!(in_space(&projects(), &host("devbox")), ["//devbox/home/dev/code/app-ios", "//devbox/home/dev/code/api"]);
        assert_eq!(in_space(&projects(), &Space::Local), ["/Users/me/code/app-ios", "/Users/me/code/app-android"]);
    }

    #[test]
    fn a_project_on_two_machines_belongs_to_both_spaces_as_two_keys() {
        let both = ["/Users/me/code/app-ios".to_string(), "//devbox/home/dev/code/app-ios".to_string()];
        assert_eq!(in_space(&both, &Space::Local), [both[0].clone()]);
        assert_eq!(in_space(&both, &host("devbox")), [both[1].clone()]);
    }

    #[test]
    fn this_mac_comes_first_then_the_hosts_in_settings_order() {
        assert_eq!(spaces(), [Space::Local, host("devbox"), host("build")]);
        assert_eq!(order([]), [Space::Local]);
    }

    #[test]
    fn a_removed_hosts_saved_space_falls_back_to_this_mac_and_a_host_not_yet_listed_comes_back() {
        assert_eq!(resolve(&host("devbox"), &spaces()), host("devbox"));
        assert_eq!(resolve(&host("gone"), &spaces()), Space::Local);
        let launching = Spaces::new(Some("devbox"));
        assert_eq!(launching.current(&order([])), Space::Local);
        assert_eq!(launching.current(&spaces()), host("devbox"));
    }

    #[test]
    fn waiting_counts_the_sessions_of_each_space_alone() {
        let projects = ["/Users/me/code/app-ios", "//devbox/home/dev/code/api", "//devbox/home/dev/code/api", "//build/src/app-ios"];
        assert_eq!([Space::Local, host("devbox"), host("build"), host("none")].map(|s| waiting(projects, &s)), [1, 2, 1, 0]);
    }

    #[test]
    fn stepping_stops_at_either_end_of_the_switcher() {
        let order = spaces();
        assert_eq!(step(&order, &Space::Local, 1), Some(host("devbox")));
        assert_eq!(step(&order, &host("build"), -1), Some(host("devbox")));
        assert_eq!([step(&order, &Space::Local, -1), step(&order, &host("build"), 1)], [None, None]);
        assert_eq!(step(&order, &host("gone"), 1), None);
    }

    #[test]
    fn ctrl_9_past_the_last_space_does_nothing_and_ctrl_1_is_this_mac() {
        let order = spaces();
        assert_eq!(
            [nth(&order, 1), nth(&order, 3), nth(&order, 4), nth(&order, 9), nth(&order, 0)],
            [Some(Space::Local), Some(host("build")), None, None, None]
        );
    }

    #[test]
    fn the_twin_is_the_project_of_the_same_folder_name_on_another_machine() {
        let projects = projects();
        assert_eq!(twin("/Users/me/code/app-ios", &host("devbox"), &projects).map(String::as_str), Some("//devbox/home/dev/code/app-ios"));
        assert_eq!(twin("//build/src/app-ios", &Space::Local, &projects).map(String::as_str), Some("/Users/me/code/app-ios"));
        assert_eq!(twin("/Users/me/code/app-android", &host("devbox"), &projects), None);
    }

    #[test]
    fn a_host_takes_its_saved_colour_else_the_palettes_by_position_and_this_mac_has_none() {
        let saved: HashMap<String, u32> = [("build".to_string(), 0x123456)].into();
        assert_eq!(colour(&host("build"), &saved, 2), Some(0x123456));
        assert_eq!(colour(&host("devbox"), &saved, 1), Some(ACCENT_SWATCHES[0]));
        assert_eq!(colour(&host("tenth"), &saved, ACCENT_SWATCHES.len() + 1), Some(ACCENT_SWATCHES[0]));
        assert_eq!(colour(&Space::Local, &saved, 0), None);
    }

    #[test]
    fn a_host_takes_its_saved_icon_else_the_server_and_this_mac_keeps_its_own() {
        let saved: HashMap<String, String> = [("build".to_string(), "cloud".to_string()), ("old".to_string(), "rocket".to_string())].into();
        assert_eq!(glyph(&host("build"), &saved), "cloud");
        assert_eq!(glyph(&host("devbox"), &saved), HOST_ICONS[0]);
        assert_eq!(glyph(&host("old"), &saved), HOST_ICONS[0]);
        assert_eq!(glyph(&Space::Local, &saved), crate::util::LOCAL_ICON);
    }

    #[test]
    fn a_blank_rename_or_the_added_name_goes_back_to_the_added_name() {
        assert_eq!(kept_name("  GPU box ", "devbox"), Some("GPU box".to_string()));
        assert_eq!(kept_name("   ", "devbox"), None);
        assert_eq!(kept_name("devbox", "devbox"), None);
    }

    #[test]
    fn a_project_on_a_host_is_named_with_it() {
        assert_eq!([labelled("api", Some("devbox")), labelled("api", None)], ["api on devbox", "api"]);
    }

    #[test]
    fn an_action_on_a_host_names_it_and_one_on_this_mac_does_not() {
        assert_eq!([on_host("Open folder", Some("devbox")), on_host("Open folder", None)], ["Open folder on devbox\u{2026}", "Open folder\u{2026}"]);
    }

    #[test]
    fn a_page_on_a_host_names_it_first_and_one_on_this_mac_does_not() {
        let page = || ["api".to_string(), "New session".to_string()];
        assert_eq!(crumbs(Some("devbox".into()), page()), ["devbox", "api", "New session"]);
        assert_eq!(crumbs(None, page()), ["api", "New session"]);
    }

    #[test]
    fn a_host_is_unreachable_until_its_link_is_online_and_this_mac_never_is() {
        let online = Link::Online { version: "1".into(), uptime: 0 };
        let got = [
            reach(&host("a"), Some(&online)),
            reach(&host("a"), Some(&Link::Connecting)),
            reach(&host("a"), Some(&Link::Retrying(Failure::Dropped))),
            reach(&host("a"), None),
            reach(&Space::Local, None),
        ];
        assert_eq!(got, [Reach::Online, Reach::Connecting, Reach::Offline, Reach::Offline, Reach::Online]);
    }

    #[test]
    fn an_offline_card_says_what_the_link_is_doing() {
        assert_eq!(
            [offline_title(Reach::Offline, "devbox"), offline_title(Reach::Connecting, "devbox")],
            ["devbox is offline", "devbox is connecting\u{2026}"]
        );
        assert_eq!(offline_note(Some(&Link::Connecting), "devbox"), "Signing in over SSH.");
        assert!(offline_note(Some(&Link::Retrying(Failure::TimedOut)), "devbox").starts_with("Lost the connection."));
        assert!(offline_note(Some(&Link::Failed(Failure::Auth)), "devbox").ends_with("Agents there keep going."));
    }

    #[test]
    fn a_space_slides_in_from_the_side_it_lies_on() {
        let order = spaces();
        assert_eq!([heading(&order, &Space::Local, &host("build")), heading(&order, &host("build"), &host("devbox"))], [Heading::Forward, Heading::Back]);
    }

    #[test]
    fn going_to_a_space_slides_once_and_going_where_it_is_does_not() {
        let (order, now) = (spaces(), Instant::now());
        let mut s = Spaces::new(None);
        assert!(s.go(host("devbox"), &order, now, None));
        assert_eq!(s.sliding(now), Some((1, Heading::Forward)));
        assert_eq!(s.sliding(now + SLIDE), None);
        assert!(!s.go(host("devbox"), &order, now, None));
        assert!(s.go(Space::Local, &order, now, None));
        assert_eq!(s.sliding(now), Some((2, Heading::Back)));
    }

    #[test]
    fn coming_back_to_a_space_finds_the_session_left_there_unless_it_was_left_without_one() {
        let (order, now) = (spaces(), Instant::now());
        let mut s = Spaces::new(None);
        let fix = ("/Users/me/code/api".to_string(), Some("/Users/me/wt/fix".to_string()));
        let app = ("//devbox/code/app".to_string(), None);
        assert!(s.go(host("devbox"), &order, now, Some(fix.clone())));
        assert!(s.go(Space::Local, &order, now, Some(app.clone())));
        assert_eq!([s.left_in(&Space::Local), s.left_in(&host("devbox")), s.left_in(&host("build"))], [Some(&fix), Some(&app), None]);
        assert!(s.go(host("build"), &order, now, None));
        assert_eq!([s.left_in(&Space::Local), s.left_in(&host("devbox"))], [None, Some(&app)]);
    }

    #[test]
    fn a_swipe_moves_one_space_once_it_is_80px_wide_and_not_again_until_fingers_land_anew() {
        let mut swipe = Swipe::default();
        assert_eq!([swipe.scroll(-30., 2., true), swipe.scroll(-30., 2., false)], [None, None]);
        assert_eq!(swipe.scroll(-30., 2., false), Some(1));
        assert_eq!(swipe.scroll(-200., 0., false), None);
        assert_eq!(swipe.scroll(100., 0., true), Some(-1));
    }

    #[test]
    fn a_flash_lights_one_key_for_a_while_and_the_next_takes_over() {
        let (now, mut s) = (Instant::now(), Spaces::new(None));
        s.flash("//devbox/code/api".into(), now);
        assert!(s.flashed("//devbox/code/api", now) && !s.flashed("//devbox/code/app", now));
        assert!(!s.flashed("//devbox/code/api", now + FLASH));
        s.flash(flash_key(&host("build")), now);
        assert!(s.flashed("space:build", now) && !s.flashed("//devbox/code/api", now));
    }

    #[test]
    fn a_mostly_vertical_scroll_is_not_a_swipe() {
        let mut swipe = Swipe::default();
        assert_eq!([swipe.scroll(-60., 90., true), swipe.scroll(-60., 90., false), swipe.scroll(-60., 90., false)], [None, None, None]);
    }

    #[test]
    fn arriving_in_a_space_shows_the_twin_else_the_first_and_leaves_a_project_already_there() {
        let devbox = Space::Host("devbox".into());
        let projects = vec!["//devbox/code/api".to_string(), "//devbox/code/app".to_string()];
        assert_eq!(arrival(Some("/Users/me/app"), &devbox, &projects), Some("//devbox/code/app".into()));
        assert_eq!(arrival(Some("/Users/me/web"), &devbox, &projects), Some("//devbox/code/api".into()));
        assert_eq!(arrival(None, &devbox, &projects), Some("//devbox/code/api".into()));
        assert_eq!(arrival(Some("//devbox/code/app"), &devbox, &projects), None);
        assert_eq!(arrival(Some("/Users/me/app"), &devbox, &[]), None);
    }

    #[test]
    fn only_a_folder_never_added_is_a_thread_of_its_own() {
        let projects = ["/w/app", "/Users/me/Downloads"].map(String::from).to_vec();
        assert_eq!(unadded(projects, &["/w/app".to_string()]), ["/Users/me/Downloads"]);
    }

    #[test]
    fn a_sign_in_locks_its_space_only_once_it_outlasts_the_grace() {
        let (mut spaces, pc, t0) = (Spaces::new(None), host("pc"), Instant::now());
        assert_eq!(spaces.track(&pc, Reach::Connecting, t0), Some(Tracked::Grace(GRACE)));
        assert!(!spaces.locked(&pc, Reach::Connecting, t0 + GRACE - Duration::from_millis(1)));
        assert_eq!(spaces.track(&pc, Reach::Connecting, t0 + GRACE), None, "the grace starts once");
        assert!(spaces.locked(&pc, Reach::Connecting, t0 + GRACE));
        assert_eq!(spaces.track(&pc, Reach::Online, t0 + GRACE), Some(Tracked::Back));
        assert!(!spaces.locked(&pc, Reach::Online, t0 + GRACE));
        assert_eq!(spaces.track(&pc, Reach::Online, t0 + GRACE), None, "back once");
    }

    #[test]
    fn a_host_that_failed_locks_at_once_and_stays_locked_while_it_reconnects() {
        let (mut spaces, pc, t0) = (Spaces::new(None), host("pc"), Instant::now());
        assert_eq!(spaces.track(&pc, Reach::Offline, t0), None);
        assert!(spaces.locked(&pc, Reach::Offline, t0));
        let soon = t0 + Duration::from_millis(1);
        assert_eq!(spaces.track(&pc, Reach::Connecting, soon), None);
        assert!(spaces.locked(&pc, Reach::Connecting, soon), "a reconnect right after a failure keeps it locked");
    }

    #[test]
    fn a_session_shows_only_in_its_own_machines_space_and_never_while_it_is_locked() {
        let (mac, host) = (Space::Local, Space::Host("h".into()));
        assert!(shows_session(Some(&mac), &mac, false));
        assert!(!shows_session(Some(&mac), &host, false), "this Mac's chat stays out of a host's space");
        assert!(!shows_session(Some(&host), &host, true));
        assert!(shows_session(None, &host, false), "a session whose machine isn't known yet");
    }

    #[test]
    fn each_space_gets_its_own_grace() {
        let (mut spaces, t0) = (Spaces::new(None), Instant::now());
        spaces.track(&host("a"), Reach::Offline, t0);
        assert_eq!(spaces.track(&host("b"), Reach::Connecting, t0 + GRACE), Some(Tracked::Grace(GRACE)));
        assert!(!spaces.locked(&host("b"), Reach::Connecting, t0 + GRACE));
        spaces.track(&Space::Local, Reach::Online, t0 + GRACE);
        assert!(spaces.locked(&host("a"), Reach::Connecting, t0 + GRACE), "a failed, so it stays locked while it reconnects, wherever the sidebar went");
    }
}
