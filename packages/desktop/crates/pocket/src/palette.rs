use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Screen};
use crate::status::{Card, Status};
use crate::util::basename;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use std::cmp::Reverse;
use theme::*;
use ui::{self, dot};

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Session(String),
    File(String),
    New,
    Split,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
enum Lead {
    Waiting,
    Running,
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

fn status_word(s: Status) -> &'static str {
    match s {
        Status::NeedsYou => "needs you",
        Status::Failed => "failed",
        Status::Done => "done",
        Status::Working => "working",
        Status::Idle => "idle",
    }
}

fn hint(keys: &str, label: &str) -> Div {
    div().flex().items_center().gap(px(4.)).child(div().text_color(rgba(TEXT_3)).child(keys.to_string())).child(label.to_string())
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
    let last = total.saturating_sub(1);
    match key {
        "up" => Some(Nav::To(ix.saturating_sub(1))),
        "down" => Some(Nav::To((ix + 1).min(last))),
        "tab" => Some(Nav::Scope),
        "escape" => Some(Nav::Close),
        "enter" => Some(Nav::Open(selected(ix, total))),
        _ => None,
    }
}

/// `q` is already lowercase.
fn hit(q: &str, s: &str) -> bool {
    q.is_empty() || s.to_lowercase().contains(q)
}

/// `cards` pairs each card with its project's name.
fn session_entries(q: &str, cards: Vec<(String, Card)>) -> Vec<Entry> {
    let mut cards: Vec<(String, Card)> = cards.into_iter().filter(|(_, c)| hit(q, &c.title)).collect();
    cards.sort_by_key(|(_, c)| (c.status, Reverse(c.at)));
    cards
        .into_iter()
        .take(5)
        .map(|(name, c)| Entry {
            lead: match c.status {
                Status::NeedsYou => Lead::Waiting,
                Status::Working => Lead::Running,
                _ => Lead::Provider(c.provider.clone()),
            },
            detail: format!("{name} · {} · {}", provider_name(&c.provider), status_word(c.status)),
            title: c.title,
            keys: None,
            pick: Pick::Session(c.id),
        })
        .collect()
}

/// `changed` and `files` are relative to `root`.
fn file_entries(q: &str, root: &str, changed: &[String], files: &[String]) -> Vec<Entry> {
    let mut paths: Vec<&String> = if q.is_empty() { changed.iter().take(3).collect() } else { files.iter().filter(|p| hit(q, p)).collect() };
    paths.sort_by_key(|p| !changed.contains(p));
    paths
        .into_iter()
        .take(5)
        .map(|p| {
            let dir = p.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            let detail = match (dir.is_empty(), changed.contains(p)) {
                (true, true) => "modified".to_string(),
                (false, true) => format!("{dir} · modified"),
                _ => dir,
            };
            Entry { pick: Pick::File(format!("{root}/{p}")), lead: Lead::File, title: basename(p), detail, keys: None }
        })
        .collect()
}

fn action_entries(q: &str, project: &str) -> Vec<Entry> {
    [
        Entry { pick: Pick::New, lead: Lead::Icon("sparkle"), title: format!("New session in {project}"), detail: String::new(), keys: Some("⌘ N") },
        Entry { pick: Pick::Split, lead: Lead::Icon("split-right"), title: "Open selected in a split".into(), detail: String::new(), keys: None },
        Entry { pick: Pick::Next, lead: Lead::Waiting, title: "Jump to next waiting session".into(), detail: String::new(), keys: Some("⌘ J") },
    ]
    .into_iter()
    .filter(|e| hit(q, &e.title))
    .collect()
}

pub struct PaletteState {
    pub(crate) filter: Entity<InputState>,
    pub(crate) ix: usize,
    pub(crate) all: bool,
    pub(crate) files: Vec<String>,
}

impl PaletteState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions, files and actions…"));
        let subs = vec![cx.subscribe(&filter, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.palette.ix = 0;
            }
            cx.notify()
        })];
        (Self { filter, ix: 0, all: true, files: Vec::new() }, subs)
    }
}

impl Desktop {
    pub fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.ix = 0;
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
        let q = self.palette.filter.read(cx).value().to_lowercase();
        let projects = if self.palette.all { self.projects() } else { self.project.iter().cloned().collect() };
        let cards: Vec<(String, Card)> = projects
            .iter()
            .flat_map(|p| {
                let name = self.repo_name(p);
                self.cards(p).into_iter().map(move |c| (name.clone(), c))
            })
            .collect();
        let sessions = session_entries(&q, cards);
        let root = self.explore_root().unwrap_or_default();
        let changed: Vec<String> = self.repos.get(&root).map(|r| r.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default();
        let files = file_entries(&q, &root, &changed, &self.palette.files);
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let actions = action_entries(&q, &project);
        [("Sessions", sessions), ("Files", files), ("Actions", actions)].into_iter().filter(|(_, e)| !e.is_empty()).collect()
    }

    fn activate(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        match pick {
            Pick::Session(id) => self.focus_agent(&id, window, cx),
            Pick::File(path) => {
                self.screen = Screen::Sessions;
                self.open_file(path, cx);
            }
            Pick::New => self.open(Overlay::NewSession, window, cx),
            Pick::Split => self.new_shell(Some(false), cx),
            Pick::Next => self.next_waiting(&crate::actions::NextWaiting, window, cx),
        }
        cx.notify();
    }

    fn on_palette_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let entries: Vec<Entry> = self.palette_sections(cx).into_iter().flat_map(|(_, e)| e).collect();
        match nav(ev.keystroke.key.as_str(), self.palette.ix, entries.len()) {
            Some(Nav::To(ix)) => self.palette.ix = ix,
            Some(Nav::Scope) => {
                self.palette.all = !self.palette.all;
                self.palette.ix = 0;
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
        let mut n = 0;
        let mut body = div().id("palette-results").max_h(px(460.)).overflow_y_scroll().px(px(8.)).pb(px(8.)).flex().flex_col();
        for (label, entries) in sections {
            body = body.child(div().pt(px(12.)).pb(px(6.)).px(px(12.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_4)).child(label));
            for e in entries {
                let i = n;
                n += 1;
                let lead = match &e.lead {
                    Lead::Waiting => dot(8., WAITING).into_any_element(),
                    Lead::Running => spinner(("palette-spin", i), 12., RUNNING_TEXT).into_any_element(),
                    Lead::Provider(p) => dot(8., provider_color(p)).into_any_element(),
                    Lead::File => file_icon(&e.title, false, false, 16.).into_any_element(),
                    Lead::Icon(name) => icon(name, 13., TEXT_2).into_any_element(),
                };
                let keys = e.keys.or((i == selected).then_some("↵"));
                let pick = e.pick.clone();
                body = body.child(ui::palette_row(("palette-row", i), i == selected, lead, e.title, e.detail, keys).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| this.activate(pick.clone(), window, cx),
                )));
            }
        }
        if total == 0 {
            body = body.child(div().p(px(20.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child("No matches."));
        }
        let input = div()
            .h(px(60.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .border_b(px(0.5))
            .border_color(rgba(SEPARATOR))
            .child(icon("search", 17., TEXT_3))
            .child(div().flex_1().text_size(px(17.)).child(Input::new(&self.palette.filter).appearance(false).p_0().text_size(px(17.))))
            .child(div().text_size(px(12.)).text_color(rgba(TEXT_4)).child("esc"));
        let scope = if self.palette.all { "Tab to filter by project" } else { "Tab to search all projects" };
        let footer = div()
            .h(px(40.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .text_size(px(12.))
            .text_color(rgba(TEXT_4))
            .child(hint("↑↓", "navigate"))
            .child(hint("↵", "open"))
            .child(div().ml_auto().child(scope));
        div().absolute().top(px(120.)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(660.)).rounded(px(22.)).overflow_hidden().flex().flex_col())
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
    use super::{Entry, Lead, Nav, Pick, action_entries, file_entries, nav, session_entries};
    use crate::status::{Card, Kind, Status};

    fn card(id: &str, title: &str, status: Status, at: i64) -> (String, Card) {
        let c = Card { id: id.into(), provider: "claude".into(), title: title.into(), cwd: String::new(), at, status, kind: Kind::Agent };
        ("app".into(), c)
    }

    fn titles(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.title.as_str()).collect()
    }

    #[test]
    fn sessions_show_the_five_most_urgent_then_latest_matches() {
        let cards = vec![
            card("a", "Fix login", Status::Idle, 9),
            card("b", "Fix CI", Status::NeedsYou, 1),
            card("c", "Docs", Status::NeedsYou, 5),
            card("d", "fix tests", Status::Working, 3),
            card("e", "Fix lint", Status::NeedsYou, 4),
            card("f", "Fix build", Status::Done, 2),
            card("g", "Fix typo", Status::Working, 8),
        ];
        let got = session_entries("fix", cards);
        assert_eq!(titles(&got), vec!["Fix lint", "Fix CI", "Fix build", "Fix typo", "fix tests"]);
    }

    #[test]
    fn a_session_opens_its_agent_and_reads_its_project_provider_and_status() {
        let got = session_entries("", vec![card("a1", "Fix CI", Status::NeedsYou, 1), card("a2", "Docs", Status::Working, 1), card("a3", "Lint", Status::Done, 1)]);
        let got: Vec<_> = got.into_iter().map(|e| (e.pick, e.lead, e.detail)).collect();
        assert_eq!(
            got,
            vec![
                (Pick::Session("a1".into()), Lead::Waiting, "app · Claude Code · needs you".to_string()),
                (Pick::Session("a3".into()), Lead::Provider("claude".into()), "app · Claude Code · done".to_string()),
                (Pick::Session("a2".into()), Lead::Running, "app · Claude Code · working".to_string()),
            ]
        );
    }

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn without_a_query_files_are_the_first_three_changed_ones() {
        let changed = strings(&["README.md", "src/a.rs", "src/b.rs", "src/c.rs"]);
        let got: Vec<_> = file_entries("", "/w", &changed, &strings(&["x.rs"])).into_iter().map(|e| (e.pick, e.title, e.detail)).collect();
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
        let got: Vec<_> = file_entries("main", "/w", &strings(&["src/main.rs", "notes.txt"]), &files).into_iter().map(|e| (e.title, e.detail)).collect();
        let want = [("main.rs", "src · modified"), ("Main.rs", ""), ("main.md", "docs"), ("main.c", "a"), ("main.h", "b")];
        assert_eq!(got, want.map(|(t, d)| (t.to_string(), d.to_string())));
    }

    #[test]
    fn actions_match_on_their_titles() {
        let all: Vec<_> = action_entries("", "app").into_iter().map(|e| (e.pick, e.title)).collect();
        let want = [(Pick::New, "New session in app"), (Pick::Split, "Open selected in a split"), (Pick::Next, "Jump to next waiting session")];
        assert_eq!(all, want.map(|(p, t)| (p, t.to_string())));
        let session: Vec<_> = action_entries("session", "app").into_iter().map(|e| e.pick).collect();
        assert_eq!(session, vec![Pick::New, Pick::Next]);
    }

    #[test]
    fn arrows_move_the_selection_and_stop_at_either_end() {
        let moves: Vec<_> = [("down", 0), ("down", 2), ("up", 1), ("up", 0)].into_iter().map(|(key, ix)| nav(key, ix, 3)).collect();
        assert_eq!(moves, vec![Some(Nav::To(1)), Some(Nav::To(2)), Some(Nav::To(0)), Some(Nav::To(0))]);
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
