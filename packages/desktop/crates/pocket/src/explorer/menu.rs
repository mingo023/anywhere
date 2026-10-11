use super::edit::Naming;
use super::toast::Notice;
use super::ExplorerState;
use crate::actions::{TreeMenuChoose, TreeMenuItem, TreeMenuStep};
use crate::add_to_chat::Choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use theme::*;

/// Set on the window while the menu is open, so its key equivalents win over the app's.
pub(crate) const CONTEXT: &str = "TreeMenu";
const MENU_W: f32 = 230.;
const SUB_W: f32 = 240.;
const GAP: f32 = 4.;
/// How long a chosen item blinks before it runs, as a macOS menu's does.
const BLINK: Duration = Duration::from_millis(170);
const LEAVE: Duration = Duration::from_millis(110);
/// Letters typed within this of each other spell one prefix.
const TYPEAHEAD: Duration = Duration::from_millis(700);

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

pub fn label(item: Item) -> &'static str {
    match item {
        Item::Mention => "Mention in",
        Item::NewFile => "New File…",
        Item::NewFolder => "New Folder…",
        Item::Copy => "Copy Path",
        Item::CopyRelative => "Copy Relative Path",
        Item::Rename => "Rename…",
        Item::Trash => "Move to Trash",
    }
}

/// Whether letting go of the right button on an item chooses it: the press that opened the menu was held, or dragged onto the item.
pub fn held(elapsed: Duration, from: Point<Pixels>, to: Point<Pixels>) -> bool {
    elapsed > Duration::from_millis(250) || f32::from(to.x - from.x).hypot(f32::from(to.y - from.y)) > 4.
}

/// What a mention types: `path` from the agent's folder, a folder's with a trailing "/", then a space.
pub fn mention_text(path: &Path, folder: bool, cwd: &Path) -> String {
    let shown = relative(path, cwd);
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

/// Where New File and New Folder make the item: in the folder clicked, else beside the file, else at the root.
pub fn new_parent(target: Option<&(PathBuf, bool)>, root: &Path) -> PathBuf {
    match target {
        Some((dir, true)) => dir.clone(),
        Some((file, false)) => file.parent().unwrap_or(root).to_path_buf(),
        None => root.to_path_buf(),
    }
}

#[derive(Clone)]
pub(crate) struct TreeMenu {
    /// The row and whether it's a folder; `None` is the root, from the space under the rows.
    pub(crate) target: Option<(PathBuf, bool)>,
    at: Point<Pixels>,
    /// The chat list opens left of the menu, there being no room right.
    flip: bool,
    pub(crate) alt: bool,
    sub: bool,
    /// The item arrows, typing or the pointer last landed on.
    hi: Option<Item>,
    typed: String,
    typed_at: Instant,
    opened: Instant,
    /// The item blinking before it runs; the menu takes no more input meanwhile.
    chosen: Option<Item>,
}

impl TreeMenu {
    /// A menu at `at` in a window `width` wide, closed chat list.
    pub(crate) fn new(target: Option<(PathBuf, bool)>, at: Point<Pixels>, alt: bool, width: Pixels) -> Self {
        let flip = f32::from(at.x) + MENU_W + GAP + SUB_W + 8. > f32::from(width);
        let now = Instant::now();
        Self { target, at, flip, alt, sub: false, hi: None, typed: String::new(), typed_at: now, opened: now, chosen: None }
    }

    /// The items that can be chosen, top to bottom.
    fn live(&self, mention_off: bool) -> Vec<Item> {
        items(self.target.is_some(), self.alt).into_iter().flatten().filter(|i| !(mention_off && *i == Item::Mention)).collect()
    }

    /// Whether `item` can run from this menu; either Copy Path runs by its key, whichever ⌥ shows.
    fn offers(&self, item: Item, mention_off: bool) -> bool {
        matches!(item, Item::Copy | Item::CopyRelative) || self.live(mention_off).contains(&item)
    }

    /// Highlights the next live item down or up, wrapping; from none, down starts at the first and up at the last.
    pub fn step(&mut self, down: bool, mention_off: bool) {
        let live = self.live(mention_off);
        let n = live.len();
        let next = match live.iter().position(|i| Some(*i) == self.hi) {
            Some(at) if down => (at + 1) % n,
            Some(at) => (at + n - 1) % n,
            None if down => 0,
            None => n - 1,
        };
        self.hi = Some(live[next]);
    }

    /// Highlights the first live item whose label starts with the letters typed so far.
    pub fn type_key(&mut self, key: &str, now: Instant, mention_off: bool) {
        if now.duration_since(self.typed_at) > TYPEAHEAD {
            self.typed.clear();
        }
        self.typed.push_str(&key.to_lowercase());
        self.typed_at = now;
        if let Some(item) = self.live(mention_off).into_iter().find(|i| label(*i).to_lowercase().starts_with(&self.typed)) {
            self.hi = Some(item);
        }
    }
}

impl ExplorerState {
    /// Closes the menu, leaving it to fade out.
    pub(crate) fn close_menu(&mut self) -> Option<TreeMenu> {
        let menu = self.menu.take();
        self.leaving = menu.clone().map(|m| (m, Instant::now()));
        menu
    }
}

impl Desktop {
    pub(crate) fn open_tree_menu(&self, target: Option<(PathBuf, bool)>, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.commit_edit(false, cx);
            // The menu's key equivalents bind on the window; a focused terminal or page would take the keys first.
            window.focus(&this.root, cx);
            this.close_menus();
            this.explorer.leaving = None;
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
        self.choose_tree_item(item.0, window, cx);
    }

    pub(crate) fn tree_menu_choose(&mut self, _: &TreeMenuChoose, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.explorer.menu.as_ref() else { return };
        self.choose_tree_item(menu.hi.unwrap_or(Item::Rename), window, cx);
    }

    pub(crate) fn tree_menu_step(&mut self, step: &TreeMenuStep, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.mention_plan() == Plan::Off;
        if let Some(menu) = self.explorer.menu.as_mut().filter(|m| m.chosen.is_none()) {
            menu.step(step.0, off);
            cx.notify();
        }
    }

    /// Takes an unmodified letter as typeahead while the menu is open; returns whether it did.
    pub(crate) fn type_tree_menu(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let m = key.modifiers;
        if self.explorer.menu.is_none() || m.platform || m.control || m.alt || m.function || key.key.chars().count() != 1 {
            return false;
        }
        let off = self.mention_plan() == Plan::Off;
        if let Some(menu) = self.explorer.menu.as_mut().filter(|m| m.chosen.is_none()) {
            menu.type_key(&key.key, Instant::now(), off);
            cx.notify();
        }
        true
    }

    /// Blinks `item`, then runs it; a Mention that asks which chat opens the list at once.
    fn choose_tree_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.mention_plan();
        let Some(menu) = self.explorer.menu.as_mut().filter(|m| m.chosen.is_none() && m.offers(item, plan == Plan::Off)) else { return };
        if cx.reduce_motion() || (item == Item::Mention && plan == Plan::Ask) {
            return self.run_tree_item(item, window, cx);
        }
        (menu.chosen, menu.hi) = (Some(item), Some(item));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(BLINK).await;
            this.update_in(cx, |d, window, cx| d.run_tree_item(item, window, cx)).ok();
        })
        .detach();
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
            Item::NewFile | Item::NewFolder => self.start_edit(Naming::New { parent: new_parent(target.as_ref(), &root), folder: item == Item::NewFolder }, window, cx),
            Item::Copy | Item::CopyRelative => {
                let text = if item == Item::Copy { path.to_string_lossy().to_string() } else { relative(&path, &root) };
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                self.show_notice(Notice::Copied(text), cx);
            }
            Item::Rename if target.is_some() => self.start_edit(Naming::Rename { path, folder }, window, cx),
            Item::Trash if target.is_some() => self.trash_file(path, cx),
            Item::Rename | Item::Trash => return,
        }
        self.explorer.close_menu();
        cx.notify();
    }

    fn mention(&mut self, id: &str, path: &Path, folder: bool, cx: &mut Context<Self>) {
        let Some((terminal, provider, cwd)) = self.agents.get(id).map(|a| (a.terminal_id.clone(), a.provider.clone(), a.cwd.clone())) else { return };
        let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
        self.send_input(&terminal, &term::paste_bytes(&mention_text(path, folder, Path::new(&cwd)), bracketed), cx);
        // The focused chat shows the mention landing in its input; any other gets a toast.
        if self.session() != Some(id) {
            let title = self.chat_choices().into_iter().find(|c| c.id == id).map(|c| c.title).unwrap_or_default();
            self.show_notice(Notice::Mentioned { provider, title }, cx);
        }
    }

    fn hover_tree_item(&mut self, item: Item, hovered: bool, cx: &mut Context<Self>) {
        let Some(menu) = self.explorer.menu.as_mut().filter(|m| m.chosen.is_none()) else { return };
        if hovered {
            (menu.hi, menu.sub) = (Some(item), item == Item::Mention);
        } else if menu.hi == Some(item) {
            menu.hi = None;
        }
        cx.notify();
    }

    /// The open menu, or the one just closed while it fades.
    pub(crate) fn tree_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(menu) = &self.explorer.menu {
            return Some(self.menu_body(menu, true, cx));
        }
        let (menu, _) = self.explorer.leaving.as_ref().filter(|(_, at)| at.elapsed() < LEAVE && !cx.reduce_motion())?;
        Some(self.menu_body(menu, false, cx))
    }

    /// The menu and its chat list; `live` takes input, else it's the closed menu fading out.
    fn menu_body(&self, menu: &TreeMenu, live: bool, cx: &mut Context<Self>) -> AnyElement {
        let plan = if menu.target.is_some() { self.mention_plan() } else { Plan::Off };
        let rows = items(menu.target.is_some(), menu.alt).into_iter().map(|item| match item {
            None => ui::menu_divider().into_any_element(),
            Some(item) => self.tree_menu_row(item, &plan, menu, live, cx),
        });
        let main = ui::pop(div().id("tree-menu")).w(px(MENU_W)).p(px(6.)).flex().flex_col().children(rows);
        let sub = (menu.sub && plan != Plan::Off).then(|| self.chat_list(&plan, live, cx));
        let x = if menu.flip && sub.is_some() { menu.at.x - px(SUB_W + GAP) } else { menu.at.x };
        let body = div().id(if live { "tree-menu-box" } else { "tree-menu-leaving" }).flex().items_start().gap(px(GAP)).when(menu.flip, |d| d.flex_row_reverse()).child(main).children(sub);
        let body = if live {
            let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.explorer.close_menu();
                cx.notify();
            });
            ui::menu_in("tree-menu-in", body.occlude().on_mouse_down_out(close)).into_any_element()
        } else {
            body.with_animation("tree-menu-out", Animation::new(LEAVE), |d, t| d.opacity(1. - t)).into_any_element()
        };
        deferred(anchored().position(point(x, menu.at.y)).snap_to_window_with_margin(px(8.)).child(body)).with_priority(1).into_any_element()
    }

    fn tree_menu_row(&self, item: Item, plan: &Plan, menu: &TreeMenu, live: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = format!("tree-menu-{item:?}");
        let row = match item {
            Item::Mention => ui::menu_row(id, "comment", label(item), None),
            Item::NewFile => ui::menu_row(id, "file", label(item), None),
            Item::NewFolder => ui::menu_row(id, "folder", label(item), None),
            Item::Copy => ui::menu_row(id, "copy", label(item), Some("⌥⌘C")),
            Item::CopyRelative => ui::menu_row(id, "copy", label(item), Some("⌥⇧⌘C")),
            Item::Rename => ui::menu_row(id, "pencil", label(item), Some("↵")),
            Item::Trash => ui::danger_row(id, "trash", label(item)).child(ui::kbd("⌘⌫")),
        };
        // The highlight follows `hi`, not the pointer, so arrows and the pointer share one.
        let row = row.hover(|s| s);
        if item == Item::Mention && *plan == Plan::Off {
            return row.opacity(0.4).cursor_default().into_any_element();
        }
        let bg = if item == Item::Trash { FAILED_BG } else { FILL_2 };
        let lit = menu.hi == Some(item) || (item == Item::Mention && menu.sub);
        let row = row.when(item == Item::Mention, |d| d.child(icon("chevron-right", 12., TEXT_3)));
        if !live {
            return row.when(lit, |d| d.bg(bg)).into_any_element();
        }
        if menu.chosen == Some(item) {
            return row.with_animation("tree-menu-blink", Animation::new(BLINK), move |d, t| d.when(t >= 0.5, |d| d.bg(bg))).into_any_element();
        }
        row.when(lit, |d| d.bg(bg))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover_tree_item(item, *hovered, cx)))
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseUpEvent, window, cx| {
                    if this.explorer.menu.as_ref().is_some_and(|m| held(m.opened.elapsed(), m.at, e.position)) {
                        this.choose_tree_item(item, window, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.choose_tree_item(item, window, cx)))
            .into_any_element()
    }

    fn chat_list(&self, plan: &Plan, live: bool, cx: &mut Context<Self>) -> Stateful<Div> {
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
                .when(c.ready && live, |d| {
                    d.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let Some((path, folder)) = this.explorer.close_menu().and_then(|m| m.target) else { return };
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
    use super::{Item, Plan, TreeMenu, held, items, mention_text, new_parent, plan, relative};
    use crate::add_to_chat::Choice;
    use gpui_kit::{point, px};
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn row_menu() -> TreeMenu {
        TreeMenu::new(Some(("/r/a.rs".into(), false)), point(px(0.), px(0.)), false, px(1200.))
    }

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
    fn a_new_item_goes_in_the_folder_clicked_beside_the_file_clicked_or_at_the_root() {
        let root = Path::new("/r");
        assert_eq!(new_parent(Some(&("/r/src".into(), true)), root), Path::new("/r/src"));
        assert_eq!(new_parent(Some(&("/r/src/a.rs".into(), false)), root), Path::new("/r/src"));
        assert_eq!(new_parent(None, root), root);
    }

    #[test]
    fn a_relative_path_is_from_the_root_and_the_root_is_a_dot() {
        assert_eq!(relative(Path::new("/r/src/a.rs"), Path::new("/r")), "src/a.rs");
        assert_eq!(relative(Path::new("/r"), Path::new("/r")), ".");
    }

    #[test]
    fn arrows_walk_the_live_items_wrapping_and_skip_a_mention_that_is_off() {
        let mut m = row_menu();
        m.step(true, true);
        assert_eq!(m.hi, Some(Item::NewFile));
        m.step(false, true);
        assert_eq!(m.hi, Some(Item::Trash));
        m.step(true, true);
        assert_eq!(m.hi, Some(Item::NewFile));
        let mut m = row_menu();
        m.step(false, false);
        assert_eq!(m.hi, Some(Item::Trash));
        m.step(true, false);
        assert_eq!(m.hi, Some(Item::Mention));
    }

    #[test]
    fn letters_typed_close_together_highlight_the_first_item_they_begin() {
        let (mut m, t) = (row_menu(), Instant::now());
        m.type_key("m", t, false);
        assert_eq!(m.hi, Some(Item::Mention));
        m.type_key("O", t + Duration::from_millis(100), false);
        assert_eq!(m.hi, Some(Item::Trash));
        m.type_key("r", t + Duration::from_secs(2), false);
        assert_eq!(m.hi, Some(Item::Rename));
        m.type_key("x", t + Duration::from_secs(4), false);
        assert_eq!(m.hi, Some(Item::Rename));
    }

    #[test]
    fn a_right_button_let_go_chooses_only_after_a_hold_or_a_drag() {
        let at = point(px(10.), px(10.));
        assert!(!held(Duration::from_millis(100), at, point(px(12.), px(12.))));
        assert!(held(Duration::from_millis(300), at, at));
        assert!(held(Duration::from_millis(100), at, point(px(15.), px(10.))));
    }

    #[test]
    fn the_root_menu_runs_only_its_items_but_either_copy_path_by_key() {
        let root = TreeMenu::new(None, point(px(0.), px(0.)), false, px(1200.));
        assert!(!root.offers(Item::Trash, false) && !root.offers(Item::Mention, false));
        assert!(root.offers(Item::CopyRelative, false));
        assert!(!row_menu().offers(Item::Mention, true));
    }
}
