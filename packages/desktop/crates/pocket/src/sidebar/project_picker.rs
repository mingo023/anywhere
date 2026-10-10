use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Screen, id};
use crate::util::tilde;
use agents::locals::is_local;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use theme::*;

pub struct ProjectPicker {
    pub(crate) open: bool,
    pub(crate) search: Entity<InputState>,
    highlight: usize,
}

impl ProjectPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects…"));
        let subs = vec![cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.sidebar.picker.highlight = first_hit(&this.picked_entries(cx));
            }
            cx.notify()
        })];
        (Self { open: false, search, highlight: 0 }, subs)
    }

    fn highlighted_row(&self, total: usize) -> usize {
        self.highlight.min(total.saturating_sub(1))
    }
}

/// A picker row: a project, or one of its worktrees or added Locals.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Entry {
    project: String,
    tree: Option<String>,
    /// False for a project listed only because some of its worktrees match.
    hit: bool,
}

impl Entry {
    /// Whether this row is what's on screen: `cwd` of project `current`, where `main` gives a project's main worktree.
    fn shown(&self, current: Option<&str>, cwd: Option<&str>, main: impl FnOnce(&str) -> Option<String>) -> bool {
        current == Some(self.project.as_str()) && cwd.map(str::to_string) == self.tree.clone().or_else(|| main(&self.project))
    }
}

/// `current` first, then the rest in sidebar order, each followed by its trees: `(key, name)`, a worktree's path or a Local's id.
/// A project stays while `query` is in its name, its path or a tree's name; its trees all stay when the project matches, else only those that match.
pub(crate) fn listed(projects: Vec<String>, current: Option<&str>, query: &str, name: impl Fn(&str) -> String, trees: impl Fn(&str) -> Vec<(String, String)>) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let hit = |text: &str| text.to_lowercase().contains(&query);
    let (mut first, rest): (Vec<String>, Vec<String>) = projects.into_iter().partition(|p| Some(p.as_str()) == current);
    first.extend(rest);
    let mut out = Vec::new();
    for p in first {
        let whole = hit(&format!("{}\n{p}", name(&p)));
        let trees: Vec<String> = trees(&p).into_iter().filter(|(_, n)| whole || hit(n)).map(|(t, _)| t).collect();
        if whole || !trees.is_empty() {
            out.push(Entry { project: p.clone(), tree: None, hit: whole });
            out.extend(trees.into_iter().map(|t| Entry { project: p.clone(), tree: Some(t), hit: true }));
        }
    }
    out
}

/// The row the highlight starts on: the first that matches the search itself.
fn first_hit(entries: &[Entry]) -> usize {
    entries.iter().position(|e| e.hit).unwrap_or(0)
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

impl Desktop {
    fn picked_entries(&self, cx: &App) -> Vec<Entry> {
        let trees = |p: &str| {
            let locals = self.agents.locals_in(p).map(|l| l.id.clone());
            let worktrees = self.listed_trees(p).map(|w| self.creates.trees(p, &w)).unwrap_or_default().into_iter().filter(|w| !w.main).map(|w| w.path);
            locals.chain(worktrees).map(|t| (t.clone(), self.place_name(&t))).collect()
        };
        listed(self.projects(), self.project.as_deref(), &self.sidebar.picker.search.read(cx).value(), |p| self.repo_name(p), trees)
    }

    pub(crate) fn toggle_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = !self.sidebar.picker.open;
        self.close_menus();
        self.sidebar.picker.open = open;
        if open {
            self.sidebar.picker.highlight = 0;
            self.sidebar.picker.search.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn pick(&mut self, e: Entry, cx: &mut Context<Self>) {
        self.sidebar.picker.open = false;
        if let Some(tree) = &e.tree {
            self.creates.show_progress(tree);
        }
        self.select_tree(e.project, e.tree, cx);
    }

    fn on_picker_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let entries = self.picked_entries(cx);
        let ix = self.sidebar.picker.highlighted_row(entries.len());
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            let Some(e) = entries.into_iter().nth(ix) else { return };
            self.pick(e, cx);
        } else {
            let Some(ix) = step(key, ix, entries.len()) else { return };
            self.sidebar.picker.highlight = ix;
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn project_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let entries = self.picked_entries(cx);
        let ix = self.sidebar.picker.highlighted_row(entries.len());
        let (current, tree) = (self.project.as_deref().filter(|_| self.screen == Screen::Sessions), self.place());
        let rows = entries.iter().enumerate().map(|(i, e)| {
            let lead = match &e.tree {
                _ if e.shown(current, tree.as_deref(), |p| self.tree_of(p)) => icon("check", 14., TEXT).into_any_element(),
                Some(tree) => icon(if is_local(tree) { "laptop" } else { "worktree" }, 14., TEXT_3).into_any_element(),
                None => ui::repo_mark(&self.repo_name(&e.project), false, None).size(px(18.)).text_size(px(10.)).into_any_element(),
            };
            let target = e.clone();
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
                .when(e.tree.is_some(), |d| d.pl(px(28.)))
                .child(div().w(px(18.)).flex().flex_none().justify_center().child(lead))
                .map(|d| match &e.tree {
                    Some(tree) => d.child(div().flex_1().min_w_0().truncate().child(self.place_name(tree))),
                    None => d
                        .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(self.repo_name(&e.project)))
                        .child(div().max_w(px(112.)).flex_none().truncate().font_family(MONO).text_size(px(11.)).text_color(TEXT_4).child(parent(&e.project)))
                        .children(ui::indicator(id(format!("projects-state:{}", e.project)), self.project_state(&e.project))),
                })
                .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                    if this.sidebar.picker.highlight != i {
                        this.sidebar.picker.highlight = i;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(target.clone(), cx)))
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
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.sidebar.picker.search).appearance(false).p_0().text_size(px(13.))));
        let empty = div().h(px(36.)).px(px(10.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("No projects found");
        let list = div().id("projects-list").max_h(px(290.)).overflow_y_scroll().p(px(5.)).flex().flex_col().when(entries.is_empty(), |d| d.child(empty)).children(rows);
        let new = ui::menu_row("projects-new", "plus", "New project", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.sidebar.picker.open = false;
            this.open(Overlay::AddRepo, window, cx);
        }));
        ui::pop(div().id("projects-menu"))
            .w(px(286.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .occlude()
            .capture_key_down(cx.listener(Self::on_picker_key))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.sidebar.picker.open = false;
                cx.notify();
            }))
            .child(search)
            .child(list)
            .child(div().p(px(5.)).border_t(px(0.5)).border_color(SEPARATOR).child(new))
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, first_hit, listed, parent, step};
    use crate::util::basename;

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn no_trees(_: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    /// `/w/app`'s worktrees, named by folder.
    fn trees(p: &str) -> Vec<(String, String)> {
        let paths = if p == "/w/app" { strings(&["/t/fix-login", "/t/dark-mode"]) } else { Vec::new() };
        paths.into_iter().map(|t| (t.clone(), basename(&t))).collect()
    }

    fn projects(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|e| e.project).collect()
    }

    fn rows(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|e| e.tree.unwrap_or(e.project)).collect()
    }

    #[test]
    fn the_current_project_leads_and_the_rest_keep_the_sidebar_order() {
        let got = listed(strings(&["/w/api", "/w/app", "/w/docs"]), Some("/w/docs"), "", basename, no_trees);
        assert_eq!(projects(got), strings(&["/w/docs", "/w/api", "/w/app"]));
    }

    #[test]
    fn the_search_matches_a_name_or_a_path_ignoring_case() {
        let all = strings(&["/work/api", "/self/app"]);
        let named = |p: &str| if p == "/self/app" { "Pocket".to_string() } else { basename(p) };
        assert_eq!(projects(listed(all.clone(), None, " API ", named, no_trees)), strings(&["/work/api"]));
        assert_eq!(projects(listed(all.clone(), None, "pock", named, no_trees)), strings(&["/self/app"]));
        assert_eq!(projects(listed(all.clone(), None, "self/", named, no_trees)), strings(&["/self/app"]));
        assert_eq!(listed(all, None, "zzz", named, no_trees), Vec::new());
    }

    #[test]
    fn each_project_is_followed_by_its_worktrees() {
        let got = listed(strings(&["/w/api", "/w/app"]), None, "", basename, trees);
        assert_eq!(rows(got), strings(&["/w/api", "/w/app", "/t/fix-login", "/t/dark-mode"]));
    }

    #[test]
    fn a_matching_worktree_keeps_its_project_and_a_matching_project_keeps_all_its_worktrees() {
        assert_eq!(rows(listed(strings(&["/w/api", "/w/app"]), None, "dark", basename, trees)), strings(&["/w/app", "/t/dark-mode"]));
        assert_eq!(rows(listed(strings(&["/w/api", "/w/app"]), None, "app", basename, trees)), strings(&["/w/app", "/t/fix-login", "/t/dark-mode"]));
    }

    #[test]
    fn a_local_is_found_by_its_name() {
        let trees = |_: &str| vec![("local-1".to_string(), "Review".to_string()), ("/t/fix".to_string(), "fix".to_string())];
        assert_eq!(rows(listed(strings(&["/w/app"]), None, "review", basename, trees)), strings(&["/w/app", "local-1"]));
    }

    #[test]
    fn the_highlight_starts_on_the_worktree_the_search_found_not_its_project() {
        assert_eq!(first_hit(&listed(strings(&["/w/api", "/w/app"]), None, "dark", basename, trees)), 1);
        assert_eq!(first_hit(&listed(strings(&["/w/api", "/w/app"]), None, "", basename, trees)), 0);
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
