pub(crate) mod add_menu;
pub(crate) mod effort_menu;
pub(crate) mod file_card;
pub(crate) mod launch;
pub(crate) mod model_menu;
pub(crate) mod pills;
pub(crate) mod rich;
pub(crate) mod team_chip;

use crate::chat::composer::suggest::Suggest;
use crate::chat::composer::{INPUT_LINE_HEIGHT, INPUT_TEXT_SIZE, dragged_over, frame, hand_focus, marks_overlay};
use crate::desktop::Desktop;
use agents::providers::{self, Transport};
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use launch::Pick;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use store::LaunchPick;
use theme::*;
use add_menu::Opens;

/// Where a composer lives in `Desktop`, so its listeners can reach it.
pub(crate) type Slot = fn(&mut Desktop) -> &mut Composer;

/// The prompt box that starts an agent: the words, the agent, model and access to start it with, and attached files
/// and images.
pub(crate) struct Composer {
    pub(crate) prompt: Entity<TextareaState>,
    /// Narrows the model menu's list.
    search: Entity<InputState>,
    pub(crate) draft: Draft,
    pub(crate) suggest: Suggest,
    slot: Slot,
    /// Closes the host's own menus as one of the composer's opens.
    opened: fn(&mut Desktop),
    /// Shows why a pasted image or long text couldn't be saved.
    unsaved: fn(&mut Desktop, (String, String)),
}

impl Composer {
    pub(crate) fn new(prompt: Entity<TextareaState>, search: Entity<InputState>, slot: Slot, opened: fn(&mut Desktop), unsaved: fn(&mut Desktop, (String, String))) -> Self {
        Self { prompt, search, draft: Draft::default(), suggest: Suggest::default(), slot, opened, unsaved }
    }
}

/// The chip whose menu is open.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    /// The model pill's panel: its agents, models and efforts.
    Model,
    Access,
    /// The chat's account popover on its usage, slid `back` to from its accounts.
    Usage { back: bool },
    /// The chat's account popover on the accounts to switch to.
    Switch,
    /// The "+" button's: Attach files, and Team.
    Add,
    /// The chat's goal, opened from its pill.
    Goal,
}

/// A composer's choices besides its words.
#[derive(Default)]
pub(crate) struct Draft {
    /// The agent picked here, and the Project it was picked for; another Project starts with its own.
    chosen: Option<(Option<String>, LaunchPick)>,
    pub(crate) menu: Option<Menu>,
    /// The rail entry the model menu lists, once one is clicked: an agent, or `model_menu::FAVORITES`.
    pub(crate) viewing: Option<&'static str>,
    /// Where the model menu's list is scrolled to, and its rail of agents.
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) rail: ScrollHandle,
    /// What the model menu lists.
    pub(crate) listed: model_menu::KeptList,
    pub(crate) effort: effort_menu::EffortMenu,
    pub(crate) images: Vec<Attachment>,
    /// The dropped files and long pastes, which the agent reads where they lie.
    pub(crate) files: Vec<PathBuf>,
    /// The Team chip: the next send asks the agent for a team plan.
    pub(crate) team: bool,
}

/// A pasted or dropped image and the file the agent reads it from.
pub(crate) struct Attachment {
    pub(crate) path: PathBuf,
    pub(crate) image: ImageSource,
}

impl Draft {
    /// The agent picked here for `project`, else `last`.
    pub(crate) fn pick(&self, project: Option<&str>, last: impl FnOnce() -> LaunchPick) -> LaunchPick {
        self.chosen.as_ref().filter(|(p, _)| p.as_deref() == project).map_or_else(last, |(_, pick)| pick.clone())
    }

    pub(crate) fn choose(&mut self, project: Option<String>, pick: LaunchPick) {
        self.adjust(project, pick);
        self.menu = None;
    }

    /// `choose`, with the menu left open.
    fn adjust(&mut self, project: Option<String>, pick: LaunchPick) {
        self.chosen = Some((project, pick));
    }

    /// Lists `entry`'s models in the model panel; the efforts are the last entry's.
    pub(crate) fn view(&mut self, entry: &'static str) {
        self.viewing = Some(entry);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.effort.fold();
    }

    /// Whether `menu` is open now.
    pub(crate) fn toggle(&mut self, menu: Menu) -> bool {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        self.viewing = None;
        (self.scroll, self.rail) = Default::default();
        self.effort = Default::default();
        self.menu.is_some()
    }

    /// The "+" menu's Team row: sets the Team chip, or takes it off, and closes the menu.
    pub(crate) fn toggle_team(&mut self) {
        (self.team, self.menu) = (!self.team, None);
    }

    /// Esc on the chat's account popover: its accounts go back to its usage, its usage closes. False while it isn't open.
    pub(crate) fn escape_account(&mut self) -> bool {
        match self.menu {
            Some(Menu::Switch) => self.menu = Some(Menu::Usage { back: true }),
            Some(Menu::Usage { .. }) => self.menu = None,
            _ => return false,
        }
        true
    }

    /// Esc on the model panel closes its list of efforts, else the panel. False while it isn't open.
    pub(crate) fn escape_model(&mut self) -> bool {
        let open = self.menu == Some(Menu::Model);
        if open && !self.effort.fold() {
            self.menu = None;
        }
        open
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether the image is new to this draft and its file still needs writing.
    fn attach(&mut self, attachment: Attachment) -> bool {
        let new = !self.images.iter().any(|a| a.path == attachment.path);
        if new {
            self.images.push(attachment);
        }
        new
    }

    pub(crate) fn has_attachments(&self) -> bool {
        !self.images.is_empty() || !self.files.is_empty()
    }

    /// Attaches the dropped files where they lie: an image shows as itself, any other file as a card.
    pub(crate) fn drop_files(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if is_image(path) {
                self.attach(Attachment { image: path.clone().into(), path: path.clone() });
            } else {
                self.attach_file(path.clone());
            }
        }
    }

    /// Whether the file is new to this draft.
    fn attach_file(&mut self, path: PathBuf) -> bool {
        let new = !self.files.contains(&path);
        if new {
            self.files.push(path);
        }
        new
    }

    /// Drops the attachment whose file couldn't be written; false if this draft no longer held it.
    fn unsaved(&mut self, path: &Path) -> bool {
        let held = self.images.len() + self.files.len();
        self.images.retain(|a| a.path != path);
        self.files.retain(|f| f != path);
        self.images.len() + self.files.len() < held
    }

    /// The prompt goes to the agent as text, so files and images go in as paths it can read.
    pub(crate) fn prompt(&self, typed: &str) -> String {
        let block = |title: &str, paths: Vec<&Path>| (!paths.is_empty()).then(|| format!("{title}{}", paths.iter().map(|p| format!("- {}", p.display())).collect::<Vec<_>>().join("\n")));
        let typed = typed.trim();
        let parts = [(!typed.is_empty()).then(|| typed.to_string()), block(FILES, self.files.iter().map(PathBuf::as_path).collect()), block(IMAGES, self.images.iter().map(|a| a.path.as_path()).collect())];
        parts.into_iter().flatten().collect::<Vec<_>>().join("\n\n")
    }
}

const FILES: &str = "# Attached files\n\n";
const IMAGES: &str = "# Attached images\n\n";

/// A sent prompt's typed text, and the paths attached to it.
#[derive(Debug, PartialEq)]
pub(crate) struct Sent<'a> {
    pub(crate) text: &'a str,
    pub(crate) files: Vec<&'a str>,
    pub(crate) images: Vec<&'a str>,
}

/// Undoes [`Draft::prompt`].
pub(crate) fn split_attachments(prompt: &str) -> Sent<'_> {
    let (rest, images) = split_block(prompt, IMAGES);
    let (text, files) = split_block(rest, FILES);
    Sent { text, files, images }
}

/// `prompt` without the list of paths under `title` that ends it, and those paths.
fn split_block<'a>(prompt: &'a str, title: &str) -> (&'a str, Vec<&'a str>) {
    let at = match prompt.rfind(&format!("\n\n{title}")) {
        Some(i) => Some((i, i + 2)),
        None => prompt.starts_with(title).then_some((0, 0)),
    };
    let Some((end, block)) = at else { return (prompt, Vec::new()) };
    let paths: Option<Vec<&str>> = prompt[block + title.len()..].lines().map(|l| l.strip_prefix("- ")).collect();
    match paths {
        Some(paths) if !paths.is_empty() => (&prompt[..end], paths),
        _ => (prompt, Vec::new()),
    }
}

/// A team's lead plans in a chat pocketd drives over Claude's or Codex's transport; `served` is `Agents::teams_offered`.
pub(crate) fn team_offered(served: bool, chat: bool, provider: &str) -> bool {
    served && chat && providers::get(provider).is_some_and(|b| matches!(b.transport, Transport::Claude | Transport::Codex))
}

/// Has a create's first prompt ask for a team plan, when `team` is set and the session the spec starts can lead one.
pub(crate) fn ask_team(spec: &mut serde_json::Value, team: bool, served: bool) {
    if team && team_offered(served, spec["chat"] == true, spec["provider"].as_str().unwrap_or_default()) {
        spec["team"] = true.into();
    }
}

fn copied_files(item: &ClipboardItem) -> Option<&ExternalPaths> {
    item.entries().iter().find_map(|e| match e {
        ClipboardEntry::ExternalPaths(paths) => Some(paths),
        _ => None,
    })
}

/// The image a paste carries when it has no text; text pastes as itself.
fn pasted_image(item: &ClipboardItem) -> Option<Image> {
    if item.text().is_some() {
        return None;
    }
    item.entries().iter().find_map(|e| match e {
        ClipboardEntry::Image(image) => Some(image.clone()),
        _ => None,
    })
}

/// What both GPUI shows and the agents read.
fn is_image(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp")
}

fn attachment_path(image: &Image, dir: &Path) -> PathBuf {
    dir.join(format!("{}.{}", image.id, image.format.extension()))
}

/// Where a long paste is kept for the agent to read, named by what it holds.
fn pasted_path(text: &str, dir: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    dir.join(format!("{}{:016x}.txt", file_card::PASTED, hasher.finish()))
}

/// Owner-only, like the rest of Pocket's files: screenshots and pastes can hold secrets.
fn save_private(bytes: &[u8], path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(dir)?;
    }
    std::fs::write(path, bytes)
}

/// Writes an attachment's file behind the UI, so starting never waits on a write; the agent opens it long after.
/// `failed` hears of one that couldn't be written.
fn save_behind(
    desktop: WeakEntity<Desktop>,
    path: PathBuf,
    save: impl FnOnce(&Path) -> std::io::Result<()> + Send + 'static,
    failed: impl FnOnce(&mut Desktop, &Path, String) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let save = cx.background_executor().spawn({
        let path = path.clone();
        async move { save(&path) }
    });
    window
        .spawn(cx, async move |cx| {
            if let Err(e) = save.await {
                desktop
                    .update(cx, |d, cx| {
                        failed(d, &path, e.to_string());
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
}

/// For `save_behind`: an attachment that couldn't be written leaves the draft, and `unsaved` shows why under `title`.
fn left_out(title: &'static str, draft: impl Fn(&mut Desktop) -> &mut Draft + 'static, unsaved: impl FnOnce(&mut Desktop, (String, String)) + 'static) -> impl FnOnce(&mut Desktop, &Path, String) + 'static {
    move |d, path, error| {
        if draft(d).unsaved(path) {
            unsaved(d, (title.into(), error));
        }
    }
}

/// What a paste puts in a prompt.
#[derive(Debug, PartialEq)]
enum Pasted<'a> {
    /// Copied files, which macOS gives as their paths, a screenshot tool's included, never their pixels.
    Files(&'a ExternalPaths),
    /// An address pasted over selected words, as a link to it.
    Link(String),
    /// A text too long for the prompt, to attach as a file.
    Long(String),
    Image(Image),
    /// Text that pastes as itself.
    Text,
}

/// What pasting `item` over the selected `picked` puts in the prompt.
fn pasted<'a>(item: &'a ClipboardItem, picked: &str) -> Pasted<'a> {
    if let Some(paths) = copied_files(item) {
        return Pasted::Files(paths);
    }
    let Some(text) = item.text() else { return pasted_image(item).map_or(Pasted::Text, Pasted::Image) };
    match chat::rich::pasted_link(picked, &text) {
        Some(link) => Pasted::Link(link),
        None if chat::rich::long_paste(&text) => Pasted::Long(text),
        None => Pasted::Text,
    }
}

/// Takes a paste into the draft `draft` picks, as `pasted` reads it, and says whether it did; text that pastes as
/// itself is left to the input.
pub(crate) fn paste(
    desktop: &WeakEntity<Desktop>,
    input: &Entity<TextareaState>,
    draft: impl Fn(&mut Desktop) -> &mut Draft + 'static,
    unsaved: impl FnOnce(&mut Desktop, (String, String)) + 'static,
    item: &ClipboardItem,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let state = input.read(cx);
    let (pasted, d) = (pasted(item, &state.value()[state.selected_range()]), desktop.upgrade());
    let dir = d.as_ref().map(|d| d.read(cx).store.home().join("attachments"));
    match (pasted, d, dir) {
        (Pasted::Text, ..) => false,
        (Pasted::Files(paths), ..) => {
            input.update(cx, |s, cx| s.drop_paths(paths, window, cx));
            true
        }
        (Pasted::Link(link), ..) => {
            input.update(cx, |s, cx| s.replace_text_in_range(None, &link, window, cx));
            true
        }
        (_, None, _) | (_, _, None) => false,
        (Pasted::Long(text), Some(d), Some(dir)) => {
            let path = pasted_path(&text, &dir);
            if d.update(cx, |d, cx| {
                cx.notify();
                draft(d).attach_file(path.clone())
            }) {
                save_behind(desktop.clone(), path, move |path| save_private(text.as_bytes(), path), left_out("Couldn't save the pasted text", draft, unsaved), window, cx);
            }
            true
        }
        (Pasted::Image(image), Some(d), Some(dir)) => {
            let image = Arc::new(image);
            let path = attachment_path(&image, &dir);
            if d.update(cx, |d, cx| {
                cx.notify();
                draft(d).attach(Attachment { path: path.clone(), image: image.clone().into() })
            }) {
                save_behind(desktop.clone(), path, move |path| save_private(&image.bytes, path), left_out("Couldn't save the pasted image", draft, unsaved), window, cx);
            }
            true
        }
    }
}

/// For a prompt's `on_drop_paths`: dropped files attach to the draft `draft` picks, so none is typed in as its path.
pub(crate) fn drop_handler(desktop: WeakEntity<Desktop>, draft: impl Fn(&mut Desktop) -> &mut Draft + 'static) -> impl Fn(&[PathBuf], &mut Window, &mut App) -> Vec<PathBuf> {
    move |paths, _, cx| match desktop.upgrade() {
        Some(d) => d.update(cx, |d, cx| {
            cx.notify();
            draft(d).drop_files(paths);
            Vec::new()
        }),
        None => paths.to_vec(),
    }
}

impl Desktop {
    /// The box around `textarea`, as the chat's; `pills` lead the attachments above it, `trailing` comes before the +
    /// and `send`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn composer(&self, c: &Composer, project: Option<String>, textarea: Textarea, typing: bool, pills: Vec<AnyElement>, trailing: Vec<AnyElement>, send: Stateful<Div>, cx: &mut Context<Self>) -> Div {
        let pick = c.draft.pick(project.as_deref(), || self.last_pick(project.as_deref()));
        let team = team_offered(self.agents.teams_offered(), self.store.agents.starts_as_chat(&pick.provider), &pick.provider).then_some(c.draft.team);
        let (slot, desktop) = (c.slot, cx.entity().downgrade());
        let remove = |id: ElementId, take: fn(&mut Draft, usize), i: usize| {
            div()
                .id(id)
                .absolute()
                .top(px(-6.))
                .right(px(-6.))
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .bg(POPOVER)
                .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                .cursor_pointer()
                .active(|s| s.opacity(0.6))
                .child(icon("x", 10., TEXT_2))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    take(&mut slot(this).draft, i);
                    cx.notify();
                }))
        };
        let attached = (!pills.is_empty() || c.draft.has_attachments()).then(|| {
            let images = c.draft.images.iter().enumerate().map(|(i, a)| {
                let image = img(a.image.clone()).size_full().rounded(px(10.)).object_fit(ObjectFit::Cover);
                div().relative().size(px(56.)).flex_none().rounded(px(10.)).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).child(image).child(remove(("image-remove", i).into(), |d, i| _ = d.images.remove(i), i))
            });
            let files = c.draft.files.iter().enumerate().map(|(i, path)| div().relative().flex_none().child(file_card::file_card(("composer-file", i), path)).child(remove(("file-remove", i).into(), |d, i| _ = d.files.remove(i), i)));
            div().flex().flex_wrap().items_center().gap(px(8.)).pt(px(14.)).px(px(16.)).children(pills).children(images).children(files)
        });
        let textarea = pills::with_pills(textarea, &c.prompt)
            .appearance(false)
            .text_size(px(INPUT_TEXT_SIZE))
            .line_height(px(INPUT_LINE_HEIGHT))
            .on_paste({
                let prompt = c.prompt.clone();
                move |item, window, cx| paste(&desktop, &prompt, move |d| &mut slot(d).draft, move |d, e| (slot(d).unsaved)(d, e), item, window, cx)
            });
        let bar = prompt_bar()
            .child(self.model_pill(c, &pick, project.clone(), cx))
            .child(self.access_chip(c, &pick, project, cx))
            .children((team == Some(true)).then(|| team_chip::team_chip("composer-team-remove", move |this, _, _| slot(this).draft.team = false, cx)))
            .child(div().flex_1())
            .children(trailing)
            .child(add_menu::add_button("composer-add", c.draft.menu == Some(Menu::Add), team, Opens::Down, move |d| &mut slot(d).draft, move |this, _, cx| this.toggle_composer(slot, Menu::Add, cx), cx))
            .child(send);
        // The search takes the keys while the model panel is open; closing it would leave them nowhere.
        let handoff = (c.draft.menu != Some(Menu::Model)).then(|| hand_focus(vec![c.search.read(cx).focus_handle(cx)], c.prompt.read(cx).focus_handle(cx)));
        let prompt = c.prompt.clone();
        frame(typing)
            .children(handoff)
            // Files dropped anywhere on the box go to the prompt. Not a `cx.listener`: the prompt's drop handler updates `Desktop`.
            .drag_over::<ExternalPaths>(|s, _, _, _| dragged_over(s))
            .on_drop(move |paths: &ExternalPaths, window, cx| prompt.update(cx, |s, cx| s.drop_paths(paths, window, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| slot(this).prompt.update(cx, |s, cx| s.focus(window, cx))))
            .children(attached)
            .child(prompt_text().child(textarea).child(marks_overlay(c.prompt.clone())))
            .child(bar)
    }

    pub(crate) fn toggle_composer(&mut self, slot: Slot, menu: Menu, cx: &mut Context<Self>) {
        let c = slot(self);
        let opened = c.opened;
        if c.draft.toggle(menu) {
            opened(self);
        }
        cx.notify();
    }

    fn choose_launch(&mut self, slot: Slot, project: Option<String>, pick: LaunchPick, cx: &mut Context<Self>) {
        slot(self).draft.choose(project, pick);
        cx.notify();
    }

    fn adjust_launch(&mut self, slot: Slot, project: Option<String>, pick: LaunchPick, cx: &mut Context<Self>) {
        slot(self).draft.adjust(project, pick);
        cx.notify();
    }

    pub(crate) fn access_chip(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        if self.agents.provider_info(&pick.provider).never_asks() {
            return div().child(launch::full_access_chip("composer-access", self.agent_name(&pick.provider)));
        }
        let (slot, open) = (c.slot, c.draft.menu == Some(Menu::Access));
        let app = self.store.agents.access();
        let chip = launch::access_chip("composer-access", open, &pick.access, app, move |this, cx| this.toggle_composer(slot, Menu::Access, cx), cx);
        let menu = open.then(|| {
            let current = pick.clone();
            let choose: Pick = Rc::new(move |this, access, cx| this.choose_launch(slot, project.clone(), LaunchPick { access, ..current.clone() }, cx));
            menu("composer-access-menu", "Access", 300., launch::access_rows("composer-access-row", launch::access_choices(app), &pick.access, choose, cx), slot, cx)
        });
        div().relative().child(chip).children(menu)
    }
}

/// Where the prompt box's text sits; the textarea pads itself 8 × 10, so its text starts 20 in.
pub(crate) fn prompt_text() -> Div {
    div().pt(px(11.)).px(px(9.))
}

/// The prompt box's bottom row: its settings, then what attaches and sends.
pub(crate) fn prompt_bar() -> Div {
    div().flex().items_center().gap(px(4.)).pt(px(4.)).px(px(11.)).pb(px(11.))
}

/// A menu's name over its rows.
pub(crate) fn menu_title(title: impl Into<SharedString>) -> Div {
    div().px(px(10.)).pt(px(4.)).pb(px(6.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child(title.into())
}

/// A setting in the prompt box's bar, as model or access.
pub(crate) fn pill() -> Div {
    div().h(px(30.)).flex_none().flex().items_center().gap(px(6.)).pl(px(7.)).pr(px(6.)).rounded(px(9.)).text_size(px(12.5))
}

/// `pill` as a picker, shaded while its menu is open.
pub(crate) fn picker(pill: Div, id: impl Into<ElementId>, open: bool) -> Stateful<Div> {
    pill.id(id).cursor_pointer().when(open, |d| d.bg(FILL_2)).hover(|s| s.bg(FILL_2)).child(icon("chevron-down", 12., TEXT_4))
}

/// A menu's `rows` under `title`.
pub(crate) fn menu_list(id: impl Into<ElementId>, title: impl Into<SharedString>, width: f32, rows: impl IntoIterator<Item = impl IntoElement>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(model_menu::PAD))
        .flex()
        .flex_col()
        .gap(px(model_menu::GAP))
        .child(menu_title(title))
        .children(rows)
}

/// A chip's menu, closed by a press outside it.
pub(crate) fn menu(id: &'static str, title: &'static str, width: f32, rows: Vec<AnyElement>, slot: Slot, cx: &mut Context<Desktop>) -> Div {
    let menu = menu_list(id, title, width, rows)
        // Menus hang over the page; without this a click on one also lands on what it covers.
        .occlude()
        .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            slot(this).draft.menu = None;
            cx.notify();
        }));
    ui::dropdown(36., ui::menu_in(SharedString::from(format!("{id}-in")), menu))
}

#[cfg(test)]
mod tests {
    use super::{Attachment, Draft, Menu, Pasted, Sent, ask_team, attachment_path, copied_files, pasted, pasted_image, pasted_path, save_private, split_attachments, team_offered};
    use serde_json::json;
    use gpui_kit::{ClipboardEntry, ClipboardItem, ClipboardString, ExternalPaths, Image, ImageFormat};
    use store::LaunchPick;

    fn codex() -> LaunchPick {
        LaunchPick { provider: "codex".into(), ..LaunchPick::default() }
    }

    #[test]
    fn an_agent_picked_here_holds_only_for_the_project_it_was_picked_in() {
        let mut draft = Draft::default();
        draft.choose(Some("/repo".into()), codex());
        assert_eq!(draft.pick(Some("/repo"), LaunchPick::default).provider, "codex");
        assert_eq!(draft.pick(Some("/other"), LaunchPick::default).provider, "");
        assert_eq!(draft.pick(None, LaunchPick::default).provider, "");
        draft.clear();
        assert_eq!(draft.pick(Some("/repo"), LaunchPick::default).provider, "");
    }

    #[test]
    fn picking_closes_the_menu_and_one_menu_is_open_at_a_time() {
        let mut draft = Draft::default();
        assert!(draft.toggle(Menu::Model));
        assert!(draft.toggle(Menu::Access));
        assert!(draft.menu == Some(Menu::Access));
        assert!(!draft.toggle(Menu::Access));
        draft.toggle(Menu::Model);
        draft.choose(None, codex());
        assert!(draft.menu.is_none());
    }

    #[test]
    fn esc_closes_the_model_panel_and_leaves_other_menus_alone() {
        let mut draft = Draft { menu: Some(Menu::Model), ..Draft::default() };
        assert!(draft.escape_model());
        assert!(draft.menu.is_none());
        assert!(!draft.escape_model());
        let mut draft = Draft { menu: Some(Menu::Access), ..Draft::default() };
        assert!(!draft.escape_model());
        assert!(draft.menu == Some(Menu::Access));
    }

    #[test]
    fn viewing_another_rail_entry_closes_the_list_of_efforts() {
        let mut draft = Draft { menu: Some(Menu::Model), ..Draft::default() };
        draft.effort.toggle(&"Effort".into());
        draft.view("codex");
        assert_eq!(draft.viewing, Some("codex"));
        assert!(!draft.effort.fold());
    }

    #[test]
    fn esc_closes_the_list_of_efforts_before_the_model_panel() {
        let mut draft = Draft { menu: Some(Menu::Model), ..Draft::default() };
        draft.effort.toggle(&"Effort".into());
        assert!(draft.escape_model());
        assert!(draft.menu == Some(Menu::Model));
        assert!(draft.escape_model());
        assert!(draft.menu.is_none());
    }

    #[test]
    fn setting_an_effort_leaves_the_panel_open_and_picking_a_model_closes_it() {
        let mut draft = Draft { menu: Some(Menu::Model), ..Draft::default() };
        draft.adjust(None, codex());
        assert!(draft.menu == Some(Menu::Model));
        assert_eq!(draft.pick(None, LaunchPick::default).provider, "codex");
        draft.choose(None, codex());
        assert!(draft.menu.is_none());
    }

    #[test]
    fn esc_on_the_account_popover_steps_back_to_its_usage_then_closes_it() {
        let mut draft = Draft { menu: Some(Menu::Switch), ..Draft::default() };
        assert!(draft.escape_account());
        assert!(draft.menu == Some(Menu::Usage { back: true }));
        assert!(draft.escape_account());
        assert!(draft.menu.is_none());
        draft.toggle(Menu::Model);
        assert!(!draft.escape_account());
        assert!(draft.menu == Some(Menu::Model));
    }

    #[test]
    fn reopening_the_model_menu_points_at_the_picked_agent() {
        let mut draft = Draft::default();
        draft.toggle(Menu::Model);
        draft.viewing = Some("codex");
        draft.toggle(Menu::Model);
        draft.toggle(Menu::Model);
        assert_eq!(draft.viewing, None);
    }


    #[test]
    fn an_image_paste_is_saved_to_a_private_file_and_text_pastes_as_itself() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("pocket-paste-test-{}", std::process::id()));
        let image = pasted_image(&ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]))).unwrap();
        let path = attachment_path(&image, &root.join("attachments"));
        save_private(&image.bytes, &path).unwrap();
        assert_eq!((path.extension().and_then(|e| e.to_str()), std::fs::read(&path).unwrap()), (Some("png"), vec![1, 2, 3]));
        #[cfg(unix)]
        assert_eq!(std::fs::metadata(root.join("attachments")).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(pasted_image(&ClipboardItem::new_string("fix ci".into())).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_copied_file_pastes_as_its_path_even_beside_its_pixels() {
        let shot = ExternalPaths(vec!["/r/shot.png".into()].into());
        let png = ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, vec![1]));
        let item = ClipboardItem { entries: vec![ClipboardEntry::ExternalPaths(shot.clone()), ClipboardEntry::String(ClipboardString::new("shot.png".into())), png] };
        assert_eq!(copied_files(&item).map(|p| p.paths()), Some(shot.paths()));
        assert!(copied_files(&ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1]))).is_none());
    }

    #[test]
    fn a_paste_is_read_as_files_a_link_a_long_text_an_image_or_text_as_itself() {
        let text = |t: &str| ClipboardItem::new_string(t.into());
        let files = ClipboardItem { entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(vec!["/r/a.rs".into()].into()))] };
        assert!(matches!(pasted(&files, ""), Pasted::Files(_)));
        assert_eq!(pasted(&text("https://a.dev"), "the docs"), Pasted::Link("[the docs](https://a.dev)".into()));
        assert_eq!(pasted(&text("https://a.dev"), ""), Pasted::Text, "nothing selected to link");
        let log = "line\n".repeat(81);
        assert_eq!(pasted(&text(&log), "the docs"), Pasted::Long(log.clone()));
        assert_eq!(pasted(&text("fix ci"), ""), Pasted::Text);
        let png = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1]));
        assert!(matches!(pasted(&png, ""), Pasted::Image(_)));
    }

    #[test]
    fn the_same_image_attaches_once_and_only_one_still_held_is_dropped_as_unsaved() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let at = |path: &str| Attachment { path: path.into(), image: image.clone().into() };
        let mut draft = Draft::default();
        assert!(draft.attach(at("/h/a.png")));
        assert!(!draft.attach(at("/h/a.png")));
        assert!(draft.attach(at("/h/b.png")));
        assert!(!draft.unsaved("/h/c.png".as_ref()));
        assert_eq!(draft.images.len(), 2);
        assert!(draft.unsaved("/h/a.png".as_ref()));
        assert_eq!(draft.images.iter().map(|a| a.path.to_str().unwrap()).collect::<Vec<_>>(), ["/h/b.png"]);
    }

    #[test]
    fn dropped_files_attach_where_they_lie_images_apart_from_the_rest() {
        let mut draft = Draft::default();
        draft.drop_files(&["/r/shot.PNG".into(), "/r/main.rs".into(), "/r/a b.jpg".into(), "/r/notes".into(), "/r/main.rs".into()]);
        assert_eq!(draft.files, [std::path::PathBuf::from("/r/main.rs"), "/r/notes".into()], "a file attaches once");
        assert_eq!(draft.prompt("Fix CI"), "Fix CI\n\n# Attached files\n\n- /r/main.rs\n- /r/notes\n\n# Attached images\n\n- /r/shot.PNG\n- /r/a b.jpg");
        assert!(draft.has_attachments() && !Draft::default().has_attachments());
    }

    #[test]
    fn a_long_paste_is_kept_in_a_file_named_by_what_it_holds() {
        let dir = std::path::Path::new("/h/attachments");
        assert_eq!(pasted_path("a log", dir), pasted_path("a log", dir));
        assert_ne!(pasted_path("a log", dir), pasted_path("another", dir));
        let name = pasted_path("a log", dir).file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("pasted-") && name.ends_with(".txt"), "{name}");
        let mut draft = Draft::default();
        assert!(draft.attach_file(pasted_path("a log", dir)));
        assert!(!draft.attach_file(pasted_path("a log", dir)));
        assert!(draft.unsaved(&pasted_path("a log", dir)));
        assert!(draft.files.is_empty());
    }

    #[test]
    fn attached_images_follow_the_prompt_as_paths() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let at = |path: &str| Attachment { path: path.into(), image: image.clone().into() };
        let draft = Draft { images: vec![at("/h/attachments/a.png"), at("/h/attachments/b.png")], ..Draft::default() };
        let block = "# Attached images\n\n- /h/attachments/a.png\n- /h/attachments/b.png";
        assert_eq!(draft.prompt(" Fix CI "), format!("Fix CI\n\n{block}"));
        assert_eq!(draft.prompt(""), block);
        assert_eq!(Draft::default().prompt(" Fix CI "), "Fix CI");
    }

    #[test]
    fn a_sent_prompt_splits_back_into_its_text_and_image_paths() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let at = |path: &str| Attachment { path: path.into(), image: image.clone().into() };
        let draft = Draft { images: vec![at("/h/a.png"), at("/h/b c.png")], ..Draft::default() };
        let sent = |text, files, images| Sent { text, files, images };
        assert_eq!(split_attachments(&draft.prompt("Fix CI")), sent("Fix CI", vec![], vec!["/h/a.png", "/h/b c.png"]));
        assert_eq!(split_attachments(&draft.prompt("")), sent("", vec![], vec!["/h/a.png", "/h/b c.png"]));
        assert_eq!(split_attachments("Fix CI"), sent("Fix CI", vec![], vec![]));
        let typed = "Explain\n\n# Attached images\n\nthis heading";
        assert_eq!(split_attachments(typed), sent(typed, vec![], vec![]));
    }

    #[test]
    fn a_sent_prompt_splits_back_into_its_files_too_with_or_without_images() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let files = vec!["/r/main.rs".into(), "/r/a b.pdf".into()];
        let both = Draft { files: files.clone(), images: vec![Attachment { path: "/h/a.png".into(), image: image.into() }], ..Draft::default() };
        assert_eq!(split_attachments(&both.prompt("Fix CI")), Sent { text: "Fix CI", files: vec!["/r/main.rs", "/r/a b.pdf"], images: vec!["/h/a.png"] });
        let only = Draft { files, ..Draft::default() };
        assert_eq!(split_attachments(&only.prompt("")), Sent { text: "", files: vec!["/r/main.rs", "/r/a b.pdf"], images: vec![] });
    }

    #[test]
    fn the_add_menus_team_row_sets_the_chip_and_closes_the_menu_and_a_sent_draft_drops_it() {
        let mut draft = Draft::default();
        assert!(draft.toggle(Menu::Add));
        draft.toggle_team();
        assert!(draft.team && draft.menu.is_none());
        draft.toggle(Menu::Add);
        draft.toggle_team();
        assert!(!draft.team, "picked again, it comes off");
        draft.toggle_team();
        draft.clear();
        assert!(!draft.team, "the next draft starts without it");
    }

    #[test]
    fn team_is_offered_only_to_a_chat_claude_or_codex_drives_on_a_pocketd_that_serves_teams() {
        assert!(team_offered(true, true, "claude"));
        assert!(team_offered(true, true, "codex"));
        assert!(!team_offered(false, true, "claude"), "pocketd doesn't advertise team.v1");
        assert!(!team_offered(true, false, "claude"), "a terminal session can't lead");
        assert!(!team_offered(true, true, "opencode"), "only Claude and Codex lead teams");
        assert!(!team_offered(true, true, "gemini"), "a provider this build doesn't know");
    }

    #[test]
    fn a_create_asks_for_a_team_only_when_the_chip_is_set_on_a_chat_that_can_lead() {
        let ask = |mut spec: serde_json::Value, team: bool, served: bool| {
            ask_team(&mut spec, team, served);
            spec.get("team").cloned()
        };
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), true, true), Some(json!(true)));
        assert_eq!(ask(json!({"provider": "codex", "chat": true}), true, true), Some(json!(true)));
        assert_eq!(ask(json!({"provider": "claude"}), true, true), None);
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), false, true), None);
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), true, false), None);
    }
}
