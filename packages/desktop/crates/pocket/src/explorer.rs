pub(crate) mod edit;
pub(crate) mod files;
pub(crate) mod frontmatter;
pub(crate) mod menu;
pub(crate) mod mermaid;
pub(crate) mod preview;
pub(crate) mod toast;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Side, id};
use crate::util::list_dir;
use git::FileStat;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use theme::*;
use std::time::{Duration, Instant};
use workspace::Doc;

/// How long a row just made, renamed or put back stays ringed.
const FLASH: Duration = Duration::from_millis(900);

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
    pub(crate) menu: Option<menu::TreeMenu>,
    /// The menu just closed, fading out, and when it closed.
    leaving: Option<(menu::TreeMenu, Instant)>,
    edit: Option<edit::NameEdit>,
    notice: Option<(toast::Notice, Task<()>)>,
    /// The last trash, while its notice shows.
    undo: Option<toast::Trashed>,
    flash: Option<(PathBuf, Task<()>)>,
    scroll: UniformListScrollHandle,
    /// Scroll the edit row, or else the flashed one, into view once it's listed.
    seek: bool,
}

/// A line of the tree: a file, or a folder and whether it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub open: bool,
}

impl ExplorerState {
    pub fn new() -> Self {
        Self { tree: HashMap::new(), menu: None, leaving: None, edit: None, notice: None, undo: None, flash: None, scroll: UniformListScrollHandle::new(), seek: false }
    }

    /// The tree under `root` top to bottom, descending into open folders only.
    pub fn rows(&self, root: &Path) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_rows(root, 0, &mut rows);
        rows
    }

    /// Folds `dir`, keeping its open subfolders; false when it wasn't open.
    pub fn fold(&mut self, dir: &Path) -> bool {
        self.tree.remove(dir).is_some()
    }

    /// Whether the menu is open on `path`, or on the root for `None`.
    pub(crate) fn menu_on(&self, path: Option<&Path>) -> bool {
        self.menu.as_ref().is_some_and(|m| m.target.as_ref().map(|(p, _)| p.as_path()) == path)
    }

    fn flashing(&self, path: &Path) -> bool {
        self.flash.as_ref().is_some_and(|(p, _)| p == path)
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
    /// The folder Explore browses: the chosen tree's folder, else the project.
    pub fn explore_root(&self) -> Option<String> {
        self.worktree.as_deref().and_then(|t| self.folder_of(t)).or_else(|| self.project.clone())
    }

    /// Whether `path` lies in the tree on screen, so what a slow write made there may be shown.
    pub(crate) fn in_explore_root(&self, path: &Path) -> bool {
        self.explore_root().is_some_and(|r| path.starts_with(r))
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

    /// Rings `path`'s row for a moment and scrolls to it: what a create, rename or undo just made.
    pub(crate) fn flash_row(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let fade = cx.spawn(async |this, cx| {
            cx.background_executor().timer(FLASH).await;
            this.update(cx, |d, cx| {
                d.explorer.flash = None;
                cx.notify();
            })
            .ok();
        });
        self.explorer.flash = Some((path, fade));
        self.explorer.seek = true;
    }

    /// Lists the open folders again under new Explorer settings, applied only while those settings still hold.
    pub(crate) fn relist_explorer(&mut self, cx: &mut Context<Self>) {
        let files = self.store.files.clone();
        let mut dirs: Vec<PathBuf> = self.explorer.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        cx.spawn(async move |this, cx| {
            let shown = files.clone();
            let fresh: HashMap<_, _> = cx.background_executor().spawn(async move { dirs.into_iter().map(|d| { let l = list_dir(&d, &shown); (d, l) }).collect() }).await;
            this.update(cx, |d, cx| {
                if d.store.files != files {
                    return;
                }
                d.explorer.tree = merge_tree(fresh, &d.explorer.tree, d.explore_root().as_deref().map(Path::new));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Folds `dir` if open, else lists it in the background and opens it, unless Explorer settings changed meanwhile.
    fn toggle_folder(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.explorer.fold(&dir) {
            return cx.notify();
        }
        let files = self.store.files.clone();
        cx.spawn(async move |this, cx| {
            let listing = cx.background_executor().spawn({
                let (dir, files) = (dir.clone(), files.clone());
                async move { list_dir(&dir, &files) }
            });
            let listing = listing.await;
            this.update(cx, |d, cx| {
                if d.store.files == files {
                    d.explorer.tree.insert(dir, listing);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn open_file(&mut self, path: String, pin: bool, cx: &mut Context<Self>) {
        self.side = Side::Explorer;
        self.open_doc(Doc::File(path), pin, cx);
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
        let root = self.explore_root().map(PathBuf::from);
        let rows = root.as_deref().map(|root| self.explorer.rows(root)).unwrap_or_default();
        let new_row = root.as_deref().zip(self.explorer.new_in()).and_then(|(root, parent)| edit::new_row_at(&rows, parent, root));
        let renamed = self.explorer.renamed().map(Path::to_path_buf);
        let flashed = self.explorer.flash.as_ref().map(|(p, _)| p.clone());
        if self.explorer.seek
            && let Some(ix) = seek(&rows, new_row.map(|(i, _)| i), renamed.as_deref(), flashed.as_deref())
        {
            self.explorer.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
            self.explorer.seek = false;
        }
        let lines = rows.len() + usize::from(new_row.is_some());
        let list = uniform_list(
            "explorer",
            lines,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|ix| {
                        let line = match new_row {
                            Some((at, depth)) if ix == at => this.edit_row(depth, cx).unwrap_or_else(|| div().into_any_element()),
                            Some((at, _)) if ix > at => this.tree_row(rows[ix - 1].clone(), &touched, cx),
                            _ => this.tree_row(rows[ix].clone(), &touched, cx),
                        };
                        div().w_full().pb(px(1.)).flex().flex_col().child(line)
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.explorer.scroll)
        .size_full()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(8.));
        let list = div()
            .id("explorer")
            .relative()
            .flex_1()
            .min_h_0()
            .child(list)
            .vertical_scrollbar(&self.explorer.scroll)
            .on_mouse_down(MouseButton::Right, self.open_tree_menu(None, cx))
            .when(self.explorer.menu_on(None), |d| d.shadow(outline(ACCENT)));
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(list).children(self.tree_menu(cx))
    }

    fn tree_row(&self, Row { path, depth, is_dir, open }: Row, touched: &[String], cx: &mut Context<Self>) -> AnyElement {
        if self.explorer.renaming(&path)
            && let Some(row) = self.edit_row(depth, cx)
        {
            return row;
        }
        let label = name_of(&path);
        let key = path.to_string_lossy().to_string();
        let menu = self.open_tree_menu(Some((path.clone(), is_dir)), cx);
        let targeted = self.explorer.menu_on(Some(&path));
        let flash = self.explorer.flashing(&path).then(|| id(format!("flash-{key}")));
        let selected = !is_dir && self.preview.file(self.focused_pane()) == Some(key.as_str());
        let row = if !is_dir {
            let git = self.file_status(&key);
            let touched = touched.contains(&key);
            ui::tree_row(id(format!("tree-{key}")), label, false, false, depth, selected, touched, git)
                .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.open_file(key.clone(), ev.click_count() > 1, cx)))
        } else {
            ui::tree_row(id(format!("tree-{key}")), label, true, open, depth, false, false, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_folder(path.clone(), cx)))
        };
        let row = row.on_mouse_down(MouseButton::Right, menu).when(targeted, |d| d.shadow(outline(ACCENT)));
        let Some(flash) = flash else { return row.into_any_element() };
        row.with_animation(flash, Animation::new(FLASH).with_easing(ease_out_quint()), move |d, t| {
            let fade = |c: Token| Hsla::from(c).opacity(1. - t);
            d.when(!selected && t < 1., |d| d.bg(fade(ACCENT_BG))).shadow(outline(fade(ACCENT)))
        })
        .into_any_element()
    }
}

/// Which row to scroll to: the new item's, else the renamed or flashed one's.
pub fn seek(rows: &[Row], new_row: Option<usize>, renamed: Option<&Path>, flashed: Option<&Path>) -> Option<usize> {
    new_row.or_else(|| {
        let at = renamed.or(flashed)?;
        rows.iter().position(|r| r.path == at)
    })
}

fn name_of(path: &Path) -> String {
    crate::util::basename(&path.to_string_lossy())
}

/// A 1px ring inside a row's edge: the row the menu is for, or the one a name is typed in.
fn outline(color: impl Into<Hsla>) -> Vec<BoxShadow> {
    vec![BoxShadow { inset: true, ..ui::ring(color, 1.) }]
}

#[cfg(test)]
mod tests {
    use super::{ExplorerState, Row, list_dir, merge_tree, seek, status_of, touched};
    use store::prefs::files::Files;
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
    fn a_folder_lists_folders_first_then_folds_keeping_open_subfolders() {
        let dir = std::env::temp_dir().join(format!("pocket-explorer-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        let mut explorer = ExplorerState::new();
        explorer.tree.insert(dir.join("sub"), Vec::new());
        explorer.tree.insert(dir.clone(), list_dir(&dir, &Files::default()));
        assert_eq!(explorer.tree[&dir], [(true, dir.join("sub")), (false, dir.join("a.rs"))]);
        assert!(explorer.fold(&dir));
        assert!(!explorer.fold(&dir));
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

    #[test]
    fn the_new_row_is_scrolled_to_before_a_renamed_or_flashed_one() {
        let row = |p: &str| Row { path: p.into(), depth: 0, is_dir: false, open: false };
        let rows = [row("/r/a"), row("/r/b")];
        assert_eq!(seek(&rows, Some(0), None, Some(Path::new("/r/b"))), Some(0));
        assert_eq!(seek(&rows, None, Some(Path::new("/r/b")), Some(Path::new("/r/a"))), Some(1));
        assert_eq!(seek(&rows, None, None, Some(Path::new("/r/a"))), Some(0));
        assert_eq!(seek(&rows, None, None, Some(Path::new("/r/gone"))), None);
    }
}
