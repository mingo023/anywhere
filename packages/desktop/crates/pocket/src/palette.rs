use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Screen, state};
use crate::desktop::sounds::Cue;
use crate::modals::may_open;
use crate::status::{self, Card, Status};
use crate::util::basename;
use agents::Agents;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;
use store::Sounds;
use theme::*;
use ui::{self, dot};
use workspace::Place;
use workspace::tree::Edge;

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Session(String),
    File(String),
    New,
    Split,
    Next,
    /// `tree` is `None` for the project's main worktree.
    Tree { project: String, tree: Option<String> },
    UpNext,
    Browser,
    Automations,
    PairPhone,
    PhoneAccess,
    Sound(Cue),
}

#[derive(Clone, Debug, PartialEq)]
enum Lead {
    Waiting,
    Status(Status),
    Provider(String),
    File,
    Icon(&'static str),
}

#[derive(Clone)]
pub struct Entry {
    pub pick: Pick,
    lead: Lead,
    title: String,
    detail: String,
    keys: Option<&'static str>,
}

/// The typed text as lowercase words; a leading `>` searches actions only.
#[derive(Debug, PartialEq)]
enum Query {
    All(Vec<String>),
    Actions(Vec<String>),
}

impl Query {
    fn words(&self) -> &[String] {
        match self {
            Query::All(w) | Query::Actions(w) => w,
        }
    }
}

fn query(raw: &str) -> Query {
    let raw = raw.trim_start();
    let words = |s: &str| s.split_whitespace().map(str::to_lowercase).collect();
    match raw.strip_prefix('>') {
        Some(rest) => Query::Actions(words(rest)),
        None => Query::All(words(raw)),
    }
}

/// Every word is part of at least one field, ignoring case.
fn matches(words: &[String], fields: &[&str]) -> bool {
    let fields: Vec<String> = fields.iter().map(|f| f.to_lowercase()).collect();
    words.iter().all(|w| fields.iter().any(|f| f.contains(w.as_str())))
}

/// The byte ranges of `text` the words match, merged. Empty when lowercasing would move byte offsets.
fn runs(text: &str, words: &[String]) -> Vec<Range<usize>> {
    let keeps_width = |c: char| {
        let lower: Vec<char> = c.to_lowercase().collect();
        lower.len() == 1 && lower[0].len_utf8() == c.len_utf8()
    };
    if !text.chars().all(keeps_width) {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let mut found: Vec<Range<usize>> = words.iter().flat_map(|w| lower.match_indices(w.as_str()).map(|(i, m)| i..i + m.len())).collect();
    found.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in found {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

fn marked(text: String, words: &[String]) -> StyledText {
    let marks: Vec<(Range<usize>, HighlightStyle)> = runs(&text, words).into_iter().map(|r| (r, HighlightStyle { color: Some(ACCENT.into()), ..Default::default() })).collect();
    StyledText::new(text).with_highlights(marks)
}

fn hint(keys: &str, label: &str) -> Div {
    div().flex().items_center().gap(px(4.)).child(div().font_weight(FontWeight::MEDIUM).child(keys.to_string())).child(label.to_string())
}

#[derive(Debug, PartialEq)]
enum Nav {
    To(usize),
    Scope,
    Close,
    Open(usize),
}

fn selected(ix: usize, total: usize) -> usize {
    ix.min(total.saturating_sub(1))
}

fn nav(key: &str, ix: usize, total: usize) -> Option<Nav> {
    let (ix, last) = (selected(ix, total), total.saturating_sub(1));
    match key {
        "up" => Some(Nav::To(if ix == 0 { last } else { ix - 1 })),
        "down" => Some(Nav::To(if ix >= last { 0 } else { ix + 1 })),
        "tab" => Some(Nav::Scope),
        "escape" => Some(Nav::Close),
        "enter" => Some(Nav::Open(ix)),
        _ => None,
    }
}

/// The results list's child to scroll to for row `ix`, given each section's row count. Section labels are children too; row 0 scrolls to the first label, so the list shows from its top.
fn row_child(lens: &[usize], ix: usize) -> usize {
    if ix == 0 {
        return 0;
    }
    let mut end = 0;
    let labels = lens.iter().take_while(|len| {
        end += *len;
        end <= ix
    });
    ix + labels.count() + 1
}

/// A session's project, its worktree's branch when known, and the card.
type SessionCard = (String, Option<String>, Card);

fn session_matches(words: &[String], (project, branch, c): &SessionCard) -> bool {
    matches(words, &[&c.title, project, branch.as_deref().unwrap_or_default()])
}

fn session_entry((project, branch, c): SessionCard) -> Entry {
    Entry {
        lead: match c.status {
            Status::Idle => Lead::Provider(c.provider.clone()),
            s => Lead::Status(s),
        },
        detail: [Some(project.as_str()), branch.as_deref(), Some(provider_name(&c.provider)), Some(c.status.label())].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        title: c.title,
        keys: None,
        pick: Pick::Session(c.id),
    }
}

/// Newest session first.
fn session_entries(words: &[String], cards: Vec<SessionCard>) -> Vec<Entry> {
    let mut cards: Vec<SessionCard> = cards.into_iter().filter(|s| session_matches(words, s)).collect();
    cards.sort_by_key(|(_, _, c)| Reverse(c.created));
    cards.into_iter().take(if words.is_empty() { 5 } else { 8 }).map(session_entry).collect()
}

/// The matching sessions in `status::up_next` order, at most five.
fn up_next_entries(words: &[String], cards: &[SessionCard]) -> Vec<Entry> {
    let matched: Vec<&SessionCard> = cards.iter().filter(|s| session_matches(words, s)).collect();
    let plain: Vec<Card> = matched.iter().map(|(_, _, c)| c.clone()).collect();
    status::up_next(&plain)
        .into_iter()
        .take(5)
        .filter_map(|c| matched.iter().find(|(_, _, m)| m.id == c.id))
        .map(|s| session_entry((*s).clone()))
        .collect()
}

/// `trees` holds each worktree with its project's path and name. Only a query lists worktrees.
fn tree_entries(words: &[String], trees: Vec<(String, String, git::Worktree)>) -> Vec<Entry> {
    if words.is_empty() {
        return Vec::new();
    }
    trees
        .into_iter()
        .filter_map(|(project, name, w)| {
            let title = if w.main { name.clone() } else { basename(&w.path) };
            matches(words, &[&title, &name, &w.branch]).then(|| Entry {
                detail: format!("{name} · {}", w.branch),
                pick: Pick::Tree { project, tree: (!w.main).then_some(w.path) },
                lead: Lead::Icon("worktree"),
                title,
                keys: None,
            })
        })
        .take(5)
        .collect()
}

/// `changed` and `files` are relative to `root`.
fn file_entries(words: &[String], root: &str, changed: &[String], files: &[String]) -> Vec<Entry> {
    let is_changed: HashSet<&str> = changed.iter().map(String::as_str).collect();
    let found: Vec<&String> = if words.is_empty() { changed.iter().take(3).collect() } else { files.iter().filter(|p| matches(words, &[p])).collect() };
    let (mut paths, rest): (Vec<&String>, Vec<&String>) = found.into_iter().partition(|p| is_changed.contains(p.as_str()));
    paths.extend(rest);
    paths
        .into_iter()
        .take(8)
        .map(|p| {
            let dir = p.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            let detail = match (dir.is_empty(), is_changed.contains(p.as_str())) {
                (true, true) => "modified".to_string(),
                (false, true) => format!("{dir} · modified"),
                _ => dir,
            };
            Entry { pick: Pick::File(format!("{root}/{p}")), lead: Lead::File, title: basename(p), detail, keys: None }
        })
        .collect()
}

fn action_entries(words: &[String], project: &str, sounds: Sounds, agents: &Agents) -> Vec<Entry> {
    let sound = |cue: Cue| Entry {
        pick: Pick::Sound(cue),
        lead: Lead::Icon("bell"),
        title: format!("{} sound: {}", cue.name(), if cue.on(sounds) { "On" } else { "Off" }),
        detail: String::new(),
        keys: None,
    };
    [
        Entry { pick: Pick::New, lead: Lead::Icon("sparkle"), title: format!("New session in {project}"), detail: String::new(), keys: Some("⌘ N") },
        Entry { pick: Pick::Split, lead: Lead::Icon("split-right"), title: "Open selected in a split".into(), detail: String::new(), keys: None },
        Entry { pick: Pick::Next, lead: Lead::Waiting, title: "Go to next Needs you".into(), detail: String::new(), keys: Some("⌘ J") },
        Entry { pick: Pick::UpNext, lead: Lead::Icon("forward"), title: "Go to Up next".into(), detail: String::new(), keys: Some("⌘ ⇧ J") },
        Entry { pick: Pick::Browser, lead: Lead::Icon("globe"), title: "Open browser".into(), detail: String::new(), keys: Some("⌘ ⇧ B") },
        Entry { pick: Pick::Automations, lead: Lead::Icon("bolt"), title: "Go to Automations".into(), detail: String::new(), keys: Some("⌘ ⇧ A") },
        Entry { pick: Pick::PairPhone, lead: Lead::Icon("shield"), title: "Pair phone…".into(), detail: String::new(), keys: None },
        Entry { pick: Pick::PhoneAccess, lead: Lead::Icon("shield"), title: "Phone access level…".into(), detail: String::new(), keys: None },
    ]
    .into_iter()
    .chain(Cue::ALL.map(sound))
    .filter(|e| match e.pick {
        Pick::New | Pick::Split => may_open(Overlay::NewSession, agents),
        Pick::PairPhone => may_open(Overlay::PairPhone, agents),
        Pick::PhoneAccess => may_open(Overlay::PhoneAccess, agents),
        Pick::Automations => agents.automations_offered(),
        _ => true,
    })
    .filter(|e| matches(words, &[&e.title]))
    .collect()
}

pub struct PaletteState {
    pub(crate) filter: Entity<InputState>,
    pub(crate) ix: usize,
    pub(crate) all: bool,
    pub(crate) files: Vec<String>,
    pub(crate) scroll: ScrollHandle,
}

impl PaletteState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions, files and actions…"));
        let subs = vec![cx.subscribe(&filter, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.palette.ix = 0;
                this.palette.scroll.set_offset(Point::default());
            }
            cx.notify()
        })];
        (Self { filter, ix: 0, all: true, files: Vec::new(), scroll: ScrollHandle::new() }, subs)
    }
}

impl Desktop {
    pub fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.ix = 0;
        self.palette.scroll.set_offset(Point::default());
        self.palette.filter.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        let Some(root) = self.explore_root() else { return };
        let task = cx.background_executor().spawn(async move { git::ls_files(&root) });
        cx.spawn(async move |this, cx| {
            let files = task.await;
            this.update(cx, |d, cx| {
                d.palette.files = files;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn palette_sections(&self, cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
        let q = query(&self.palette.filter.read(cx).value());
        let words = q.words();
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let actions = action_entries(words, &project, self.store.sounds, &self.agents);
        let groups = match q {
            Query::Actions(_) => vec![("Actions", actions)],
            Query::All(_) => {
                let scope = if self.palette.all { self.projects() } else { self.project.iter().cloned().collect() };
                let cards: Vec<SessionCard> = scope
                    .iter()
                    .flat_map(|p| {
                        let name = self.repo_name(p);
                        self.cards(p).into_iter().map(move |c| (name.clone(), self.repos.get(&c.cwd).map(|r| r.branch.clone()), c))
                    })
                    .collect();
                let trees = scope
                    .iter()
                    .filter_map(|p| Some((p, self.repo_name(p), self.listed_trees(p)?)))
                    .flat_map(|(p, name, trees)| trees.into_iter().map(move |w| (p.clone(), name.clone(), w)))
                    .collect();
                let root = self.explore_root().unwrap_or_default();
                let changed: Vec<String> = self.repos.get(&root).map(|r| r.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default();
                vec![
                    ("Up next", up_next_entries(words, &cards)),
                    ("Sessions", session_entries(words, cards)),
                    ("Worktrees", tree_entries(words, trees)),
                    ("Files", file_entries(words, &root, &changed, &self.palette.files)),
                    ("Actions", actions),
                ]
            }
        };
        groups.into_iter().filter(|(_, e)| !e.is_empty()).collect()
    }

    fn activate(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        match pick {
            Pick::Session(id) => self.focus_agent(&id, window, cx),
            Pick::File(path) => {
                self.screen = Screen::Sessions;
                self.open_file(path, true, cx);
            }
            Pick::New => self.open(Overlay::NewSession, window, cx),
            Pick::Split => self.new_shell(Place::Split(self.focused_pane(), Edge::Right), cx),
            Pick::Next => self.next_needs_you(&crate::actions::NextNeedsYou, window, cx),
            Pick::Tree { project, tree } => {
                if self.projects().contains(&project) {
                    self.select_tree(project, tree, cx);
                }
            }
            Pick::UpNext => self.go_to_up_next(&crate::actions::GoToUpNext, window, cx),
            Pick::Browser => self.open_browser(None, window, cx),
            Pick::Automations => self.open_automations(&crate::actions::OpenAutomations, window, cx),
            Pick::PairPhone => self.open(Overlay::PairPhone, window, cx),
            Pick::PhoneAccess => self.open(Overlay::PhoneAccess, window, cx),
            Pick::Sound(cue) => {
                cue.flip(&mut self.store.sounds);
                self.store.save();
            }
        }
        cx.notify();
    }

    fn on_palette_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let sections = self.palette_sections(cx);
        let lens: Vec<usize> = sections.iter().map(|(_, e)| e.len()).collect();
        let entries: Vec<Entry> = sections.into_iter().flat_map(|(_, e)| e).collect();
        match nav(ev.keystroke.key.as_str(), self.palette.ix, entries.len()) {
            Some(Nav::To(ix)) => {
                self.palette.ix = ix;
                self.palette.scroll.scroll_to_item(row_child(&lens, ix));
            }
            Some(Nav::Scope) => {
                self.palette.all = !self.palette.all;
                self.palette.ix = 0;
                self.palette.scroll.scroll_to_item(0);
            }
            Some(Nav::Close) => self.close_overlay(window, cx),
            Some(Nav::Open(ix)) => {
                if let Some(e) = entries.get(ix) {
                    self.activate(e.pick.clone(), window, cx);
                }
            }
            None => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn palette(&mut self, cx: &mut Context<Self>) -> Div {
        let sections = self.palette_sections(cx);
        let total: usize = sections.iter().map(|(_, e)| e.len()).sum();
        let selected = selected(self.palette.ix, total);
        let q = query(&self.palette.filter.read(cx).value());
        let words = q.words();
        let mut n = 0;
        let mut body = div().id("palette-results").max_h(px(460.)).overflow_y_scroll().track_scroll(&self.palette.scroll).px(px(8.)).pb(px(8.)).flex().flex_col();
        for (label, entries) in sections {
            body = body.child(div().pt(px(12.)).pb(px(6.)).px(px(12.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(label));
            for e in entries {
                let i = n;
                n += 1;
                let lead = match &e.lead {
                    Lead::Waiting => dot(8., WAITING).into_any_element(),
                    Lead::Status(s) => ui::indicator(("palette-status", i), Some(state(*s, 0, 0))).unwrap_or_else(|| div().into_any_element()),
                    Lead::Provider(p) => provider_icon(p, 13., TEXT_2).into_any_element(),
                    Lead::File => file_icon(&e.title, false, false, 16.).into_any_element(),
                    Lead::Icon(name) => icon(name, 13., TEXT_2).into_any_element(),
                };
                let keys = e.keys.or((i == selected).then_some("↵"));
                let pick = e.pick.clone();
                let (title, detail) = (marked(e.title, words), marked(e.detail, words));
                body = body.child(
                    ui::palette_row(("palette-row", i), i == selected, lead, title, detail, keys)
                        .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                            if this.palette.ix != i {
                                this.palette.ix = i;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.activate(pick.clone(), window, cx))),
                );
            }
        }
        if total == 0 {
            body = body.child(
                div()
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .text_color(TEXT_2)
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("No matches."))
                    .child(div().text_size(px(12.5)).child("Try a session title, project, worktree or branch.")),
            );
        }
        let input = div()
            .h(px(60.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("search", 17., TEXT_3))
            .child(div().flex_1().text_size(px(17.)).child(Input::new(&self.palette.filter).appearance(false).p_0().text_size(px(17.))))
            .child(div().text_size(px(12.)).text_color(TEXT_2).child("esc"));
        let scope = if self.palette.all { "Tab to filter by project" } else { "Tab to search all projects" };
        let footer = div()
            .h(px(40.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .text_size(px(11.5))
            .text_color(TEXT_2)
            .child(hint("↑↓", "Navigate"))
            .child(hint("↵", "Open"))
            .child(div().ml_auto().child(scope))
            .child(hint("esc", "Close"));
        div().absolute().top(px(120.)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(660.)).overflow_hidden().flex().flex_col())
                .rounded(px(R_DIALOG))
                .occlude()
                .capture_key_down(cx.listener(Self::on_palette_key))
                .child(input)
                .child(body)
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Lead, Nav, Pick, Query, action_entries, file_entries, matches, nav, query, row_child, runs, session_entries, tree_entries, up_next_entries};
    use crate::desktop::sounds::Cue;
    use crate::status::{Card, Kind, Status};
    use agents::{Agents, Event};
    use store::Sounds;

    fn card(id: &str, title: &str, status: Status, at: i64) -> (String, Option<String>, Card) {
        let c = Card { id: id.into(), provider: "claude".into(), model: String::new(), title: title.into(), cwd: String::new(), at, created: at, status, kind: Kind::Agent, notice: None, pinned: false };
        ("app".into(), Some("main".into()), c)
    }

    fn titles(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.title.as_str()).collect()
    }

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn tree(path: &str, branch: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: branch.into(), main }
    }

    #[test]
    fn every_word_must_match_some_field() {
        assert!(matches(&strings(&["fix", "main"]), &["Fix login", "app", "main"]));
        assert!(!matches(&strings(&["fix", "docs"]), &["Fix login", "app", "main"]));
        assert!(matches(&[], &["anything"]));
    }

    #[test]
    fn a_greater_than_prefix_keeps_only_actions() {
        assert_eq!(query("  Fix  Login "), Query::All(strings(&["fix", "login"])));
        assert_eq!(query(">new"), Query::Actions(strings(&["new"])));
        assert_eq!(query(">"), Query::Actions(vec![]));
    }

    #[test]
    fn runs_merge_overlapping_matches() {
        assert_eq!(runs("Fix build fix", &strings(&["fix", "ix b"])), vec![0..5, 10..13]);
        assert!(runs("abc", &strings(&["x"])).is_empty());
    }

    #[test]
    fn runs_are_empty_when_lowercasing_changes_length() {
        assert!(runs("İstanbul", &strings(&["stan"])).is_empty());
        assert!(matches(&strings(&["stan"]), &["İstanbul"]));
    }

    #[test]
    fn empty_query_shows_five_sessions_and_three_files() {
        let cards = (0..7).map(|i| card(&format!("s{i}"), &format!("Task {i}"), Status::Idle, i)).collect();
        assert_eq!(session_entries(&[], cards).len(), 5);
        let changed = strings(&["a.rs", "b.rs", "c.rs", "d.rs"]);
        assert_eq!(file_entries(&[], "/w", &changed, &changed).len(), 3);
        assert!(tree_entries(&[], vec![("/w/app".into(), "app".into(), tree("/w/app", "main", true))]).is_empty());
    }

    #[test]
    fn groups_cap_at_8_5_8() {
        let q = strings(&["fix"]);
        let cards = (0..10).map(|i| card(&format!("s{i}"), &format!("Fix {i}"), Status::Idle, i)).collect();
        assert_eq!(session_entries(&q, cards).len(), 8);
        let trees = (0..7).map(|i| ("/w/app".into(), "app".into(), tree(&format!("/w/fix-{i}"), &format!("fix-{i}"), false))).collect();
        assert_eq!(tree_entries(&q, trees).len(), 5);
        let files: Vec<String> = (0..10).map(|i| format!("src/fix_{i}.rs")).collect();
        assert_eq!(file_entries(&q, "/w", &[], &files).len(), 8);
    }

    #[test]
    fn a_status_change_keeps_the_session_order() {
        let cards = |s: [Status; 3]| vec![card("a", "Fix CI", s[0], 1), card("b", "Fix login", s[1], 3), card("c", "Fix lint", s[2], 2)];
        let fix = strings(&["fix"]);
        let before = session_entries(&fix, cards([Status::Working, Status::Idle, Status::Working]));
        let after = session_entries(&fix, cards([Status::Done, Status::NeedsYou, Status::Failed]));
        assert_eq!(titles(&before), vec!["Fix login", "Fix lint", "Fix CI"]);
        assert_eq!(titles(&after), titles(&before));
    }

    #[test]
    fn up_next_lists_five_matching_sessions_that_want_a_look() {
        let cards = [
            card("a", "Fix CI", Status::Done, 1),
            card("b", "Fix login", Status::NeedsYou, 5),
            card("c", "Fix docs", Status::Working, 2),
            card("d", "Fix lint", Status::Failed, 4),
            card("e", "Fix build", Status::NeedsYou, 6),
            card("f", "Fix typo", Status::Done, 3),
            card("g", "Fix tests", Status::Done, 7),
            card("h", "Docs", Status::NeedsYou, 0),
        ];
        assert_eq!(titles(&up_next_entries(&strings(&["fix"]), &cards)), vec!["Fix login", "Fix build", "Fix lint", "Fix CI", "Fix typo"]);
        assert_eq!(titles(&up_next_entries(&[], &cards)), vec!["Docs", "Fix login", "Fix build", "Fix lint", "Fix CI"]);
    }

    #[test]
    fn a_session_opens_its_agent_and_reads_its_project_branch_provider_and_status() {
        let mut detached = card("a3", "Lint", Status::Done, 1);
        detached.1 = None;
        let got = session_entries(
            &[],
            vec![card("a1", "Fix CI", Status::NeedsYou, 3), card("a2", "Docs", Status::Working, 2), detached, card("a4", "Build", Status::Failed, 0), card("a5", "Docs2", Status::Idle, -1)],
        );
        let got: Vec<_> = got.into_iter().map(|e| (e.pick, e.lead, e.detail)).collect();
        assert_eq!(
            got,
            vec![
                (Pick::Session("a1".into()), Lead::Status(Status::NeedsYou), "app · main · Claude Code · Needs you".to_string()),
                (Pick::Session("a2".into()), Lead::Status(Status::Working), "app · main · Claude Code · Working".to_string()),
                (Pick::Session("a3".into()), Lead::Status(Status::Done), "app · Claude Code · Done".to_string()),
                (Pick::Session("a4".into()), Lead::Status(Status::Failed), "app · main · Claude Code · Failed".to_string()),
                (Pick::Session("a5".into()), Lead::Provider("claude".into()), "app · main · Claude Code · Idle".to_string()),
            ]
        );
    }

    #[test]
    fn sessions_match_on_project_and_branch_too() {
        let got = session_entries(&strings(&["app", "main"]), vec![card("a1", "Fix CI", Status::Idle, 1)]);
        assert_eq!(titles(&got), vec!["Fix CI"]);
    }

    #[test]
    fn a_worktree_opens_in_its_project_and_the_main_one_reads_as_the_project() {
        let trees = vec![("/w/app".into(), "app".into(), tree("/w/app", "main", true)), ("/w/app".into(), "app".into(), tree("/w/app-login", "login", false))];
        let got: Vec<_> = tree_entries(&strings(&["app"]), trees).into_iter().map(|e| (e.pick, e.title, e.detail)).collect();
        assert_eq!(
            got,
            vec![
                (Pick::Tree { project: "/w/app".into(), tree: None }, "app".to_string(), "app · main".to_string()),
                (Pick::Tree { project: "/w/app".into(), tree: Some("/w/app-login".into()) }, "app-login".to_string(), "app · login".to_string()),
            ]
        );
    }

    #[test]
    fn without_a_query_files_are_the_first_three_changed_ones() {
        let changed = strings(&["README.md", "src/a.rs", "src/b.rs", "src/c.rs"]);
        let got: Vec<_> = file_entries(&[], "/w", &changed, &strings(&["x.rs"])).into_iter().map(|e| (e.pick, e.title, e.detail)).collect();
        assert_eq!(
            got,
            vec![
                (Pick::File("/w/README.md".into()), "README.md".to_string(), "modified".to_string()),
                (Pick::File("/w/src/a.rs".into()), "a.rs".to_string(), "src · modified".to_string()),
                (Pick::File("/w/src/b.rs".into()), "b.rs".to_string(), "src · modified".to_string()),
            ]
        );
    }

    #[test]
    fn a_query_searches_every_file_and_lists_changed_ones_first() {
        let files = strings(&["Main.rs", "src/app.rs", "src/main.rs", "docs/main.md", "notes.txt", "a/main.c", "b/main.h", "c/main.py"]);
        let got: Vec<_> = file_entries(&strings(&["main"]), "/w", &strings(&["src/main.rs", "notes.txt"]), &files).into_iter().map(|e| (e.title, e.detail)).collect();
        let want = [("main.rs", "src · modified"), ("Main.rs", ""), ("main.md", "docs"), ("main.c", "a"), ("main.h", "b"), ("main.py", "c")];
        assert_eq!(got, want.map(|(t, d)| (t.to_string(), d.to_string())));
    }

    fn offering_automations() -> Agents {
        let mut a = Agents::default();
        a.apply(Event::Connected { scopes: vec![], caps: vec!["automations.v1".into()], version: String::new() });
        a
    }

    #[test]
    fn automations_is_listed_only_once_pocketd_offers_it() {
        let listed = |a: &Agents| action_entries(&[], "app", Sounds::default(), a).iter().any(|e| e.pick == Pick::Automations);
        assert!(!listed(&Agents::default()));
        assert!(listed(&offering_automations()));
    }

    #[test]
    fn actions_match_on_their_titles() {
        let all: Vec<_> = action_entries(&[], "app", Sounds::default(), &offering_automations()).into_iter().map(|e| (e.pick, e.title)).collect();
        let want = [
            (Pick::New, "New session in app"),
            (Pick::Split, "Open selected in a split"),
            (Pick::Next, "Go to next Needs you"),
            (Pick::UpNext, "Go to Up next"),
            (Pick::Browser, "Open browser"),
            (Pick::Automations, "Go to Automations"),
            (Pick::Sound(Cue::NeedsYou), "Needs you sound: On"),
            (Pick::Sound(Cue::Done), "Done sound: On"),
            (Pick::Sound(Cue::Failed), "Failed sound: On"),
        ];
        assert_eq!(all, want.map(|(p, t)| (p, t.to_string())));
        let session: Vec<_> = action_entries(&strings(&["session"]), "app", Sounds::default(), &offering_automations()).into_iter().map(|e| e.pick).collect();
        assert_eq!(session, vec![Pick::New]);
    }

    #[test]
    fn pair_phone_is_offered_only_to_the_owner() {
        let picks = |scopes: &[&str]| {
            let mut a = Agents::default();
            a.apply(Event::Connected { scopes: scopes.iter().map(|s| s.to_string()).collect(), caps: vec!["pair.v1".into(), "automations.v1".into()], version: String::new() });
            action_entries(&[], "app", Sounds::default(), &a).into_iter().map(|e| e.pick).filter(|p| !matches!(p, Pick::Sound(_))).collect::<Vec<_>>()
        };
        let owner = ["observe", "drive", "approve", "spawn", "owner"];
        assert_eq!(picks(&owner), vec![Pick::New, Pick::Split, Pick::Next, Pick::UpNext, Pick::Browser, Pick::Automations, Pick::PairPhone, Pick::PhoneAccess]);
        assert_eq!(picks(&["observe"]), vec![Pick::Next, Pick::UpNext, Pick::Browser, Pick::Automations]);
        assert_eq!(picks(&[]), vec![Pick::New, Pick::Split, Pick::Next, Pick::UpNext, Pick::Browser, Pick::Automations]);
    }

    #[test]
    fn phone_access_is_offered_only_to_the_owner() {
        let picks = |scopes: &[&str]| {
            let mut a = Agents::default();
            a.apply(Event::Connected { scopes: scopes.iter().map(|s| s.to_string()).collect(), caps: vec!["pair.v1".into()], version: String::new() });
            action_entries(&["phone".to_string()], "app", Sounds::default(), &a).into_iter().map(|e| e.pick).collect::<Vec<_>>()
        };
        assert_eq!(picks(&["observe", "drive", "approve", "spawn", "owner"]), vec![Pick::PairPhone, Pick::PhoneAccess]);
        assert_eq!(picks(&["observe"]), vec![]);
    }

    #[test]
    fn a_sound_toggle_shows_its_state_and_flips_it() {
        let mut sounds = Sounds::default();
        Cue::NeedsYou.flip(&mut sounds);
        let got: Vec<_> = action_entries(&strings(&["sound"]), "app", sounds, &Agents::default()).into_iter().map(|e| e.title).collect();
        assert_eq!(got, vec!["Needs you sound: Off", "Done sound: On", "Failed sound: On"]);
    }

    #[test]
    fn arrow_keys_wrap() {
        let moves: Vec<_> = [("down", 0), ("down", 2), ("up", 1), ("up", 0)].into_iter().map(|(key, ix)| nav(key, ix, 3)).collect();
        assert_eq!(moves, vec![Some(Nav::To(1)), Some(Nav::To(0)), Some(Nav::To(0)), Some(Nav::To(2))]);
    }

    #[test]
    fn a_row_scrolls_into_view_past_the_section_labels_above_it() {
        let got: Vec<_> = (0..5).map(|ix| row_child(&[2, 3], ix)).collect();
        assert_eq!(got, vec![0, 2, 4, 5, 6]);
    }

    #[test]
    fn enter_opens_the_selection_even_after_the_list_shrank() {
        assert_eq!(nav("enter", 1, 3), Some(Nav::Open(1)));
        assert_eq!(nav("enter", 7, 3), Some(Nav::Open(2)));
    }

    #[test]
    fn tab_switches_scope_escape_closes_and_other_keys_reach_the_input() {
        let got: Vec<_> = ["tab", "escape", "a", "left"].into_iter().map(|key| nav(key, 1, 3)).collect();
        assert_eq!(got, vec![Some(Nav::Scope), Some(Nav::Close), None, None]);
    }
}
