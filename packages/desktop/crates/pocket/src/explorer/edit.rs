use super::{ExplorerState, Row, files, outline};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use std::path::{Path, PathBuf};
use theme::*;

#[derive(Clone)]
pub enum Naming {
    New { parent: PathBuf, folder: bool },
    Rename { path: PathBuf, folder: bool },
}

impl Naming {
    fn parent(&self) -> &Path {
        match self {
            Naming::New { parent, .. } => parent,
            Naming::Rename { path, .. } => path.parent().unwrap_or(path),
        }
    }

    fn renaming(&self) -> Option<&Path> {
        match self {
            Naming::Rename { path, .. } => Some(path),
            Naming::New { .. } => None,
        }
    }
}

/// A name being typed into the tree: a new item's, in a row of its own, or a renamed one's, in its row.
pub(crate) struct NameEdit {
    naming: Naming,
    input: Entity<InputState>,
    _sub: Subscription,
}

/// How much of `name` starts selected: a file's name up to its extension, all of a folder's.
pub fn stem(name: &str, folder: bool) -> usize {
    if folder {
        return name.len();
    }
    name.rfind('.').filter(|&i| i > 0).unwrap_or(name.len())
}

/// Where a new item's row goes among `rows`, and how deep: first inside `parent`, which must be open.
pub fn new_row_at(rows: &[Row], parent: &Path, root: &Path) -> Option<(usize, usize)> {
    if parent == root {
        return Some((0, 0));
    }
    rows.iter().position(|r| r.path == parent && r.open).map(|i| (i + 1, rows[i].depth + 1))
}

impl ExplorerState {
    /// Why `name` can't be made in `parent`, judged by the folders listed so far; `renaming` is the item being renamed, which doesn't clash with itself.
    pub fn problem(&self, parent: &Path, name: &str, renaming: Option<&Path>, root: &Path) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if renaming.is_some() && name.contains('/') {
            return Some("A name can’t contain “/”".into());
        }
        let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Some("Needs a name".into());
        }
        if parts.iter().any(|p| *p == "." || *p == "..") {
            return Some("“.” and “..” are reserved".into());
        }
        let mut dir = parent.to_path_buf();
        for (i, part) in parts.iter().enumerate() {
            let (is_dir, hit) = self.tree.get(&dir)?.iter().find(|(_, p)| Some(p.as_path()) != renaming && name_of(p).to_lowercase() == part.to_lowercase())?;
            if i == parts.len() - 1 {
                let place = if dir == root { "the project".to_string() } else { format!("{}/", name_of(&dir)) };
                return Some(format!("“{}” already exists in {place}", name_of(hit)));
            }
            if !is_dir {
                return Some(format!("“{}” is a file, not a folder", name_of(hit)));
            }
            dir = hit.clone();
        }
        None
    }

    /// Opens the folders from `parent` down to the one holding `made`, so it shows once they're listed.
    pub fn reveal(&mut self, parent: &Path, made: &Path) {
        for dir in made.ancestors().skip(1).take_while(|d| d.starts_with(parent)) {
            self.tree.entry(dir.to_path_buf()).or_default();
        }
    }

    /// Points what the tree holds under `from` at `to`, after a rename.
    pub fn moved(&mut self, from: &Path, to: &Path) {
        let swap = |p: PathBuf| match p.strip_prefix(from) {
            Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
            Ok(rest) => to.join(rest),
            Err(_) => p,
        };
        self.tree = std::mem::take(&mut self.tree).into_iter().map(|(dir, listing)| (swap(dir), listing.into_iter().map(|(d, p)| (d, swap(p))).collect())).collect();
    }

    pub(crate) fn renaming(&self, path: &Path) -> bool {
        self.edit.as_ref().is_some_and(|e| e.naming.renaming() == Some(path))
    }

    pub(crate) fn new_parent(&self) -> Option<&Path> {
        match &self.edit.as_ref()?.naming {
            Naming::New { parent, .. } => Some(parent),
            Naming::Rename { .. } => None,
        }
    }

    /// Drops the name being typed unsaved; true when there was one.
    pub(crate) fn cancel_edit(&mut self) -> bool {
        self.edit.take().is_some()
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_string()
}

impl Desktop {
    pub(crate) fn start_edit(&mut self, naming: Naming, window: &mut Window, cx: &mut Context<Self>) {
        let (name, folder) = match &naming {
            Naming::New { folder, .. } => (String::new(), *folder),
            Naming::Rename { path, folder } => (name_of(path), *folder),
        };
        let placeholder = if folder { "Folder name" } else { "File name — / makes folders" };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let end = stem(&name, folder);
        input.update(cx, |s, cx| {
            s.set_value(name, window, cx);
            s.focus(window, cx);
            s.set_selected_range(0..end, cx);
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } => this.commit_edit(true, cx),
            InputEvent::Blur => this.commit_edit(false, cx),
            InputEvent::Change => cx.notify(),
            InputEvent::Focus => {}
        });
        if let Naming::New { parent, .. } = &naming
            && !self.explorer.tree.contains_key(parent)
        {
            self.explorer.tree.insert(parent.clone(), Vec::new());
            self.relist_explorer(cx);
        }
        self.explorer.edit = Some(NameEdit { naming, input, _sub: sub });
    }

    /// Saves the typed name. Enter (`strict`) keeps a refused name up to fix, and an emptied rename; leaving drops them.
    fn commit_edit(&mut self, strict: bool, cx: &mut Context<Self>) {
        let (Some(edit), Some(root)) = (self.explorer.edit.as_ref(), self.explore_root()) else { return };
        let name = edit.input.read(cx).value().trim().to_string();
        let renaming = edit.naming.renaming();
        if renaming.is_some_and(|p| name_of(p) == name) {
            self.explorer.edit = None;
            return cx.notify();
        }
        let problem = self.explorer.problem(edit.naming.parent(), &name, renaming, Path::new(&root));
        if name.is_empty() || problem.is_some() {
            if !(strict && (problem.is_some() || renaming.is_some())) {
                self.explorer.edit = None;
                cx.notify();
            }
            return;
        }
        let Some(edit) = self.explorer.edit.take() else { return };
        self.save_name(edit.naming, name, cx);
        cx.notify();
    }

    fn save_name(&mut self, naming: Naming, name: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let made = cx.background_executor().spawn({
                let (naming, name) = (naming.clone(), name.clone());
                async move {
                    match &naming {
                        Naming::New { parent, folder } => files::create(parent, &name, *folder),
                        Naming::Rename { path, .. } => files::rename(path, &name),
                    }
                }
            });
            let made = made.await;
            this.update(cx, |d, cx| {
                match (naming, made) {
                    (Naming::New { parent, folder }, Ok(path)) => {
                        d.explorer.reveal(&parent, &path);
                        if !folder && !name.ends_with('/') {
                            d.open_file(path.to_string_lossy().to_string(), false, cx);
                        }
                    }
                    (Naming::Rename { path, .. }, Ok(to)) => d.explorer.moved(&path, &to),
                    (Naming::New { .. }, Err(e)) => d.error = Some(format!("Couldn’t create “{name}”: {e}")),
                    (Naming::Rename { .. }, Err(e)) => d.error = Some(format!("Couldn’t rename to “{name}”: {e}")),
                }
                d.relist_explorer(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The row the name is typed in, laid out as the tree's rows are, with the reason it can't be saved under it.
    pub(crate) fn edit_row(&self, depth: usize, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let (edit, root) = (self.explorer.edit.as_ref()?, self.explore_root()?);
        let name = edit.input.read(cx).value().to_string();
        let folder = match edit.naming {
            Naming::New { folder, .. } | Naming::Rename { folder, .. } => folder || name.ends_with('/'),
        };
        let problem = self.explorer.problem(edit.naming.parent(), &name, edit.naming.renaming(), Path::new(&root));
        let row = div()
            .id("tree-edit")
            .relative()
            .h(px(28.))
            .pl(px(8. + depth as f32 * 16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .text_size(px(13.5))
            .shadow(outline(if problem.is_some() { FAILED } else { ACCENT }))
            .child(div().w(px(12.)))
            .child(file_icon(&name, folder, false, 16.))
            .child(div().flex_1().min_w_0().child(Input::new(&edit.input).appearance(false).p_0().text_size(px(13.5))))
            .children(problem.map(|p| ui::dropdown(32., ui::pop(div()).max_w(px(280.)).px(px(10.)).py(px(6.)).text_size(px(12.5)).text_color(FAILED).child(p))));
        Some(row)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExplorerState, Row, new_row_at, stem};
    use std::path::{Path, PathBuf};

    fn state(listings: &[(&str, &[(bool, &str)])]) -> ExplorerState {
        let mut s = ExplorerState::new();
        for (dir, listing) in listings {
            s.tree.insert(dir.into(), listing.iter().map(|(d, p)| (*d, PathBuf::from(p))).collect());
        }
        s
    }

    #[test]
    fn a_rename_starts_with_the_name_before_its_extension_selected() {
        assert_eq!(stem("explorer.rs", false), 8);
        assert_eq!(stem(".gitignore", false), 10);
        assert_eq!(stem("Makefile", false), 8);
        assert_eq!(stem("v1.2", true), 4);
    }

    #[test]
    fn a_name_already_in_the_folder_is_refused_whatever_its_case() {
        let s = state(&[("/r", &[(true, "/r/src"), (false, "/r/README.md")]), ("/r/src", &[(false, "/r/src/a.rs")])]);
        let root = Path::new("/r");
        assert_eq!(s.problem(root, "readme.md", None, root).as_deref(), Some("“README.md” already exists in the project"));
        assert_eq!(s.problem(root, "src/A.rs", None, root).as_deref(), Some("“a.rs” already exists in src/"));
        assert_eq!(s.problem(root, "README.md/x", None, root).as_deref(), Some("“README.md” is a file, not a folder"));
        assert_eq!(s.problem(root, "src/b.rs", None, root), None);
    }

    #[test]
    fn a_rename_may_change_only_its_case_but_never_hold_a_slash() {
        let s = state(&[("/r", &[(false, "/r/a.rs")])]);
        let root = Path::new("/r");
        assert_eq!(s.problem(root, "A.rs", Some(Path::new("/r/a.rs")), root), None);
        assert_eq!(s.problem(root, "x/a.rs", Some(Path::new("/r/a.rs")), root).as_deref(), Some("A name can’t contain “/”"));
    }

    #[test]
    fn dots_and_bare_slashes_are_not_names_and_an_empty_one_waits() {
        let s = state(&[("/r", &[])]);
        let root = Path::new("/r");
        assert_eq!(s.problem(root, "a/../b", None, root).as_deref(), Some("“.” and “..” are reserved"));
        assert_eq!(s.problem(root, "//", None, root).as_deref(), Some("Needs a name"));
        assert_eq!(s.problem(root, "  ", None, root), None);
    }

    #[test]
    fn a_folder_never_listed_raises_no_clash() {
        let s = state(&[("/r", &[(true, "/r/src")])]);
        assert_eq!(s.problem(Path::new("/r"), "src/a.rs", None, Path::new("/r")), None);
    }

    #[test]
    fn a_new_row_goes_first_in_its_folder() {
        let rows = [
            Row { path: "/r/src".into(), depth: 0, is_dir: true, open: true },
            Row { path: "/r/src/a.rs".into(), depth: 1, is_dir: false, open: false },
        ];
        assert_eq!(new_row_at(&rows, Path::new("/r"), Path::new("/r")), Some((0, 0)));
        assert_eq!(new_row_at(&rows, Path::new("/r/src"), Path::new("/r")), Some((1, 1)));
        assert_eq!(new_row_at(&rows, Path::new("/r/docs"), Path::new("/r")), None);
    }

    #[test]
    fn a_made_item_opens_the_folders_down_to_it() {
        let mut s = state(&[("/r", &[])]);
        s.reveal(Path::new("/r"), Path::new("/r/a/b/c.rs"));
        let mut open: Vec<_> = s.tree.keys().cloned().collect();
        open.sort();
        assert_eq!(open, [PathBuf::from("/r"), "/r/a".into(), "/r/a/b".into()]);
    }

    #[test]
    fn renaming_a_folder_keeps_what_is_open_inside_it() {
        let mut s = state(&[("/r", &[(true, "/r/src")]), ("/r/src", &[(true, "/r/src/ui")]), ("/r/src/ui", &[(false, "/r/src/ui/a.rs")])]);
        s.moved(Path::new("/r/src"), Path::new("/r/lib"));
        assert_eq!(s.tree[Path::new("/r")], [(true, PathBuf::from("/r/lib"))]);
        assert_eq!(s.tree[Path::new("/r/lib")], [(true, PathBuf::from("/r/lib/ui"))]);
        assert_eq!(s.tree[Path::new("/r/lib/ui")], [(false, PathBuf::from("/r/lib/ui/a.rs"))]);
        assert!(!s.tree.contains_key(Path::new("/r/src")));
    }
}
