pub(crate) mod edit;
pub(crate) mod files;
pub(crate) mod frontmatter;
pub(crate) mod menu;
pub(crate) mod mermaid;
pub(crate) mod preview;
pub(crate) mod toast;

use crate::actions::{TreeMenuArrow, TreeMenuChoose, TreeMenuItem};
use crate::add_to_chat::Choice;
use crate::chat::Route;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Side, id};
use crate::fs::fs_for;
use crate::util::{list_dir, list_dirs};
use git::{FileStat, Repo};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use store::prefs::files::Files;
use theme::*;
use workspace::Doc;

/// How long a row just made, renamed or put back stays ringed.
const FLASH: Duration = Duration::from_millis(900);

/// Fresh listings for the folders still open, so a folder opened or closed while they were read keeps its new state.
pub fn merge_tree(
    fresh: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    open: &HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    root: Option<&Path>,
) -> HashMap<PathBuf, Vec<(bool, PathBuf)>> {
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

/// The letter a folder at `dir` takes from the changed `files` inside it: theirs when they agree, else M.
pub fn folder_status(files: &[FileStat], root: &str, dir: &str) -> Option<char> {
    let rel = dir.strip_prefix(root)?.trim_start_matches('/');
    let mut inside = files.iter().filter(|f| f.path.strip_prefix(rel).is_some_and(|r| r.starts_with('/'))).map(|f| f.status);
    let first = inside.next()?;
    Some(if inside.all(|s| s == first) { first } else { 'M' })
}

/// What `Desktop` hands the tree before it draws or is called: the folder on screen, its changes, and what the menu reads of the rest of the app.
#[derive(Default)]
pub(crate) struct Inputs {
    pub(crate) root: Option<String>,
    /// The root's files are on this Mac.
    pub(crate) local: bool,
    pub(crate) repo: Option<Arc<Repo>>,
    pub(crate) files: Files,
    /// The file the focused pane previews.
    pub(crate) selected: Option<String>,
    /// The paths agents edited, as of the stamp.
    pub(crate) touched: (u64, Arc<HashSet<String>>),
    pub(crate) chats: Vec<Choice>,
    pub(crate) session: Option<String>,
    pub(crate) observe_only: bool,
}

/// What the tree asks `Desktop` for: what opens files and chats, closes other menus, or checks what it read against the tree on screen.
#[derive(Clone)]
pub(crate) enum ExplorerEvent {
    GoToFile,
    Open {
        path: String,
        pin: bool,
    },
    /// A right press on a row, or on the space under them for `None`; `Desktop` shuts the other menus, then asks for the tree's.
    MenuRequested {
        target: Option<(PathBuf, bool)>,
        at: Point<Pixels>,
        alt: bool,
        width: Pixels,
    },
    Mention {
        id: String,
        path: PathBuf,
        folder: bool,
    },
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    /// What the Trash took and where it went, `None` when it couldn't.
    Trashed {
        path: PathBuf,
        to: Option<PathBuf>,
    },
    Failed(String),
    /// A background read or write done; `Desktop` hands the tree what it reads of the app first, so the result is judged against the tree on screen now.
    Landed(Landing),
}

#[derive(Clone)]
pub(crate) enum Landing {
    Root { files: Files, root: PathBuf, listing: Option<Vec<(bool, PathBuf)>> },
    Relisted { files: Files, fresh: HashMap<PathBuf, Vec<(bool, PathBuf)>> },
    Folder { files: Files, dir: PathBuf, listing: Option<Vec<(bool, PathBuf)>> },
    Saved { naming: edit::Naming, name: String, made: Result<PathBuf, String> },
    Restored { from: PathBuf, open: Option<String>, restored: Result<(), String> },
}

pub(crate) struct Explorer {
    pub(crate) inputs: Inputs,
    pub(crate) tree: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    pub(crate) menu: Option<menu::TreeMenu>,
    /// The menu just closed, fading out, and when it closed.
    leaving: Option<(menu::TreeMenu, Instant)>,
    edit: Option<edit::NameEdit>,
    pub(crate) notice: Option<(toast::Notice, Task<()>)>,
    /// The last trash, while its notice shows.
    undo: Option<toast::Trashed>,
    flash: Option<(PathBuf, Task<()>)>,
    scroll: UniformListScrollHandle,
    /// Scroll the edit row, or else the flashed one, into view once it's listed.
    seek: bool,
}

impl EventEmitter<ExplorerEvent> for Explorer {}

/// A line of the tree: a file, or a folder and whether it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub open: bool,
}

impl Explorer {
    pub fn new() -> Self {
        Self {
            inputs: Inputs::default(),
            tree: HashMap::new(),
            menu: None,
            leaving: None,
            edit: None,
            notice: None,
            undo: None,
            flash: None,
            scroll: UniformListScrollHandle::new(),
            seek: false,
        }
    }

    /// The tree under `root` top to bottom, descending into open folders only.
    pub fn rows(&self, root: &Path) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_rows(root, 0, &mut rows);
        rows
    }

    /// The folders to list again: every open one, and the root.
    pub(crate) fn listed_dirs(&self, root: Option<&str>) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.tree.keys().cloned().collect();
        dirs.extend(root.map(PathBuf::from));
        dirs
    }

    /// Lists `dir` as `listing`; false when it already was.
    pub fn set_listing(&mut self, dir: PathBuf, listing: Vec<(bool, PathBuf)>) -> bool {
        if self.tree.get(&dir) == Some(&listing) {
            return false;
        }
        self.tree.insert(dir, listing);
        true
    }

    /// Takes fresh listings for the folders still open under `root`; whether the tree changed.
    pub(crate) fn land_tree(&mut self, fresh: HashMap<PathBuf, Vec<(bool, PathBuf)>>, root: Option<&Path>) -> bool {
        let tree = merge_tree(fresh, &self.tree, root);
        let changed = tree != self.tree;
        self.tree = tree;
        changed
    }

    /// Folds `dir`, keeping its open subfolders; false when it wasn't open.
    pub fn fold(&mut self, dir: &Path) -> bool {
        self.tree.remove(dir).is_some()
    }

    pub(crate) fn clear_tree(&mut self) {
        self.tree.clear();
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
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

    /// Whether `path` lies in the tree on screen, so what a slow write made there may be shown.
    pub(crate) fn in_root(&self, path: &Path) -> bool {
        self.inputs.root.as_deref().is_some_and(|r| path.starts_with(r))
    }

    /// Git's letter for a file under the root, if it has changed.
    fn file_status(&self, path: &str) -> Option<char> {
        status_of(&self.inputs.repo.as_ref()?.files, self.inputs.root.as_deref()?, path)
    }

    /// The letter a folder under the root takes from the changes inside it.
    fn folder_status(&self, path: &str) -> Option<char> {
        folder_status(&self.inputs.repo.as_ref()?.files, self.inputs.root.as_deref()?, path)
    }

    /// Rings `path`'s row for a moment and scrolls to it: what a create, rename or undo just made.
    pub(crate) fn flash_row(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let fade = cx.spawn(async |this, cx| {
            cx.background_executor().timer(FLASH).await;
            this.update(cx, |e, cx| {
                e.flash = None;
                cx.notify();
            })
            .ok();
        });
        self.flash = Some((path, fade));
        self.seek = true;
    }

    /// Lists the open folders again, as the root listing needs after new Explorer settings or a change on disk; landed only while those settings still hold.
    pub(crate) fn relist(&mut self, cx: &mut Context<Self>) {
        let files = self.inputs.files.clone();
        let dirs = self.listed_dirs(self.inputs.root.as_deref());
        cx.spawn(async move |this, cx| {
            let shown = files.clone();
            let fresh = cx.background_executor().spawn(async move { list_dirs(dirs, &shown) }).await;
            this.update(cx, |_, cx| cx.emit(ExplorerEvent::Landed(Landing::Relisted { files, fresh }))).ok();
        })
        .detach();
    }

    /// Lists only the folder on screen, so a switch shows it without waiting for `refresh_git` to read every tree.
    pub(crate) fn load_root(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.inputs.root.clone().map(PathBuf::from) else { return };
        let files = self.inputs.files.clone();
        cx.spawn(async move |this, cx| {
            let listing = cx.background_executor().spawn({
                let (root, files) = (root.clone(), files.clone());
                async move { list_dir(&root, &files) }
            });
            let listing = listing.await;
            this.update(cx, |_, cx| cx.emit(ExplorerEvent::Landed(Landing::Root { files, root, listing }))).ok();
        })
        .detach();
    }

    /// Folds `dir` if open, else lists it in the background and opens it, unless Explorer settings changed meanwhile.
    fn toggle_folder(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.fold(&dir) {
            return cx.notify();
        }
        let files = self.inputs.files.clone();
        cx.spawn(async move |this, cx| {
            let listing = cx.background_executor().spawn({
                let (dir, files) = (dir.clone(), files.clone());
                async move { list_dir(&dir, &files) }
            });
            let listing = listing.await;
            this.update(cx, |_, cx| cx.emit(ExplorerEvent::Landed(Landing::Folder { files, dir, listing }))).ok();
        })
        .detach();
    }

    /// Takes what a background read or write brought, judged against the tree on screen now.
    pub(crate) fn land(&mut self, landing: Landing, cx: &mut Context<Self>) {
        match landing {
            Landing::Root { files, root, listing } => {
                if let Some(listing) = listing
                    && self.inputs.files == files
                    && self.inputs.root.as_deref().map(Path::new) == Some(root.as_path())
                    && self.set_listing(root, listing)
                {
                    cx.notify();
                }
            }
            Landing::Relisted { files, fresh } => {
                if self.inputs.files != files {
                    return;
                }
                self.land_tree(fresh, self.inputs.root.clone().as_deref().map(Path::new));
                cx.notify();
            }
            Landing::Folder { files, dir, listing } => {
                if let Some(listing) = listing
                    && self.inputs.files == files
                {
                    self.tree.insert(dir, listing);
                    cx.notify();
                }
            }
            Landing::Saved { naming, name, made } => self.saved(naming, name, made, cx),
            Landing::Restored { from, open, restored } => self.restored(from, open, restored, cx),
        }
    }

    fn render_tree(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let go = ui::trigger_field("go-to-file", "search", "Go to file…", "⌘P")
            .mx(px(8.))
            .mt(px(8.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ExplorerEvent::GoToFile)));
        let root = self.inputs.root.clone().map(PathBuf::from);
        let rows = root.as_deref().map(|root| self.rows(root)).unwrap_or_default();
        let new_row = root.as_deref().zip(self.new_in()).and_then(|(root, parent)| edit::new_row_at(&rows, parent, root));
        let renamed = self.renamed().map(Path::to_path_buf);
        let flashed = self.flash.as_ref().map(|(p, _)| p.clone());
        if self.seek
            && let Some(ix) = seek(&rows, new_row.map(|(i, _)| i), renamed.as_deref(), flashed.as_deref())
        {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
            self.seek = false;
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
                            Some((at, _)) if ix > at => this.tree_row(rows[ix - 1].clone(), cx),
                            _ => this.tree_row(rows[ix].clone(), cx),
                        };
                        div().w_full().pb(px(1.)).flex().flex_col().child(line)
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
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
            .vertical_scrollbar(&self.scroll)
            .on_mouse_down(MouseButton::Right, self.open_tree_menu(None, cx))
            .when(self.menu_on(None), |d| d.shadow(outline(ACCENT)));
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(list).children(self.tree_menu(cx))
    }

    fn tree_row(&self, Row { path, depth, is_dir, open }: Row, cx: &mut Context<Self>) -> AnyElement {
        if self.renaming(&path)
            && let Some(row) = self.edit_row(depth, cx)
        {
            return row;
        }
        let label = name_of(&path);
        let key = path.to_string_lossy().to_string();
        let menu = self.open_tree_menu(Some((path.clone(), is_dir)), cx);
        let targeted = self.menu_on(Some(&path));
        let flash = self.flashing(&path).then(|| id(format!("flash-{key}")));
        let selected = !is_dir && self.inputs.selected.as_deref() == Some(key.as_str());
        let row = if !is_dir {
            let git = self.file_status(&key);
            let touched = git.is_some() && self.inputs.touched.1.contains(&key);
            ui::tree_row(id(format!("tree-{key}")), label, false, false, depth, selected, touched, git)
                .on_click(cx.listener(move |_, ev: &ClickEvent, _, cx| cx.emit(ExplorerEvent::Open { path: key.clone(), pin: ev.click_count() > 1 })))
        } else {
            ui::tree_row(id(format!("tree-{key}")), label, true, open, depth, false, false, self.folder_status(&key))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_folder(path.clone(), cx)))
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

impl Render for Explorer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_tree(cx)
    }
}

impl Desktop {
    /// The folder Explore browses: the chosen tree's folder, else the project.
    pub fn explore_root(&self) -> Option<String> {
        self.worktree.as_deref().and_then(|t| self.folder_of(t)).or_else(|| self.project.clone())
    }

    /// The folder Explore browses, while its files are on this Mac.
    pub(crate) fn local_explore_root(&self) -> Option<String> {
        self.explore_root().filter(|root| fs_for(root).local().is_ok())
    }

    /// Git's letter for a file under the explore root, if it has changed.
    pub fn file_status(&self, path: &str) -> Option<char> {
        let root = self.explore_root()?;
        status_of(&self.repos.get(&root)?.files, &root, path)
    }

    /// Hands the tree what it reads of `Desktop`; before it draws and before anything calls into it.
    pub(crate) fn sync_explorer(&mut self, cx: &mut Context<Self>) {
        let root = self.explore_root();
        let repo = root.as_ref().and_then(|r| self.repos.get(r)).cloned();
        let local = root.as_deref().is_some_and(|r| fs_for(r).local().is_ok());
        let selected = self.preview.read(cx).file(self.focused_pane()).map(str::to_string);
        let stamp = self.agents.edits_stamp();
        let (menu_open, touched_at) = {
            let e = self.explorer.read(cx);
            (e.menu_open(), e.inputs.touched.0)
        };
        let touched = (touched_at != stamp).then(|| (stamp, Arc::new(self.agents.edited_paths())));
        let chats = menu_open.then(|| self.chat_choices());
        let (session, observe_only, files) = (self.session().map(str::to_string), self.agents.observe_only(), self.store.files.clone());
        self.explorer.update(cx, |e, _| {
            let i = &mut e.inputs;
            (i.root, i.repo, i.local, i.selected, i.files, i.session, i.observe_only) = (root, repo, local, selected, files, session, observe_only);
            if let Some(touched) = touched {
                i.touched = touched;
            }
            if let Some(chats) = chats {
                i.chats = chats;
            }
        });
    }

    pub(crate) fn explorer_view(&mut self, cx: &mut Context<Self>) -> Entity<Explorer> {
        self.sync_explorer(cx);
        self.explorer.clone()
    }

    pub(crate) fn load_explorer_root(&mut self, cx: &mut Context<Self>) {
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.load_root(cx));
    }

    pub(crate) fn relist_explorer(&mut self, cx: &mut Context<Self>) {
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.relist(cx));
    }

    pub(crate) fn on_explorer_event(&mut self, _: &Entity<Explorer>, ev: &ExplorerEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev.clone() {
            ExplorerEvent::GoToFile => self.go_to_file(&crate::actions::GoToFile, window, cx),
            ExplorerEvent::Open { path, pin } => self.open_file(path, pin, cx),
            ExplorerEvent::MenuRequested { target, at, alt, width } => {
                // The menu's key equivalents bind on the window; a focused terminal or page would take the keys first.
                window.focus(&self.root, cx);
                self.close_menus(cx);
                self.sync_explorer(cx);
                self.explorer.update(cx, |e, cx| e.open_menu(target, at, alt, width, cx));
            }
            ExplorerEvent::Mention { id, path, folder } => self.mention(&id, &path, folder, window, cx),
            ExplorerEvent::Renamed { from, to } => self.tabs_follow(&from, &to, cx),
            ExplorerEvent::Trashed { path, to } => {
                let trashed = to.map(|to| (to, self.close_trashed(&path, cx)));
                self.explorer.update(cx, |e, cx| e.trashed(path, trashed, cx));
            }
            ExplorerEvent::Failed(message) => {
                self.error = Some(message);
                cx.notify();
            }
            ExplorerEvent::Landed(landing) => {
                self.sync_explorer(cx);
                self.explorer.update(cx, |e, cx| e.land(landing, cx));
            }
        }
    }

    pub(crate) fn tree_menu_item(&mut self, item: &TreeMenuItem, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.choose_in_tree_menu(menu::Pick::Item(item.0), window, cx));
    }

    pub(crate) fn tree_menu_choose(&mut self, _: &TreeMenuChoose, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.tree_menu_choose(window, cx));
    }

    pub(crate) fn tree_menu_arrow(&mut self, arrow: &TreeMenuArrow, _: &mut Window, cx: &mut Context<Self>) {
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.tree_menu_arrow(arrow.0, cx));
    }

    /// Takes an unmodified letter as typeahead while the menu is open, and any key while it blinks; returns whether it did.
    pub(crate) fn type_tree_menu(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if !self.explorer.read(cx).menu_open() {
            return false;
        }
        self.sync_explorer(cx);
        self.explorer.update(cx, |e, cx| e.type_tree_menu(key, cx))
    }

    fn mention(&mut self, id: &str, path: &Path, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((terminal, provider, cwd)) = self.agents.get(id).map(|a| (a.terminal_id.clone(), a.provider.clone(), a.cwd.clone())) else { return };
        let text = menu::mention_text(path, folder, Path::new(&cwd));
        if self.route(&terminal) == Route::Chat {
            self.chat_add_text(id, &text, window, cx);
        } else {
            let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
            self.send_input(&terminal, &term::paste_bytes(&text, bracketed), cx);
        }
        // The focused chat shows the mention landing in its input; any other gets a toast.
        if self.session() != Some(id) {
            let title = self.chat_choices().into_iter().find(|c| c.id == id).map(|c| c.title).unwrap_or_default();
            self.show_notice(toast::Notice::Mentioned { provider, title }, cx);
        }
    }

    pub fn open_file(&mut self, path: String, pin: bool, cx: &mut Context<Self>) {
        self.side = Side::Explorer;
        self.open_doc(Doc::File(path), pin, cx);
    }

    pub fn go_to_file(&mut self, _: &crate::actions::GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.update(cx, |p, _| p.search_project());
        self.open(Overlay::Palette, window, cx);
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
    use super::{Explorer, Row, folder_status, list_dir, list_dirs, merge_tree, seek, status_of};
    use git::FileStat;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use store::prefs::files::Files;

    fn stat(path: &str, status: char) -> FileStat {
        FileStat { path: path.into(), added: 0, removed: 0, staged: false, unstaged: true, status, old_path: None }
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
    fn a_folder_takes_the_letter_its_changes_share_or_else_m() {
        let files = [stat("new/a.rs", 'A'), stat("new/deep/b.rs", 'A'), stat("src/a.rs", 'A'), stat("src/b.rs", 'D')];
        assert_eq!(folder_status(&files, "/r", "/r/new"), Some('A'));
        assert_eq!(folder_status(&files, "/r", "/r/new/deep"), Some('A'));
        assert_eq!(folder_status(&files, "/r", "/r/src"), Some('M'));
    }

    #[test]
    fn a_folder_with_no_changes_inside_takes_no_letter_even_beside_a_like_named_one() {
        let files = [stat("src2/a.rs", 'M')];
        assert_eq!(folder_status(&files, "/r", "/r/src"), None);
        assert_eq!(folder_status(&files, "/r", "/elsewhere/src2"), None);
    }

    fn entry(is_dir: bool, path: &str) -> (bool, PathBuf) {
        (is_dir, PathBuf::from(path))
    }

    fn row(path: &str, depth: usize, is_dir: bool, open: bool) -> Row {
        Row { path: PathBuf::from(path), depth, is_dir, open }
    }

    #[test]
    fn tree_nests_what_an_open_folder_holds_and_hides_what_a_folded_one_holds() {
        let mut explorer = Explorer::new();
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
    fn a_host_that_cannot_be_reached_keeps_the_folders_it_listed() {
        let dir = PathBuf::from("//devbox/home/me/app");
        let open = HashMap::from([(dir.clone(), vec![(false, dir.join("a.rs"))])]);
        assert_eq!(merge_tree(list_dirs(vec![dir.clone()], &Files::default()), &open, Some(&dir)), open);
    }

    #[test]
    fn a_folder_lists_folders_first_then_folds_keeping_open_subfolders() {
        let dir = std::env::temp_dir().join(format!("pocket-explorer-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        let mut explorer = Explorer::new();
        explorer.tree.insert(dir.join("sub"), Vec::new());
        explorer.tree.insert(dir.clone(), list_dir(&dir, &Files::default()).unwrap());
        assert_eq!(explorer.tree[&dir], [(true, dir.join("sub")), (false, dir.join("a.rs"))]);
        assert!(explorer.fold(&dir));
        assert!(!explorer.fold(&dir));
        assert_eq!(explorer.tree.keys().collect::<Vec<_>>(), [&dir.join("sub")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_a_folder_as_it_already_is_changes_nothing() {
        let mut explorer = Explorer::new();
        explorer.tree.insert("/r/sub".into(), Vec::new());
        assert!(explorer.set_listing("/r".into(), vec![entry(false, "/r/a.rs")]));
        assert!(!explorer.set_listing("/r".into(), vec![entry(false, "/r/a.rs")]));
        assert!(explorer.set_listing("/r".into(), Vec::new()));
        assert!(explorer.tree.contains_key(Path::new("/r/sub")));
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
