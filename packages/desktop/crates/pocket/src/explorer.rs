pub(crate) mod mermaid;
pub(crate) mod preview;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Side, id};
use crate::util::list_dir;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use theme::*;
use workspace::Doc;

/// Fresh listings for the folders still open, so a folder opened or closed while they were read keeps its new state.
pub fn merge_tree(fresh: HashMap<PathBuf, Vec<(bool, PathBuf)>>, open: &HashMap<PathBuf, Vec<(bool, PathBuf)>>, root: Option<&Path>) -> HashMap<PathBuf, Vec<(bool, PathBuf)>> {
    let mut tree: HashMap<_, _> = fresh.into_iter().filter(|(dir, _)| open.contains_key(dir) || root == Some(dir.as_path())).collect();
    for (dir, listing) in open {
        tree.entry(dir.clone()).or_insert_with(|| listing.clone());
    }
    tree
}

pub fn status_word(status: Option<char>) -> (u32, &'static str) {
    match status {
        Some('A') => (RUNNING, "Added"),
        Some('D') => (FAILED, "Deleted"),
        Some(_) => (WAITING, "Modified"),
        None => (TEXT_5, "Unchanged"),
    }
}

impl Desktop {
    /// The folder Explore browses: the chosen worktree, else the project.
    pub fn explore_root(&self) -> Option<String> {
        self.worktree.clone().or_else(|| self.project.clone())
    }

    /// Git's letter for a file under the explore root, if it has changed.
    pub fn file_status(&self, path: &str) -> Option<char> {
        let root = self.explore_root()?;
        let rel = path.strip_prefix(&root)?.trim_start_matches('/');
        self.repos.get(&root)?.files.iter().find(|f| f.path == rel).map(|f| f.status)
    }

    fn touched(&self) -> Vec<String> {
        let Some(root) = self.explore_root() else { return Vec::new() };
        let files = self.repos.get(&root).map(|r| r.files.clone()).unwrap_or_default();
        files.into_iter().map(|f| format!("{root}/{}", f.path)).filter(|p| self.agents.last_edit(p).is_some()).collect()
    }

    pub fn open_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.side = Side::Explorer;
        self.open_doc(Doc::File(path), cx);
    }

    pub fn go_to_file(&mut self, _: &crate::actions::GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_all = false;
        self.open(Overlay::Palette, window, cx);
    }

    pub fn explorer(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let touched = self.touched();
        let go = ui::trigger_field("go-to-file", "search", "Go to file…", "⌘P")
            .mx(px(8.))
            .mt(px(8.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.go_to_file(&crate::actions::GoToFile, window, cx)));
        let mut rows = Vec::new();
        if let Some(root) = self.explore_root() {
            self.tree(Path::new(&root), 0, &touched, &mut rows, cx);
        }
        let list = div().id("explorer").flex_1().min_h_0().px(px(8.)).pt(px(8.)).pb(px(8.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(rows);
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(list)
    }

    fn file_row(&self, path: String, label: String, depth: usize, touched: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.file.as_ref() == Some(&path);
        let git = self.file_status(&path);
        ui::tree_row(id(format!("tree-{path}")), label, false, false, depth, selected, touched, git)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_file(path.clone(), cx)))
    }

    fn tree(&self, dir: &Path, depth: usize, touched: &[String], rows: &mut Vec<Stateful<Div>>, cx: &mut Context<Self>) {
        for (is_dir, path) in self.tree.get(dir).cloned().unwrap_or_default() {
            let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let key = path.to_string_lossy().to_string();
            if !is_dir {
                rows.push(self.file_row(key.clone(), label, depth, touched.contains(&key), cx));
                continue;
            }
            let open = self.tree.contains_key(&path);
            let target = path.clone();
            rows.push(ui::tree_row(id(format!("tree-{key}")), label, true, open, depth, false, false, None).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    if this.tree.remove(&target).is_none() {
                        this.tree.insert(target.clone(), list_dir(&target));
                    }
                    cx.notify();
                },
            )));
            if open {
                self.tree(&path, depth + 1, touched, rows, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::merge_tree;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_folder_toggled_during_a_refresh_keeps_its_state() {
        let listing = |names: &[&str]| names.iter().map(|n| (true, PathBuf::from(n))).collect::<Vec<_>>();
        let fresh = HashMap::from([("/r".into(), listing(&["/r/a", "/r/b", "/r/c"])), ("/r/a".into(), listing(&["/r/a/new"]))]);
        let open = HashMap::from([("/r".into(), listing(&["/r/a", "/r/b"])), ("/r/b".into(), listing(&["/r/b/x"]))]);
        let tree = merge_tree(fresh, &open, Some(Path::new("/r")));
        let mut dirs: Vec<&PathBuf> = tree.keys().collect();
        dirs.sort();
        assert_eq!(dirs, [Path::new("/r"), Path::new("/r/b")]);
        assert_eq!(tree[Path::new("/r")], listing(&["/r/a", "/r/b", "/r/c"]));
    }
}
