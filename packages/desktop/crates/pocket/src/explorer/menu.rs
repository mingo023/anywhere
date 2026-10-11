use super::edit::Naming;
use super::toast::Notice;
use crate::actions::TreeMenuItem;
use crate::add_to_chat::Choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use theme::*;

/// Set on the window while the menu is open, so its key equivalents win over the app's.
pub(crate) const CONTEXT: &str = "TreeMenu";
const MENU_W: f32 = 230.;
const SUB_W: f32 = 240.;
const GAP: f32 = 4.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Mention,
    NewFile,
    NewFolder,
    Copy,
    CopyRelative,
    Rename,
    Trash,
}

/// Where Mention sends without asking.
#[derive(Debug, PartialEq)]
pub enum Plan {
    To(String),
    Ask,
    Off,
}

/// The focused chat while it can take text, else the only one that can; several leave it to a pick, none turns it off.
pub fn plan(focused: Option<&str>, choices: &[Choice]) -> Plan {
    let ready: Vec<&Choice> = choices.iter().filter(|c| c.ready).collect();
    match ready.iter().find(|c| Some(c.id.as_str()) == focused).or(ready.first().filter(|_| ready.len() == 1)) {
        Some(c) => Plan::To(c.id.clone()),
        None if ready.len() > 1 => Plan::Ask,
        None => Plan::Off,
    }
}

/// The menu for a row, or the root's when `row` is false; `None` is a divider. ⌥ swaps Copy Path for the relative one.
pub fn items(row: bool, alt: bool) -> Vec<Option<Item>> {
    let copy = if alt { Item::CopyRelative } else { Item::Copy };
    let mut out = vec![];
    if row {
        out.extend([Some(Item::Mention), None]);
    }
    out.extend([Some(Item::NewFile), Some(Item::NewFolder), None, Some(copy)]);
    if row {
        out.extend([Some(Item::Rename), None, Some(Item::Trash)]);
    }
    out
}

/// What a mention types: `path` from the agent's folder, a folder's with a trailing "/", then a space.
pub fn mention_text(path: &Path, folder: bool, cwd: &Path) -> String {
    let shown = path.strip_prefix(cwd).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string_lossy().to_string());
    let shown = if shown.is_empty() { ".".to_string() } else { shown };
    format!("@{shown}{} ", if folder && shown != "." { "/" } else { "" })
}

/// `path` from `root`; the root itself is ".".
pub fn relative(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root).map(|p| p.to_string_lossy().to_string()) {
        Ok(p) if p.is_empty() => ".".into(),
        Ok(p) => p,
        Err(_) => path.to_string_lossy().to_string(),
    }
}

/// The right-click menu over the file tree.
pub(crate) struct TreeMenu {
    /// The row and whether it's a folder; `None` is the root, from the space under the rows.
    pub(crate) target: Option<(PathBuf, bool)>,
    at: Point<Pixels>,
    /// The chat list opens left of the menu, there being no room right.
    flip: bool,
    pub(crate) alt: bool,
    sub: bool,
}

impl TreeMenu {
    /// A menu at `at` in a window `width` wide, closed chat list.
    pub(crate) fn new(target: Option<(PathBuf, bool)>, at: Point<Pixels>, alt: bool, width: Pixels) -> Self {
        let flip = f32::from(at.x) + MENU_W + GAP + SUB_W + 8. > f32::from(width);
        Self { target, at, flip, alt, sub: false }
    }
}

impl Desktop {
    pub(crate) fn open_tree_menu(&self, target: Option<(PathBuf, bool)>, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.close_menus();
            this.explorer.menu = Some(TreeMenu::new(target.clone(), e.position, window.modifiers().alt, window.viewport_size().width));
            cx.notify();
        })
    }

    fn mention_plan(&self) -> Plan {
        if self.agents.observe_only() {
            return Plan::Off;
        }
        plan(self.session(), &self.chat_choices())
    }

    pub(crate) fn tree_menu_item(&mut self, item: &TreeMenuItem, window: &mut Window, cx: &mut Context<Self>) {
        self.run_tree_item(item.0, window, cx);
    }

    fn run_tree_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(menu), Some(root)) = (self.explorer.menu.as_ref(), self.explore_root().map(PathBuf::from)) else { return };
        let target = menu.target.clone();
        let (path, folder) = target.clone().unwrap_or((root.clone(), true));
        match item {
            Item::Mention => {
                let Some((path, folder)) = target else { return };
                match self.mention_plan() {
                    Plan::To(id) => self.mention(&id, &path, folder, cx),
                    Plan::Ask => {
                        if let Some(menu) = self.explorer.menu.as_mut() {
                            menu.sub = true;
                        }
                        return cx.notify();
                    }
                    Plan::Off => return,
                }
            }
            Item::NewFile | Item::NewFolder => {
                let parent = if folder { path } else { path.parent().map(Path::to_path_buf).unwrap_or(root) };
                self.start_edit(Naming::New { parent, folder: item == Item::NewFolder }, window, cx);
            }
            Item::Copy | Item::CopyRelative => {
                let text = if item == Item::Copy { path.to_string_lossy().to_string() } else { relative(&path, &root) };
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                self.show_notice(Notice::Copied(text), cx);
            }
            Item::Rename if target.is_some() => self.start_edit(Naming::Rename { path, folder }, window, cx),
            Item::Trash if target.is_some() => self.trash_file(path, cx),
            Item::Rename | Item::Trash => return,
        }
        self.explorer.menu = None;
        cx.notify();
    }

    fn mention(&mut self, id: &str, path: &Path, folder: bool, cx: &mut Context<Self>) {
        let Some((terminal, provider, cwd)) = self.agents.get(id).map(|a| (a.terminal_id.clone(), a.provider.clone(), a.cwd.clone())) else { return };
        let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
        self.send_input(&terminal, &term::paste_bytes(&mention_text(path, folder, Path::new(&cwd)), bracketed), cx);
        // The focused chat shows the mention landing in its input; any other gets a toast.
        if self.session() != Some(id) {
            self.flash_added(provider, cx);
        }
    }

    fn set_sub(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(menu) = self.explorer.menu.as_mut().filter(|m| m.sub != open) {
            menu.sub = open;
            cx.notify();
        }
    }

    pub(crate) fn tree_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.explorer.menu.as_ref()?;
        let plan = if menu.target.is_some() { self.mention_plan() } else { Plan::Off };
        let rows = items(menu.target.is_some(), menu.alt).into_iter().map(|item| match item {
            None => ui::menu_divider().into_any_element(),
            Some(item) => self.tree_menu_row(item, &plan, cx),
        });
        let main = ui::pop(div().id("tree-menu")).w(px(MENU_W)).p(px(6.)).flex().flex_col().children(rows);
        let sub = (menu.sub && plan != Plan::Off).then(|| self.chat_list(&plan, cx));
        let x = if menu.flip && sub.is_some() { menu.at.x - px(SUB_W + GAP) } else { menu.at.x };
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.explorer.menu = None;
            cx.notify();
        });
        let body = div().id("tree-menu-box").flex().items_start().gap(px(GAP)).when(menu.flip, |d| d.flex_row_reverse()).occlude().on_mouse_down_out(close).child(main).children(sub);
        Some(deferred(anchored().position(point(x, menu.at.y)).snap_to_window_with_margin(px(8.)).child(ui::menu_in("tree-menu-in", body))).with_priority(1))
    }

    fn tree_menu_row(&self, item: Item, plan: &Plan, cx: &mut Context<Self>) -> AnyElement {
        let id = format!("tree-menu-{item:?}");
        let row = match item {
            Item::Mention => ui::menu_row(id, "comment", "Mention in", None),
            Item::NewFile => ui::menu_row(id, "file", "New File…", None),
            Item::NewFolder => ui::menu_row(id, "folder", "New Folder…", None),
            Item::Copy => ui::menu_row(id, "copy", "Copy Path", Some("⌥⌘C")),
            Item::CopyRelative => ui::menu_row(id, "copy", "Copy Relative Path", Some("⌥⇧⌘C")),
            Item::Rename => ui::menu_row(id, "pencil", "Rename…", Some("↵")),
            Item::Trash => ui::danger_row(id, "trash", "Move to Trash").child(ui::kbd("⌘⌫")),
        };
        if item == Item::Mention && *plan == Plan::Off {
            return row.opacity(0.4).cursor_default().hover(|s| s).into_any_element();
        }
        row.when(item == Item::Mention, |d| d.child(icon("chevron-right", 12., TEXT_3)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.set_sub(item == Item::Mention, cx);
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run_tree_item(item, window, cx)))
            .into_any_element()
    }

    fn chat_list(&self, plan: &Plan, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows = self.chat_choices().into_iter().enumerate().map(|(i, c)| {
            let trailing = if !c.ready {
                Some(div().text_size(px(12.)).text_color(TEXT_3).child("Needs you"))
            } else {
                (*plan == Plan::To(c.id.clone())).then(|| ui::kbd("⌘L"))
            };
            div()
                .id(("tree-chat", i))
                .h(px(32.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .text_size(px(13.5))
                .child(provider_icon(&c.provider, 14., TEXT_2))
                .child(div().flex_1().min_w_0().truncate().child(c.title.clone()))
                .children(trailing)
                .when(!c.ready, |d| d.opacity(0.5))
                .when(c.ready, |d| {
                    d.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let Some((path, folder)) = this.explorer.menu.take().and_then(|m| m.target) else { return };
                        this.mention(&c.id, &path, folder, cx);
                        cx.notify();
                    }))
                })
        });
        ui::pop(div().id("tree-chats")).w(px(SUB_W)).p(px(6.)).flex().flex_col().children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, Plan, items, mention_text, plan, relative};
    use crate::add_to_chat::Choice;
    use std::path::Path;

    fn chat(id: &str, ready: bool) -> Choice {
        Choice { id: id.into(), provider: "claude".into(), title: id.into(), note: String::new(), ready }
    }

    #[test]
    fn mention_goes_to_the_focused_chat_while_it_can_take_text() {
        assert_eq!(plan(Some("b"), &[chat("a", true), chat("b", true)]), Plan::To("b".into()));
    }

    #[test]
    fn without_a_ready_focused_chat_mention_goes_to_the_only_ready_one() {
        assert_eq!(plan(None, &[chat("a", false), chat("b", true)]), Plan::To("b".into()));
        assert_eq!(plan(Some("a"), &[chat("a", false), chat("b", true)]), Plan::To("b".into()));
    }

    #[test]
    fn several_ready_chats_ask_and_none_turns_mention_off() {
        assert_eq!(plan(None, &[chat("a", true), chat("b", true)]), Plan::Ask);
        assert_eq!(plan(None, &[chat("a", false)]), Plan::Off);
        assert_eq!(plan(None, &[]), Plan::Off);
    }

    #[test]
    fn the_root_menu_only_creates_and_copies() {
        assert_eq!(items(false, false), [Some(Item::NewFile), Some(Item::NewFolder), None, Some(Item::Copy)]);
    }

    #[test]
    fn a_row_menu_mentions_first_and_trashes_last_and_alt_copies_the_relative_path() {
        let row = items(true, true);
        assert_eq!(row.first(), Some(&Some(Item::Mention)));
        assert_eq!(row.last(), Some(&Some(Item::Trash)));
        assert!(row.contains(&Some(Item::CopyRelative)) && !row.contains(&Some(Item::Copy)));
    }

    #[test]
    fn a_mention_names_the_path_from_the_agent_s_folder() {
        assert_eq!(mention_text(Path::new("/r/src/a.rs"), false, Path::new("/r")), "@src/a.rs ");
        assert_eq!(mention_text(Path::new("/r/src"), true, Path::new("/r")), "@src/ ");
        assert_eq!(mention_text(Path::new("/r"), true, Path::new("/r")), "@. ");
        assert_eq!(mention_text(Path::new("/elsewhere/a.rs"), false, Path::new("/r")), "@/elsewhere/a.rs ");
    }

    #[test]
    fn a_relative_path_is_from_the_root_and_the_root_is_a_dot() {
        assert_eq!(relative(Path::new("/r/src/a.rs"), Path::new("/r")), "src/a.rs");
        assert_eq!(relative(Path::new("/r"), Path::new("/r")), ".");
    }
}
