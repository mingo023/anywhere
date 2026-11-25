use crate::desktop::Desktop;
use crate::desktop::chrome::{Screen, id};
use crate::util::{basename, tilde, tree_icon};
use agents::locals::is_local;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use theme::*;

pub(crate) enum PickerEvent {
    Searched,
    Pick(Entry),
    NewProject,
    OpenHostFolder,
}

impl EventEmitter<PickerEvent> for ProjectPicker {}

pub struct ProjectPicker {
    pub(crate) open: bool,
    search: Entity<InputState>,
    highlight: usize,
    rows: Vec<Row>,
    _sub: Subscription,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Lead {
    Check,
    Tree(&'static str),
    Project(ui::Mark),
}

#[derive(Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) entry: Entry,
    pub(crate) lead: Lead,
    pub(crate) title: String,
    pub(crate) parent: String,
    pub(crate) state: Option<ui::State>,
}

impl ProjectPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects…"));
        let _sub = cx.subscribe(&search, |_, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                cx.emit(PickerEvent::Searched);
            }
        });
        Self { open: false, search, highlight: 0, rows: Vec::new(), _sub }
    }

    pub(crate) fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    pub(crate) fn open_fresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.highlight = 0;
        self.search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn shut(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<Row>, cx: &mut Context<Self>) {
        if self.rows != rows {
            self.rows = rows;
            cx.notify();
        }
    }

    pub(crate) fn highlight_first(&mut self) {
        self.highlight = first_hit(self.rows.iter().map(|r| &r.entry));
    }

    fn highlighted_row(&self) -> usize {
        self.highlight.min(self.rows.len().saturating_sub(1))
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ix = self.highlighted_row();
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            let Some(row) = self.rows.get(ix) else { return };
            cx.emit(PickerEvent::Pick(row.entry.clone()));
        } else {
            let Some(ix) = step(key, ix, self.rows.len()) else { return };
            self.highlight = ix;
        }
        cx.stop_propagation();
        cx.notify();
    }
}

/// A picker row: a project, or one of its worktrees or added Locals.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Entry {
    pub(crate) project: String,
    pub(crate) tree: Option<String>,
    /// False for a project listed only because some of its worktrees match.
    hit: bool,
}

impl Entry {
    /// Whether this row is what's on screen: `cwd` of project `current`, where `main` gives a project's main worktree.
    pub(super) fn shown(&self, current: Option<&str>, cwd: Option<&str>, main: impl FnOnce(&str) -> Option<String>) -> bool {
        current == Some(self.project.as_str()) && cwd.map(str::to_string) == self.tree.clone().or_else(|| main(&self.project))
    }
}

/// `current` first, then the rest in sidebar order, each followed by its trees: `(key, texts it's found by)`, the key a worktree's path or a Local's id.
/// A project stays while `query` is in one of its `search_texts`, its path or a tree's texts; its trees all stay when the project matches, else only those that match.
pub(crate) fn listed(
    projects: Vec<String>,
    current: Option<&str>,
    query: &str,
    search_texts: impl Fn(&str) -> Vec<String>,
    trees: impl Fn(&str) -> Vec<(String, Vec<String>)>,
) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let hit = |text: &str| text.to_lowercase().contains(&query);
    let (mut first, rest): (Vec<String>, Vec<String>) = projects.into_iter().partition(|p| Some(p.as_str()) == current);
    first.extend(rest);
    let mut out = Vec::new();
    for p in first {
        let whole = hit(&p) || search_texts(&p).iter().any(|t| hit(t));
        let trees: Vec<String> = trees(&p).into_iter().filter(|(_, texts)| whole || texts.iter().any(|t| hit(t))).map(|(t, _)| t).collect();
        if whole || !trees.is_empty() {
            out.push(Entry { project: p.clone(), tree: None, hit: whole });
            out.extend(trees.into_iter().map(|t| Entry { project: p.clone(), tree: Some(t), hit: true }));
        }
    }
    out
}

/// The row the highlight starts on: the first that matches the search itself.
fn first_hit<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> usize {
    entries.into_iter().position(|e| e.hit).unwrap_or(0)
}

fn step(key: &str, ix: usize, total: usize) -> Option<usize> {
    match key {
        "up" => Some(ix.saturating_sub(1)),
        "down" => Some((ix + 1).min(total.saturating_sub(1))),
        _ => None,
    }
}

fn parent(path: &str) -> String {
    tilde(&Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
}

/// What tree `key` is found by: its `given` name, else its folder's, and a renamed worktree's folder as well.
fn tree_texts(key: &str, given: Option<&str>) -> Vec<String> {
    match given {
        Some(n) if is_local(key) => vec![n.to_string()],
        Some(n) => vec![n.to_string(), basename(key)],
        None => vec![basename(key)],
    }
}

/// What a project is found by besides its path: its `name`, and the name its own Local was given.
fn project_texts(name: &str, own: Option<&str>) -> Vec<String> {
    std::iter::once(name).chain(own).map(str::to_string).collect()
}

impl Desktop {
    fn picked_entries(&self, cx: &App) -> Vec<Entry> {
        let trees = |p: &str| {
            let locals = self.agents.locals_in(p).map(|l| l.id.clone());
            let worktrees =
                self.listed_trees(p).map(|w| self.creates.read(cx).trees(p, &w)).unwrap_or_default().into_iter().filter(|w| !w.main).map(|w| w.path);
            locals.chain(worktrees).map(|t| (t.clone(), tree_texts(&t, self.agents.given_name(&t)))).collect()
        };
        let search_texts = |p: &str| project_texts(&self.repo_name(p), self.tree_of(p, cx).as_deref().and_then(|t| self.agents.given_name(t)));
        let query = self.sidebar.read(cx).picker.read(cx).query(cx);
        listed(self.store.projects.clone(), self.project.as_deref(), &query, search_texts, trees)
    }

    pub(crate) fn picked_rows(&self, cx: &App) -> Vec<Row> {
        let (current, tree) = (self.project.as_deref().filter(|_| self.screen == Screen::Sessions), self.place(cx));
        self.picked_entries(cx)
            .into_iter()
            .map(|entry| {
                let lead = match &entry.tree {
                    _ if entry.shown(current, tree.as_deref(), |p| self.tree_of(p, cx)) => Lead::Check,
                    Some(tree) => Lead::Tree(tree_icon(tree)),
                    None => Lead::Project(self.project_mark(&entry.project, ui::mark_letter(&self.repo_name(&entry.project)))),
                };
                match entry.tree.clone() {
                    Some(tree) => Row { title: self.place_name(&tree), parent: String::new(), state: None, lead, entry },
                    None => Row {
                        title: self.repo_name(&entry.project),
                        parent: parent(&entry.project),
                        state: self.project_state(&entry.project, cx),
                        lead,
                        entry,
                    },
                }
            })
            .collect()
    }

    pub(crate) fn sync_picker(&self, cx: &mut App) {
        let picker = self.sidebar.read(cx).picker.clone();
        if picker.read(cx).open {
            let rows = self.picked_rows(cx);
            picker.update(cx, |p, cx| p.set_rows(rows, cx));
        }
    }

    pub(crate) fn toggle_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = self.sidebar.read(cx).picker.clone();
        let open = !picker.read(cx).open;
        self.close_menus(cx);
        if open {
            picker.update(cx, |p, cx| p.open_fresh(window, cx));
        }
        cx.notify();
    }

    fn pick(&mut self, e: Entry, cx: &mut Context<Self>) {
        self.sidebar.read(cx).picker.clone().update(cx, |p, cx| p.shut(cx));
        if let Some(tree) = &e.tree {
            self.creates.update(cx, |c, _| c.show_progress(tree));
        }
        self.select_tree(e.project, e.tree, cx);
    }

    pub(super) fn on_picker_event(&mut self, picker: &Entity<ProjectPicker>, ev: &PickerEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PickerEvent::Searched => {
                let rows = self.picked_rows(cx);
                picker.update(cx, |p, cx| {
                    p.set_rows(rows, cx);
                    p.highlight_first();
                });
            }
            PickerEvent::Pick(entry) => self.pick(entry.clone(), cx),
            PickerEvent::NewProject => self.add_local_project(window, cx),
            PickerEvent::OpenHostFolder => self.open_host_folder(window, cx),
        }
        cx.notify();
    }
}

impl Render for ProjectPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ix = self.highlighted_row();
        let rows = self.rows.iter().enumerate().map(|(i, r)| {
            let lead = match &r.lead {
                Lead::Check => icon("check", 14., TEXT).into_any_element(),
                Lead::Tree(name) => icon(name, 14., TEXT_3).into_any_element(),
                Lead::Project(mark) => ui::repo_mark(mark, false, None).size(px(18.)).text_size(px((18. * mark.scale(0.56)).round())).into_any_element(),
            };
            let target = r.entry.clone();
            div()
                .id(("projects-row", i))
                .h(px(36.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(i == ix, |d| d.bg(FILL_3).text_color(TEXT))
                .when(i != ix, |d| d.text_color(TEXT_2))
                .when(r.entry.tree.is_some(), |d| d.pl(px(28.)))
                .child(div().w(px(18.)).flex().flex_none().justify_center().child(lead))
                .map(|d| match &r.entry.tree {
                    Some(_) => d.child(div().flex_1().min_w_0().truncate().child(r.title.clone())),
                    None => d
                        .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(r.title.clone()))
                        .child(div().max_w(px(112.)).flex_none().truncate().font_family(MONO).text_size(px(11.)).text_color(TEXT_4).child(r.parent.clone()))
                        .children(ui::indicator(id(format!("projects-state:{}", r.entry.project)), r.state)),
                })
                .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                    if this.highlight != i {
                        this.highlight = i;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PickerEvent::Pick(target.clone()))))
        });
        let search = div()
            .h(px(44.))
            .px(px(14.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("search", 15., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.search).appearance(false).p_0().text_size(px(13.))));
        let empty = div().h(px(36.)).px(px(10.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("No projects found");
        let list = div()
            .id("projects-list")
            .max_h(px(290.))
            .overflow_y_scroll()
            .p(px(5.))
            .flex()
            .flex_col()
            .when(self.rows.is_empty(), |d| d.child(empty))
            .children(rows);
        let new = ui::menu_row("projects-new", "plus", "New project", None).on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PickerEvent::NewProject)));
        let host = ui::menu_row("projects-host-folder", "globe", "Open folder on a host…", None)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PickerEvent::OpenHostFolder)));
        ui::pop(div().id("projects-menu"))
            .w(px(286.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .occlude()
            .capture_key_down(cx.listener(Self::on_key))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.shut(cx)))
            .child(search)
            .child(list)
            .child(div().p(px(5.)).border_t(px(0.5)).border_color(SEPARATOR).child(new).child(host))
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, first_hit, listed, parent, project_texts, step, tree_texts};
    use crate::util::basename;

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn folder_name(p: &str) -> Vec<String> {
        vec![basename(p)]
    }

    fn no_trees(_: &str) -> Vec<(String, Vec<String>)> {
        Vec::new()
    }

    /// `/w/app`'s worktrees, named by folder.
    fn trees(p: &str) -> Vec<(String, Vec<String>)> {
        let paths = if p == "/w/app" { strings(&["/t/fix-login", "/t/dark-mode"]) } else { Vec::new() };
        paths.into_iter().map(|t| (t.clone(), folder_name(&t))).collect()
    }

    fn projects(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|e| e.project).collect()
    }

    fn rows(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|e| e.tree.unwrap_or(e.project)).collect()
    }

    #[test]
    fn the_current_project_leads_and_the_rest_keep_the_sidebar_order() {
        let got = listed(strings(&["/w/api", "/w/app", "/w/docs"]), Some("/w/docs"), "", folder_name, no_trees);
        assert_eq!(projects(got), strings(&["/w/docs", "/w/api", "/w/app"]));
    }

    #[test]
    fn the_search_matches_a_name_or_a_path_ignoring_case() {
        let all = strings(&["/work/api", "/self/app"]);
        let named = |p: &str| if p == "/self/app" { vec!["Pocket".to_string()] } else { folder_name(p) };
        assert_eq!(projects(listed(all.clone(), None, " API ", named, no_trees)), strings(&["/work/api"]));
        assert_eq!(projects(listed(all.clone(), None, "pock", named, no_trees)), strings(&["/self/app"]));
        assert_eq!(projects(listed(all.clone(), None, "self/", named, no_trees)), strings(&["/self/app"]));
        assert_eq!(listed(all, None, "zzz", named, no_trees), Vec::new());
    }

    #[test]
    fn each_project_is_followed_by_its_worktrees() {
        let got = listed(strings(&["/w/api", "/w/app"]), None, "", folder_name, trees);
        assert_eq!(rows(got), strings(&["/w/api", "/w/app", "/t/fix-login", "/t/dark-mode"]));
    }

    #[test]
    fn a_matching_worktree_keeps_its_project_and_a_matching_project_keeps_all_its_worktrees() {
        assert_eq!(rows(listed(strings(&["/w/api", "/w/app"]), None, "dark", folder_name, trees)), strings(&["/w/app", "/t/dark-mode"]));
        assert_eq!(rows(listed(strings(&["/w/api", "/w/app"]), None, "app", folder_name, trees)), strings(&["/w/app", "/t/fix-login", "/t/dark-mode"]));
    }

    #[test]
    fn a_renamed_worktree_is_found_by_its_name_or_folder_and_a_local_by_its_name_alone() {
        assert_eq!(tree_texts("/t/calm-otter", Some("Sign in")), ["Sign in", "calm-otter"]);
        assert_eq!(tree_texts("/t/calm-otter", None), ["calm-otter"]);
        assert_eq!(tree_texts("local-1", Some("Review")), ["Review"]);
    }

    #[test]
    fn a_project_is_found_by_the_name_its_own_local_was_given() {
        let texts = |p: &str| project_texts(&basename(p), (p == "/w/app").then_some("Hotfix"));
        assert_eq!(projects(listed(strings(&["/w/api", "/w/app"]), None, "hot", texts, no_trees)), strings(&["/w/app"]));
        assert_eq!(project_texts("api", None), ["api"]);
    }

    #[test]
    fn a_local_is_found_by_its_name() {
        let trees = |_: &str| vec![("local-1".to_string(), strings(&["Review"])), ("/t/fix".to_string(), strings(&["fix"]))];
        assert_eq!(rows(listed(strings(&["/w/app"]), None, "review", folder_name, trees)), strings(&["/w/app", "local-1"]));
    }

    #[test]
    fn the_highlight_starts_on_the_worktree_the_search_found_not_its_project() {
        assert_eq!(first_hit(&listed(strings(&["/w/api", "/w/app"]), None, "dark", folder_name, trees)), 1);
        assert_eq!(first_hit(&listed(strings(&["/w/api", "/w/app"]), None, "", folder_name, trees)), 0);
    }

    #[test]
    fn the_check_marks_the_worktree_on_screen_and_the_project_row_stands_for_its_main_one() {
        let entry = |tree: Option<&str>| Entry { project: "/w/app".into(), tree: tree.map(str::to_string), hit: true };
        let main = |_: &str| Some("/w/app".to_string());
        assert!(entry(Some("/t/fix")).shown(Some("/w/app"), Some("/t/fix"), main));
        assert!(!entry(None).shown(Some("/w/app"), Some("/t/fix"), main));
        assert!(entry(None).shown(Some("/w/app"), Some("/w/app"), main));
        assert!(!entry(None).shown(Some("/w/api"), Some("/w/app"), main));
    }

    #[test]
    fn arrows_move_the_highlight_and_stop_at_the_ends() {
        let got = [step("up", 0, 3), step("down", 0, 3), step("down", 2, 3), step("up", 2, 3), step("down", 0, 0), step("x", 1, 3)];
        assert_eq!(got, [Some(0), Some(1), Some(2), Some(1), Some(0), None]);
    }

    #[test]
    fn a_row_names_the_folder_its_project_sits_in() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(parent(&format!("{home}/work/app")), "~/work");
        assert_eq!(parent("/srv/app"), "/srv");
    }
}
