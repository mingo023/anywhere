use super::edit::Naming;
use super::toast::Notice;
use super::ExplorerState;
use crate::actions::{TreeMenuArrow, TreeMenuChoose, TreeMenuItem};
use crate::add_to_chat::Choice;
use crate::chat::Route;
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
/// A right press held this long, or dragged this far, lets its release choose an item.
const HOLD: Duration = Duration::from_millis(250);
const DRAG: f32 = 4.;

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
        Item::Mention => "Mention in Chat",
        Item::NewFile => "New File…",
        Item::NewFolder => "New Folder…",
        Item::Copy => "Copy Path",
        Item::CopyRelative => "Copy Relative Path",
        Item::Rename => "Rename…",
        Item::Trash => "Move to Trash",
    }
}

/// Whether letting go of the right button on an item chooses it: the press that opened the menu was held, or dragged onto the item.
pub fn release_chooses(elapsed: Duration, from: Point<Pixels>, to: Point<Pixels>) -> bool {
    elapsed > HOLD || f32::from(to.x - from.x).hypot(f32::from(to.y - from.y)) > DRAG
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

/// What Mention can reach as the menu stands: where it sends, and which chats in the list can take text.
pub struct Reach {
    pub plan: Plan,
    pub ready: Vec<bool>,
}

impl Reach {
    fn off(&self) -> bool {
        self.plan == Plan::Off
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

/// What a click, Enter or a let-go chose in the menu: an item, or a chat in the list by its place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pick {
    Item(Item),
    Chat(usize),
}

#[derive(Clone)]
pub(crate) struct TreeMenu {
    /// The row and whether it's a folder; `None` is the root, from the space under the rows.
    pub(crate) target: Option<(PathBuf, bool)>,
    at: Point<Pixels>,
    /// The chat list opens left of the menu, there being no room right.
    flip: bool,
    alt: bool,
    chats_open: bool,
    /// The item arrows, typing or the pointer last landed on.
    highlighted: Option<Item>,
    /// The chat arrows or the pointer last landed on; while there is one, the list takes the arrows.
    chat_highlighted: Option<usize>,
    typed: String,
    typed_at: Instant,
    opened: Instant,
    /// The right press that opened the menu is still down, so its let-go may choose.
    pressing: bool,
    /// What blinks before it runs; the menu takes no more input meanwhile.
    chosen: Option<Pick>,
}

impl TreeMenu {
    /// A menu at `at` in a window `width` wide, closed chat list.
    pub(crate) fn new(target: Option<(PathBuf, bool)>, at: Point<Pixels>, alt: bool, width: Pixels) -> Self {
        let flip = f32::from(at.x) + MENU_W + GAP + SUB_W + 8. > f32::from(width);
        let now = Instant::now();
        Self { target, at, flip, alt, chats_open: false, highlighted: None, chat_highlighted: None, typed: String::new(), typed_at: now, opened: now, pressing: true, chosen: None }
    }

    /// The items that can be chosen, top to bottom.
    fn live(&self, reach: &Reach) -> Vec<Item> {
        items(self.target.is_some(), self.alt).into_iter().flatten().filter(|i| !(reach.off() && *i == Item::Mention)).collect()
    }

    /// Whether `pick` can run from this menu; either Copy Path runs by its key, whichever ⌥ shows.
    fn offers(&self, pick: Pick, reach: &Reach) -> bool {
        match pick {
            Pick::Item(Item::Copy | Item::CopyRelative) => true,
            Pick::Item(item) => self.live(reach).contains(&item),
            Pick::Chat(i) => self.target.is_some() && reach.ready.get(i) == Some(&true),
        }
    }

    pub(crate) fn busy(&self) -> bool {
        self.chosen.is_some()
    }

    /// Up and down walk the items, wrapping, or the ready chats while the list has the arrows; right opens the list from Mention, left closes it.
    pub fn arrow(&mut self, arrow: Arrow, reach: &Reach) {
        match (arrow, self.chat_highlighted) {
            (Arrow::Up | Arrow::Down, Some(at)) => self.chat_highlighted = next_ready(&reach.ready, at, arrow == Arrow::Down).or(Some(at)),
            (Arrow::Up | Arrow::Down, None) => {
                let live = self.live(reach);
                let n = live.len();
                let next = match live.iter().position(|i| Some(*i) == self.highlighted) {
                    Some(at) if arrow == Arrow::Down => (at + 1) % n,
                    Some(at) => (at + n - 1) % n,
                    None if arrow == Arrow::Down => 0,
                    None => n - 1,
                };
                (self.highlighted, self.chats_open) = (Some(live[next]), false);
            }
            (Arrow::Right, _) if self.highlighted == Some(Item::Mention) && !reach.off() => self.open_chats(reach),
            (Arrow::Left, _) => (self.chats_open, self.chat_highlighted) = (false, None),
            (Arrow::Right, _) => {}
        }
    }

    /// Shows the chat list from Mention, the first ready chat taking the arrows.
    fn open_chats(&mut self, reach: &Reach) {
        (self.highlighted, self.chats_open, self.chat_highlighted) = (Some(Item::Mention), true, reach.ready.iter().position(|r| *r));
    }

    /// Highlights the first live item whose label starts with the letters typed so far; false for a key that isn't a letter, or while the chat list has the keys.
    pub fn type_key(&mut self, key: &str, now: Instant, reach: &Reach) -> bool {
        if self.chat_highlighted.is_some() || key.len() != 1 || !key.chars().all(|c| c.is_ascii_alphabetic()) {
            return false;
        }
        if now.duration_since(self.typed_at) > TYPEAHEAD {
            self.typed.clear();
        }
        self.typed.push_str(&key.to_lowercase());
        self.typed_at = now;
        if let Some(item) = self.live(reach).into_iter().find(|i| label(*i).to_lowercase().starts_with(&self.typed)) {
            (self.highlighted, self.chats_open) = (Some(item), false);
        }
        true
    }

    pub fn hover(&mut self, item: Item, hovered: bool) {
        if hovered {
            (self.highlighted, self.chats_open, self.chat_highlighted) = (Some(item), item == Item::Mention, None);
        } else if self.highlighted == Some(item) {
            self.highlighted = None;
        }
    }

    pub fn hover_chat(&mut self, i: usize, hovered: bool) {
        if hovered {
            self.chat_highlighted = Some(i);
        } else if self.chat_highlighted == Some(i) {
            self.chat_highlighted = None;
        }
    }

    /// Follows ⌥, keeping a highlighted or blinking Copy Path on the row it becomes; false when it didn't change.
    pub fn set_alt(&mut self, alt: bool) -> bool {
        if self.alt == alt {
            return false;
        }
        let swap = |i: Item| match i {
            Item::Copy => Item::CopyRelative,
            Item::CopyRelative => Item::Copy,
            i => i,
        };
        self.alt = alt;
        self.highlighted = self.highlighted.map(swap);
        if let Some(Pick::Item(i)) = self.chosen {
            self.chosen = Some(Pick::Item(swap(i)));
        }
        true
    }

    /// What Enter chooses: the highlighted chat while the list has the arrows, else the highlighted item.
    pub fn enter(&self) -> Option<Pick> {
        self.chat_highlighted.map(Pick::Chat).or(self.highlighted.map(Pick::Item))
    }

    /// Whether the right button let go at `to` chooses: only the press that opened the menu does, once held or dragged.
    pub fn let_go(&mut self, to: Point<Pixels>) -> bool {
        std::mem::take(&mut self.pressing) && release_chooses(self.opened.elapsed(), self.at, to)
    }
}

/// The next ready chat after `at`, down or up, wrapping.
fn next_ready(ready: &[bool], at: usize, down: bool) -> Option<usize> {
    let n = ready.len();
    (1..=n).map(|k| if down { (at + k) % n } else { (at + n - k) % n }).find(|i| ready[*i])
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

    fn reach(&self) -> Reach {
        let choices = self.chat_choices();
        let plan = if self.agents.observe_only() { Plan::Off } else { plan(self.session(), &choices) };
        Reach { plan, ready: choices.iter().map(|c| c.ready).collect() }
    }

    pub(crate) fn tree_menu_item(&mut self, item: &TreeMenuItem, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_in_tree_menu(Pick::Item(item.0), window, cx);
    }

    pub(crate) fn tree_menu_choose(&mut self, _: &TreeMenuChoose, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pick) = self.explorer.menu.as_ref().and_then(TreeMenu::enter) {
            self.choose_in_tree_menu(pick, window, cx);
        }
    }

    pub(crate) fn tree_menu_arrow(&mut self, arrow: &TreeMenuArrow, _: &mut Window, cx: &mut Context<Self>) {
        let reach = self.reach();
        if let Some(menu) = self.explorer.menu.as_mut().filter(|m| !m.busy()) {
            menu.arrow(arrow.0, &reach);
            cx.notify();
        }
    }

    /// Takes an unmodified letter as typeahead while the menu is open, and any key while it blinks; returns whether it did.
    pub(crate) fn type_tree_menu(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let m = key.modifiers;
        if self.explorer.menu.is_none() || m.platform || m.control || m.alt || m.function {
            return false;
        }
        let reach = self.reach();
        let Some(menu) = self.explorer.menu.as_mut() else { return false };
        if menu.busy() {
            return true;
        }
        let took = menu.type_key(&key.key, Instant::now(), &reach);
        cx.notify();
        took
    }

    /// Blinks `pick`, then runs it; a Mention that asks which chat opens the list at once.
    fn choose_in_tree_menu(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let reach = self.reach();
        let Some(menu) = self.explorer.menu.as_mut().filter(|m| !m.busy() && m.offers(pick, &reach)) else { return };
        if cx.reduce_motion() || (pick == Pick::Item(Item::Mention) && reach.plan == Plan::Ask) {
            return self.run_pick(pick, window, cx);
        }
        menu.chosen = Some(pick);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(BLINK).await;
            this.update_in(cx, |d, window, cx| {
                // A menu opened elsewhere during the blink isn't the one this was chosen in.
                if d.explorer.menu.as_ref().is_some_and(|m| m.chosen == Some(pick)) {
                    d.run_pick(pick, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn run_pick(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(menu), Some(root)) = (self.explorer.menu.as_ref(), self.explore_root().map(PathBuf::from)) else { return };
        let target = menu.target.clone();
        let item = match pick {
            Pick::Item(item) => item,
            Pick::Chat(i) => {
                let (Some(chat), Some((path, folder))) = (self.chat_choices().into_iter().nth(i), target) else { return };
                self.mention(&chat.id, &path, folder, window, cx);
                self.explorer.close_menu();
                return cx.notify();
            }
        };
        match (item, target) {
            (Item::Mention, Some((path, folder))) => {
                let reach = self.reach();
                match &reach.plan {
                    Plan::To(id) => self.mention(id, &path, folder, window, cx),
                    Plan::Ask => {
                        if let Some(menu) = self.explorer.menu.as_mut() {
                            menu.open_chats(&reach);
                        }
                        return cx.notify();
                    }
                    Plan::Off => return,
                }
            }
            (Item::NewFile | Item::NewFolder, target) => self.start_edit(Naming::New { parent: new_parent(target.as_ref(), &root), folder: item == Item::NewFolder }, window, cx),
            (Item::Copy | Item::CopyRelative, target) => {
                let path = target.map_or(root.clone(), |(p, _)| p);
                let text = if item == Item::Copy { path.to_string_lossy().to_string() } else { relative(&path, &root) };
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                self.show_notice(Notice::Copied(text), cx);
            }
            (Item::Rename, Some((path, folder))) => self.start_edit(Naming::Rename { path, folder }, window, cx),
            (Item::Trash, Some((path, _))) => self.trash_file(path, cx),
            (Item::Mention | Item::Rename | Item::Trash, None) => return,
        }
        self.explorer.close_menu();
        cx.notify();
    }

    fn mention(&mut self, id: &str, path: &Path, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((terminal, provider, cwd)) = self.agents.get(id).map(|a| (a.terminal_id.clone(), a.provider.clone(), a.cwd.clone())) else { return };
        let text = mention_text(path, folder, Path::new(&cwd));
        if self.route(&terminal) == Route::Chat {
            self.chat_add_text(id, &text, window, cx);
        } else {
            let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
            self.send_input(&terminal, &term::paste_bytes(&text, bracketed), cx);
        }
        // The focused chat shows the mention landing in its input; any other gets a toast.
        if self.session() != Some(id) {
            let title = self.chat_choices().into_iter().find(|c| c.id == id).map(|c| c.title).unwrap_or_default();
            self.show_notice(Notice::Mentioned { provider, title }, cx);
        }
    }

    fn hover_tree_menu(&mut self, hover: impl FnOnce(&mut TreeMenu), cx: &mut Context<Self>) {
        if let Some(menu) = self.explorer.menu.as_mut().filter(|m| !m.busy()) {
            hover(menu);
            cx.notify();
        }
    }

    /// Chooses `pick` when the right button let go on it is the press that opened the menu.
    fn let_go_tree_menu(&mut self, pick: Pick, to: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.explorer.menu.as_mut().is_some_and(|m| m.let_go(to)) {
            self.choose_in_tree_menu(pick, window, cx);
        }
    }

    /// The open menu, or the one just closed while it fades.
    pub(crate) fn tree_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(menu) = &self.explorer.menu {
            return Some(self.menu_body(menu, true, cx));
        }
        let (menu, _) = self.explorer.leaving.as_ref().filter(|(_, at)| at.elapsed() < LEAVE && !cx.reduce_motion())?;
        Some(self.menu_body(menu, false, cx))
    }

    /// The menu and its chat list; an `open` one takes input, else it's the closed menu fading out.
    fn menu_body(&self, menu: &TreeMenu, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let reach = if menu.target.is_some() { self.reach() } else { Reach { plan: Plan::Off, ready: Vec::new() } };
        let rows = items(menu.target.is_some(), menu.alt).into_iter().map(|item| match item {
            None => ui::menu_divider().into_any_element(),
            Some(item) => self.tree_menu_row(item, &reach, menu, open, cx),
        });
        let main = ui::pop(div().id("tree-menu")).w(px(MENU_W)).p(px(6.)).flex().flex_col().children(rows);
        let chats = (menu.chats_open && !reach.off()).then(|| self.chat_list(&reach, menu, open, cx));
        let x = if menu.flip && chats.is_some() { menu.at.x - px(SUB_W + GAP) } else { menu.at.x };
        let body = div().id(if open { "tree-menu-box" } else { "tree-menu-leaving" }).flex().items_start().gap(px(GAP)).when(menu.flip, |d| d.flex_row_reverse()).child(main).children(chats);
        let body = if open {
            let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.explorer.close_menu();
                cx.notify();
            });
            let let_go = |this: &mut Self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>| {
                if let Some(menu) = this.explorer.menu.as_mut() {
                    menu.pressing = false;
                }
            };
            let body = body.occlude().on_mouse_down_out(close).on_mouse_up(MouseButton::Right, cx.listener(let_go)).on_mouse_up_out(MouseButton::Right, cx.listener(let_go));
            ui::menu_in("tree-menu-in", body).into_any_element()
        } else {
            body.with_animation("tree-menu-out", Animation::new(LEAVE), |d, t| d.opacity(1. - t)).into_any_element()
        };
        deferred(anchored().position(point(x, menu.at.y)).snap_to_window_with_margin(px(8.)).child(body)).with_priority(1).into_any_element()
    }

    fn tree_menu_row(&self, item: Item, reach: &Reach, menu: &TreeMenu, open: bool, cx: &mut Context<Self>) -> AnyElement {
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
        // The highlight follows `highlighted`, not the pointer, so arrows and the pointer share one.
        let row = row.hover(|s| s).when(item == Item::Mention && !reach.ready.is_empty(), |d| d.child(icon("chevron-right", 12., TEXT_3)));
        if item == Item::Mention && reach.off() {
            return row.opacity(0.4).cursor_default().into_any_element();
        }
        let bg = if item == Item::Trash { FAILED_BG } else { FILL_2 };
        let lit = menu.highlighted == Some(item) || (item == Item::Mention && menu.chats_open);
        if !open {
            return row.when(lit, |d| d.bg(bg)).into_any_element();
        }
        if menu.chosen == Some(Pick::Item(item)) {
            return blink(row, if item == Item::Trash { FAILED_BG } else { ACCENT_BG }).into_any_element();
        }
        row.when(lit, |d| d.bg(bg))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover_tree_menu(|m| m.hover(item, *hovered), cx)))
            .on_mouse_up(MouseButton::Right, cx.listener(move |this, e: &MouseUpEvent, window, cx| this.let_go_tree_menu(Pick::Item(item), e.position, window, cx)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.choose_in_tree_menu(Pick::Item(item), window, cx)))
            .into_any_element()
    }

    fn chat_list(&self, reach: &Reach, menu: &TreeMenu, open: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows = self.chat_choices().into_iter().enumerate().map(|(i, c)| {
            let trailing = if !c.ready {
                Some(div().text_size(px(12.)).text_color(TEXT_3).child("Needs you"))
            } else {
                (reach.plan == Plan::To(c.id.clone())).then(|| ui::kbd("⌘L"))
            };
            let row = div()
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
                .when(!c.ready, |d| d.opacity(0.5));
            if !c.ready || !open {
                return row.into_any_element();
            }
            if menu.chosen == Some(Pick::Chat(i)) {
                return blink(row, ACCENT_BG).into_any_element();
            }
            row.cursor_pointer()
                .when(menu.chat_highlighted == Some(i), |d| d.bg(FILL_2))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover_tree_menu(|m| m.hover_chat(i, *hovered), cx)))
                .on_mouse_up(MouseButton::Right, cx.listener(move |this, e: &MouseUpEvent, window, cx| this.let_go_tree_menu(Pick::Chat(i), e.position, window, cx)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.choose_in_tree_menu(Pick::Chat(i), window, cx)))
                .into_any_element()
        });
        ui::pop(div().id("tree-chats")).w(px(SUB_W)).p(px(6.)).flex().flex_col().children(rows)
    }
}

/// On, off, on again: a macOS menu's blink on what was chosen.
fn blink(row: Stateful<Div>, bg: impl Into<Hsla>) -> impl IntoElement {
    let bg = bg.into();
    row.with_animation("tree-menu-blink", Animation::new(BLINK), move |d, t| d.when(!(0.35..0.7).contains(&t), |d| d.bg(bg)))
}

#[cfg(test)]
mod tests {
    use super::{Arrow, Item, Pick, Plan, Reach, TreeMenu, items, mention_text, new_parent, plan, relative, release_chooses};
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

    fn reach(plan: Plan, ready: &[bool]) -> Reach {
        Reach { plan, ready: ready.to_vec() }
    }

    #[test]
    fn arrows_walk_the_live_items_wrapping_and_skip_a_mention_that_is_off() {
        let on = reach(Plan::To("a".into()), &[true]);
        let mut m = row_menu();
        m.arrow(Arrow::Down, &on);
        assert_eq!(m.highlighted, Some(Item::Mention));
        m.arrow(Arrow::Up, &on);
        assert_eq!(m.highlighted, Some(Item::Trash));
        m.arrow(Arrow::Down, &on);
        assert_eq!(m.highlighted, Some(Item::Mention));
        let off = reach(Plan::Off, &[]);
        let mut m = row_menu();
        m.arrow(Arrow::Up, &off);
        assert_eq!(m.highlighted, Some(Item::Trash));
        m.arrow(Arrow::Down, &off);
        assert_eq!(m.highlighted, Some(Item::NewFile));
    }

    #[test]
    fn right_from_mention_hands_the_arrows_to_the_ready_chats_and_left_takes_them_back() {
        let ask = reach(Plan::Ask, &[false, true, false, true]);
        let mut m = row_menu();
        m.arrow(Arrow::Down, &ask);
        m.arrow(Arrow::Right, &ask);
        assert_eq!(m.enter(), Some(Pick::Chat(1)));
        m.arrow(Arrow::Down, &ask);
        assert_eq!(m.enter(), Some(Pick::Chat(3)));
        m.arrow(Arrow::Down, &ask);
        assert_eq!(m.enter(), Some(Pick::Chat(1)));
        m.arrow(Arrow::Up, &ask);
        assert_eq!(m.enter(), Some(Pick::Chat(3)));
        m.arrow(Arrow::Left, &ask);
        assert_eq!(m.enter(), Some(Pick::Item(Item::Mention)));
        assert!(!m.chats_open);
    }

    #[test]
    fn walking_or_typing_to_another_item_closes_the_chat_list() {
        let ask = reach(Plan::Ask, &[true, true]);
        let mut m = row_menu();
        m.hover(Item::Mention, true);
        assert!(m.chats_open);
        m.arrow(Arrow::Down, &ask);
        assert!(!m.chats_open);
        m.hover(Item::Mention, true);
        m.type_key("r", Instant::now(), &ask);
        assert_eq!((m.highlighted, m.chats_open), (Some(Item::Rename), false));
    }

    #[test]
    fn letters_typed_close_together_highlight_the_first_item_they_begin() {
        let (mut m, t, on) = (row_menu(), Instant::now(), reach(Plan::To("a".into()), &[true]));
        m.type_key("m", t, &on);
        assert_eq!(m.highlighted, Some(Item::Mention));
        m.type_key("O", t + Duration::from_millis(100), &on);
        assert_eq!(m.highlighted, Some(Item::Trash));
        m.type_key("r", t + Duration::from_secs(2), &on);
        assert_eq!(m.highlighted, Some(Item::Rename));
        m.type_key("x", t + Duration::from_secs(4), &on);
        assert_eq!(m.highlighted, Some(Item::Rename));
    }

    #[test]
    fn typeahead_takes_only_letters() {
        let mut m = row_menu();
        let on = reach(Plan::To("a".into()), &[true]);
        assert!(!m.type_key("1", Instant::now(), &on));
        assert!(!m.type_key("space", Instant::now(), &on));
        assert_eq!(m.highlighted, None);
    }

    #[test]
    fn alt_keeps_a_highlighted_copy_path_on_the_row_it_becomes() {
        let mut m = row_menu();
        m.hover(Item::Copy, true);
        assert!(m.set_alt(true));
        assert_eq!(m.highlighted, Some(Item::CopyRelative));
        assert!(!m.set_alt(true));
    }

    #[test]
    fn enter_with_nothing_highlighted_chooses_nothing() {
        assert_eq!(row_menu().enter(), None);
    }

    #[test]
    fn a_right_button_let_go_chooses_only_after_a_hold_or_a_drag() {
        let at = point(px(10.), px(10.));
        assert!(!release_chooses(Duration::from_millis(100), at, point(px(12.), px(12.))));
        assert!(release_chooses(Duration::from_millis(300), at, at));
        assert!(release_chooses(Duration::from_millis(100), at, point(px(15.), px(10.))));
    }

    #[test]
    fn only_the_let_go_of_the_press_that_opened_the_menu_chooses() {
        let mut m = row_menu();
        assert!(m.let_go(point(px(20.), px(0.))));
        assert!(!m.let_go(point(px(20.), px(0.))));
    }

    #[test]
    fn the_root_menu_runs_only_its_items_but_either_copy_path_by_key() {
        let (on, off) = (reach(Plan::To("a".into()), &[true]), reach(Plan::Off, &[]));
        let root = TreeMenu::new(None, point(px(0.), px(0.)), false, px(1200.));
        assert!(!root.offers(Pick::Item(Item::Trash), &on) && !root.offers(Pick::Item(Item::Mention), &on) && !root.offers(Pick::Chat(0), &on));
        assert!(root.offers(Pick::Item(Item::CopyRelative), &on));
        assert!(!row_menu().offers(Pick::Item(Item::Mention), &off));
        assert!(row_menu().offers(Pick::Chat(0), &on) && !row_menu().offers(Pick::Chat(1), &on));
    }
}
