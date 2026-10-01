pub(crate) mod mermaid;
pub(crate) mod preview;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Side, id};
use crate::util::list_dir;
use git::FileStat;
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

/// Git's letter for the file at `path` among a repo's changed `files`, which are relative to `root`.
pub fn status_of(files: &[FileStat], root: &str, path: &str) -> Option<char> {
    let rel = path.strip_prefix(root)?.trim_start_matches('/');
    files.iter().find(|f| f.path == rel).map(|f| f.status)
}

/// The changed `files` that `edited` says an agent wrote, as absolute paths.
pub fn touched(files: &[FileStat], root: &str, edited: impl Fn(&str) -> bool) -> Vec<String> {
    files.iter().map(|f| format!("{root}/{}", f.path)).filter(|p| edited(p)).collect()
}

pub fn status_word(status: Option<char>) -> (Token, &'static str) {
    match status {
        Some('A') => (SUCCESS, "Added"),
        Some('D') => (FAILED, "Deleted"),
        Some(_) => (WAITING, "Modified"),
        None => (TEXT_5, "Unchanged"),
    }
}

pub struct ExplorerState {
    pub(crate) tree: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
}

/// A line of the tree: a file, or a folder and whether it is open.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub open: bool,
}

impl ExplorerState {
    pub fn new() -> Self {
        Self { tree: HashMap::new() }
    }

    /// The tree under `root` top to bottom, descending into open folders only.
    pub fn rows(&self, root: &Path) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_rows(root, 0, &mut rows);
        rows
    }

    /// Folds `dir` if open, else lists and opens it.
    pub fn toggle(&mut self, dir: &Path) {
        if self.tree.remove(dir).is_none() {
            self.tree.insert(dir.to_path_buf(), list_dir(dir));
        }
    }

    fn push_rows(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        for (is_dir, path) in self.tree.get(dir).into_iter().flatten() {
            let open = *is_dir && self.tree.contains_key(path);
            rows.push(Row { path: path.clone(), depth, is_dir: *is_dir, open });
            if open {
                self.push_rows(path, depth + 1, rows);
            }
        }
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
        status_of(&self.repos.get(&root)?.files, &root, path)
    }

    fn touched(&self) -> Vec<String> {
        let Some(root) = self.explore_root() else { return Vec::new() };
        let files = self.repos.get(&root).map(|r| r.files.as_slice()).unwrap_or_default();
        touched(files, &root, |p| self.agents.last_edit(p).is_some())
    }

    pub fn open_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.side = Side::Explorer;
        self.open_doc(Doc::File(path), cx);
    }

    pub fn go_to_file(&mut self, _: &crate::actions::GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.all = false;
        self.open(Overlay::Palette, window, cx);
    }

    pub fn explorer(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let touched = self.touched();
        let go = ui::trigger_field("go-to-file", "search", "Go to file…", "⌘P")
            .mx(px(8.))
            .mt(px(8.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.go_to_file(&crate::actions::GoToFile, window, cx)));
        let rows = self.explore_root().map(|root| self.explorer.rows(Path::new(&root))).unwrap_or_default();
        let rows: Vec<_> = rows.into_iter().map(|row| self.tree_row(row, &touched, cx)).collect();
        let list = div().id("explorer").flex_1().min_h_0().px(px(8.)).pt(px(8.)).pb(px(8.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(rows);
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(list)
    }

    fn tree_row(&self, Row { path, depth, is_dir, open }: Row, touched: &[String], cx: &mut Context<Self>) -> Stateful<Div> {
        let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let key = path.to_string_lossy().to_string();
        if !is_dir {
            let selected = self.preview.file.as_ref() == Some(&key);
            let git = self.file_status(&key);
            let touched = touched.contains(&key);
            return ui::tree_row(id(format!("tree-{key}")), label, false, false, depth, selected, touched, git)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_file(key.clone(), cx)));
        }
        ui::tree_row(id(format!("tree-{key}")), label, true, open, depth, false, false, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.explorer.toggle(&path);
            cx.notify();
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{ExplorerState, Row, merge_tree, status_of, touched};
    use git::FileStat;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    fn stat(path: &str, status: char) -> FileStat {
        FileStat { path: path.into(), added: 0, removed: 0, staged: false, unstaged: true, status }
    }

    #[test]
    fn a_changed_file_shows_its_git_letter() {
        let files = [stat("src/a.rs", 'M'), stat("new.rs", 'A')];
        assert_eq!(status_of(&files, "/r", "/r/src/a.rs"), Some('M'));
        assert_eq!(status_of(&files, "/r", "/r/new.rs"), Some('A'));
    }

    #[test]
    fn unchanged_files_folders_and_files_outside_the_root_show_no_letter() {
        let files = [stat("src/a.rs", 'M')];
        assert_eq!(status_of(&files, "/r", "/r/src/b.rs"), None);
        assert_eq!(status_of(&files, "/r", "/r/src"), None);
        assert_eq!(status_of(&files, "/r", "/elsewhere/src/a.rs"), None);
    }

    #[test]
    fn touched_are_the_changed_files_an_agent_edited() {
        let files = [stat("a.rs", 'M'), stat("src/b.rs", 'A'), stat("c.rs", 'D')];
        let edited = |p: &str| p != "/r/c.rs";
        assert_eq!(touched(&files, "/r", edited), ["/r/a.rs", "/r/src/b.rs"]);
    }

    fn entry(is_dir: bool, path: &str) -> (bool, PathBuf) {
        (is_dir, PathBuf::from(path))
    }

    fn row(path: &str, depth: usize, is_dir: bool, open: bool) -> Row {
        Row { path: PathBuf::from(path), depth, is_dir, open }
    }

    #[test]
    fn tree_nests_what_an_open_folder_holds_and_hides_what_a_folded_one_holds() {
        let mut explorer = ExplorerState::new();
        explorer.tree = HashMap::from([
            ("/r".into(), vec![entry(true, "/r/open"), entry(true, "/r/shut"), entry(false, "/r/a.rs")]),
            ("/r/open".into(), vec![entry(false, "/r/open/b.rs")]),
        ]);
        assert_eq!(
            explorer.rows(Path::new("/r")),
            [row("/r/open", 0, true, true), row("/r/open/b.rs", 1, false, false), row("/r/shut", 0, true, false), row("/r/a.rs", 0, false, false)]
        );
    }

    #[test]
    fn toggling_opens_a_folder_with_its_listing_then_folds_it_keeping_open_subfolders() {
        let dir = std::env::temp_dir().join(format!("pocket-explorer-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        let mut explorer = ExplorerState::new();
        explorer.tree.insert(dir.join("sub"), Vec::new());
        explorer.toggle(&dir);
        assert_eq!(explorer.tree[&dir], [(true, dir.join("sub")), (false, dir.join("a.rs"))]);
        explorer.toggle(&dir);
        assert_eq!(explorer.tree.keys().collect::<Vec<_>>(), [&dir.join("sub")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

    #[test]
    fn a_refresh_brings_the_root_listing_even_before_anything_is_open() {
        let fresh = HashMap::from([("/r".into(), vec![entry(false, "/r/a.rs")]), ("/old".into(), vec![entry(false, "/old/b.rs")])]);
        let tree = merge_tree(fresh.clone(), &HashMap::new(), Some(Path::new("/r")));
        assert_eq!(tree, HashMap::from([("/r".into(), vec![entry(false, "/r/a.rs")])]));
        assert_eq!(merge_tree(fresh, &HashMap::new(), None), HashMap::new());
    }
}
