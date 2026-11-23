pub(crate) mod effort_menu;
pub(crate) mod effort_slider;
pub(crate) mod element_card;
pub(crate) mod elements;
pub(crate) mod file_card;
pub(crate) mod launch;
pub(crate) mod model_menu;
pub(crate) mod pills;
pub(crate) mod rich;

use crate::chat::composer::ComposerState;
use crate::chat::composer::attachments::attach_button;
use crate::chat::composer::{INPUT_LINE_HEIGHT, INPUT_TEXT_SIZE, frame, hand_focus, marks_overlay};
use crate::desktop::Desktop;
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::remote::Client;
use launch::Pick;
use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use store::LaunchPick;
use theme::*;

/// The prompt box that starts an agent: the words, the agent, model and access to start it with, and attached files
/// and images.
pub(crate) struct Composer {
    pub(crate) prompt: Entity<TextareaState>,
    /// Narrows the model menu's list.
    pub(crate) search: Entity<InputState>,
    pub(crate) draft: Draft,
}

/// What a composer tells its owner.
pub(crate) enum ComposerEvent {
    /// One of its menus opened: the owner closes its own.
    Opened,
    /// The model panel opened on `provider` in `project`.
    ModelsOpened {
        provider: String,
        project: Option<String>,
    },
    /// The model panel points at a rail entry.
    Viewed(&'static str),
    /// A model was clicked; `from` is the pick it changes.
    Picked {
        provider: String,
        model: String,
        from: LaunchPick,
        project: Option<String>,
    },
    Starred {
        provider: String,
        model: String,
    },
    /// The + was pressed: the owner asks for files to attach.
    Attach,
    /// A pasted image or long text couldn't be saved and left the draft: its title and the error.
    Unsaved(&'static str, String),
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub(crate) fn new(prompt: Entity<TextareaState>, search: Entity<InputState>) -> Self {
        Self { prompt, search, draft: Draft::default() }
    }

    /// Opens or closes `menu`, saying whether it is open now.
    pub(crate) fn toggle(&mut self, menu: Menu, cx: &mut Context<Self>) -> bool {
        let open = self.draft.toggle(menu);
        if open {
            cx.emit(ComposerEvent::Opened);
        }
        cx.notify();
        open
    }

    /// Sets the pick and closes the menu.
    fn choose(&mut self, project: Option<String>, pick: LaunchPick, cx: &mut Context<Self>) {
        self.draft.choose(project, pick);
        cx.notify();
    }

    /// The access the launch starts with, and the menu that picks it; `chip` says what both draw.
    pub(crate) fn access_chip(&self, pick: &LaunchPick, project: Option<String>, chip: &AccessChip, cx: &mut Context<Self>) -> Div {
        let (app, choices) = match chip {
            AccessChip::Full(agent) => return div().child(launch::full_access_chip("composer-access", agent.clone())),
            AccessChip::Choices { app, choices } => (app, choices),
        };
        let open = self.draft.menu == Some(Menu::Access);
        let trigger = launch::access_chip("composer-access", open, &pick.access, app, |this: &mut Self, cx| _ = this.toggle(Menu::Access, cx), cx);
        let menu = open.then(|| {
            let current = pick.clone();
            let choose: Pick<Self> = Rc::new(move |this, access, cx| this.choose(project.clone(), LaunchPick { access, ..current.clone() }, cx));
            menu("composer-access-menu", "Access", 300., launch::access_rows("composer-access-row", choices.clone(), &pick.access, choose, cx), cx)
        });
        div().relative().child(trigger).children(menu)
    }

    /// The box around `textarea`, as the chat's.
    pub(crate) fn frame(&mut self, textarea: Textarea, f: Frame, cx: &mut Context<Self>) -> Div {
        let Frame { project, typing, chips, attachments, pills, trailing, send } = f;
        let (prompt, search, menu, slider) = (self.prompt.clone(), self.search.clone(), self.draft.menu, self.draft.slider.clone());
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
                    take(&mut this.draft, i);
                    cx.notify();
                }))
        };
        let draft = &self.draft;
        let attached = (!pills.is_empty() || draft.has_attachments()).then(|| {
            let images = draft.images.iter().enumerate().map(|(i, a)| {
                let image = img(a.image.clone()).size_full().rounded(px(10.)).object_fit(ObjectFit::Cover);
                div().relative().size(px(56.)).flex_none().rounded(px(10.)).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).child(image).child(remove(
                    ("image-remove", i).into(),
                    |d, i| _ = d.images.remove(i),
                    i,
                ))
            });
            let files = draft.files.iter().enumerate().map(|(i, path)| {
                div().relative().flex_none().child(file_card::file_card(("composer-file", i), path)).child(remove(
                    ("file-remove", i).into(),
                    |d, i| _ = d.files.remove(i),
                    i,
                ))
            });
            div().flex().flex_wrap().items_center().gap(px(8.)).pt(px(14.)).px(px(16.)).children(pills).children(images).children(files)
        });
        let textarea = pills::with_pills(textarea, &prompt).appearance(false).text_size(px(INPUT_TEXT_SIZE)).line_height(px(INPUT_LINE_HEIGHT)).on_paste({
            let (this, prompt, attachments) = (cx.weak_entity(), prompt.clone(), attachments.to_path_buf());
            move |item, window, cx| paste_in(&this, &prompt, &attachments, item, window, cx)
        });
        let pick = chips.launch.pick.clone();
        let attach = attach_button(
            "composer-attach",
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.draft.menu = None;
                cx.emit(ComposerEvent::Attach);
                cx.notify();
            }),
        );
        let bar = prompt_bar()
            .child(self.model_pill(&chips.launch, cx))
            .child(self.access_chip(&pick, project, &chips.access, cx))
            .child(div().flex_1())
            .children(trailing)
            .child(attach)
            .child(send);
        // The search or the effort slider takes the keys while the model panel is open; closing it would leave them nowhere.
        let handoff = (menu != Some(Menu::Model)).then(|| {
            let from = std::iter::once(search.read(cx).focus_handle(cx)).chain(slider.borrow().focus.clone()).collect();
            hand_focus(from, prompt.read(cx).focus_handle(cx))
        });
        frame(typing)
            .children(handoff)
            .on_mouse_down(MouseButton::Left, {
                let prompt = prompt.clone();
                move |_, window, cx| prompt.update(cx, |s, cx| s.focus(window, cx))
            })
            .children(attached)
            .child(prompt_text().child(textarea).child(marks_overlay(prompt)))
            .child(bar)
    }

    /// For a prompt's `on_drop_paths`: dropped files attach to this composer's draft, so none is typed in as its path.
    pub(crate) fn dropped(this: WeakEntity<Self>) -> impl Fn(&[PathBuf], &mut Window, &mut App) -> Vec<PathBuf> {
        move |paths, _, cx| match this.update(cx, |c, cx| {
            c.draft.drop_files(paths);
            cx.notify();
        }) {
            Ok(()) => Vec::new(),
            Err(_) => paths.to_vec(),
        }
    }
}

/// What a composer's model pill and access chip draw from.
pub(crate) struct Chips {
    pub(crate) launch: model_menu::Launch,
    pub(crate) access: AccessChip,
}

/// What an owner hands a composer to draw its box.
pub(crate) struct Frame<'a> {
    pub(crate) project: Option<String>,
    pub(crate) typing: bool,
    pub(crate) chips: &'a Chips,
    /// Where a pasted image or long text is kept.
    pub(crate) attachments: &'a Path,
    /// Lead the attachments above the prompt.
    pub(crate) pills: Vec<AnyElement>,
    /// Come before the + and `send`.
    pub(crate) trailing: Vec<AnyElement>,
    pub(crate) send: Stateful<Div>,
}

/// What the access chip draws: the access of an agent that never asks, or the choices of one that does.
pub(crate) enum AccessChip {
    Full(String),
    Choices { app: String, choices: Vec<(String, String, String)> },
}

/// Whose draft a helper edits: the chat's, which `Desktop` holds, or a composer's.
#[derive(Clone)]
pub(crate) enum Held {
    Chat,
    Composer(Entity<Composer>),
}

impl Held {
    pub(crate) fn edit<R>(&self, d: &mut Desktop, cx: &mut App, f: impl FnOnce(&mut Draft) -> R) -> R {
        match self {
            Held::Chat => d.chats.composer.update(cx, |c, cx| {
                cx.notify();
                f(&mut c.draft)
            }),
            Held::Composer(c) => c.update(cx, |c, cx| {
                cx.notify();
                f(&mut c.draft)
            }),
        }
    }
}

/// The chip whose menu is open.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    /// The model pill's panel: its agents, models and efforts.
    Model,
    Access,
    /// The chat's account popover on its usage, slid `back` to from its accounts.
    Usage {
        back: bool,
    },
    /// The chat's account popover on the accounts to switch to.
    Switch,
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
    pub(crate) slider: Rc<RefCell<effort_slider::EffortSlider>>,
    revealed: Vec<String>,
    pub(crate) images: Vec<Attachment>,
    /// The dropped files and long pastes, which the agent reads where they lie.
    pub(crate) files: Vec<PathBuf>,
    /// Elements picked on a browser page, numbered by their place here.
    pub(crate) elements: Vec<web::Element>,
}

/// A `Draft`'s model panel as it stands: the handles it draws with, which share the draft's.
pub(crate) struct PanelView {
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) rail: ScrollHandle,
    pub(crate) slider: Rc<RefCell<effort_slider::EffortSlider>>,
    pub(crate) effort: effort_menu::EffortMenu,
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

    /// What the model panel draws from, to hold while the panel draws.
    pub(crate) fn view_of(&self) -> PanelView {
        PanelView { scroll: self.scroll.clone(), rail: self.rail.clone(), slider: self.slider.clone(), effort: self.effort.clone() }
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
        // The slider may hold the keys: its handle outlives the reset so the prompt's handoff can still take them back.
        let focus = self.slider.borrow_mut().focus.take();
        self.slider = Default::default();
        self.slider.borrow_mut().focus = focus;
        self.menu.is_some()
    }

    /// Opens or closes the chat's account popover, with every email in it masked again.
    pub(crate) fn toggle_account(&mut self) {
        let open = matches!(self.menu, Some(Menu::Usage { .. } | Menu::Switch));
        self.menu = (!open).then_some(Menu::Usage { back: false });
        self.revealed.clear();
    }

    pub(crate) fn toggle_email(&mut self, id: &str) {
        match self.revealed.iter().position(|s| s == id) {
            Some(i) => _ = self.revealed.remove(i),
            None => self.revealed.push(id.to_string()),
        }
    }

    pub(crate) fn reveals(&self, id: &str) -> bool {
        self.revealed.iter().any(|s| s == id)
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

    /// Esc on the model panel closes a setting's list of values, else the panel. False while it isn't open.
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

    pub(crate) fn holds(&self, path: &Path) -> bool {
        self.images.iter().any(|a| a.path == path) || self.files.iter().any(|f| f == path)
    }

    pub(crate) fn has_attachments(&self) -> bool {
        !self.images.is_empty() || !self.files.is_empty() || !self.elements.is_empty()
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

    /// Whether the element is new to this draft: one picked again on the same page isn't added twice.
    pub(crate) fn pick_element(&mut self, element: web::Element) -> bool {
        let new = !self.elements.iter().any(|e| e.url == element.url && e.selector == element.selector);
        if new {
            self.elements.push(element);
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
        self.prompt_with(typed, &[])
    }

    /// `prompt` for an agent on a host, naming each file in `copies` by its copy there.
    pub(crate) fn prompt_with(&self, typed: &str, copies: &[(PathBuf, String)]) -> String {
        let named = |p: &Path| copies.iter().find(|(from, _)| from == p).map_or_else(|| p.display().to_string(), |(_, to)| hosts::split(to).1.to_string());
        let block = |title: &str, paths: Vec<&Path>| {
            (!paths.is_empty()).then(|| format!("{title}{}", paths.iter().map(|p| format!("- {}", named(p))).collect::<Vec<_>>().join("\n")))
        };
        let typed = typed.trim();
        let elements = (!self.elements.is_empty()).then(|| elements::block(&self.elements));
        let parts = [
            (!typed.is_empty()).then(|| typed.to_string()),
            block(FILES, self.files.iter().map(PathBuf::as_path).collect()),
            block(IMAGES, self.images.iter().map(|a| a.path.as_path()).collect()),
            elements,
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join("\n\n")
    }

    /// Each attached file on this Mac, and the key of its copy in `dir`, a host's attachments folder. One already in this Mac's
    /// attachments folder `here` keeps its name there; any other gets its path's hash in front, so two files of one name don't collide.
    fn host_copies(&self, here: &Path, dir: &str) -> Vec<(PathBuf, String)> {
        let on_mac = self.images.iter().map(|a| &a.path).chain(&self.files).filter(|p| hosts::split(&p.to_string_lossy()).0.is_none());
        on_mac.map(|p| (p.clone(), format!("{dir}/{}", copy_name(p, here)))).collect()
    }
}

fn copy_name(file: &Path, here: &Path) -> String {
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    if file.parent() == Some(here) {
        return name.into_owned();
    }
    format!("{:016x}-{name}", hashed(file))
}

fn hashed(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The files a prompt attaches from this Mac, to copy to the host whose agent reads them.
pub(crate) struct HostCopies {
    client: Arc<Client>,
    /// The host's attachments folder, as its key.
    dir: String,
    /// Each file here and its copy's key there.
    pub(crate) files: Vec<(PathBuf, String)>,
    /// Each image from outside this Mac's attachments folder and its copy in there under the name it has on the host, which
    /// a sent prompt's thumbnail falls back to.
    thumbs: Vec<(PathBuf, PathBuf)>,
}

impl HostCopies {
    /// Blocks on the host's replies.
    pub(crate) fn make(&self) -> std::io::Result<()> {
        self.client.mkdir(&self.dir, true)?;
        self.files.iter().try_for_each(|(from, to)| self.client.write(to, &std::fs::read(from)?, false))?;
        self.thumbs.iter().try_for_each(|(from, to)| save_private(&std::fs::read(from)?, to))
    }
}

/// What `draft` must copy to the host holding `tree` before an agent there can read its attachments: into the attachments
/// folder of pocketd's own folder there. None when `tree` is on this Mac or nothing is attached from it. `here` is this
/// Mac's attachments folder. Err names a host that isn't connected.
pub(crate) fn host_copies(draft: &Draft, tree: &str, here: &Path) -> Result<Option<HostCopies>, String> {
    let Some(host) = hosts::split(tree).0 else { return Ok(None) };
    let client = hosts::remote::connected(host);
    let dir = client.as_ref().and_then(|c| c.pocket_dir()).map(|pocket| hosts::place(host, &format!("{pocket}/attachments")));
    let files = draft.host_copies(here, dir.as_deref().unwrap_or_default());
    if files.is_empty() {
        return Ok(None);
    }
    let (Some(client), Some(dir)) = (client, dir) else { return Err(host.to_string()) };
    let outside = draft.images.iter().map(|a| &a.path).filter(|p| p.parent() != Some(here) && files.iter().any(|(from, _)| from == *p));
    let thumbs = outside.map(|p| (p.clone(), here.join(copy_name(p, here)))).collect();
    Ok(Some(HostCopies { client, dir, files, thumbs }))
}

const FILES: &str = "# Attached files\n\n";
const IMAGES: &str = "# Attached images\n\n";

/// A sent prompt's typed text, and the paths attached to it.
#[derive(Debug, PartialEq)]
pub(crate) struct Sent<'a> {
    pub(crate) text: &'a str,
    pub(crate) files: Vec<&'a str>,
    pub(crate) images: Vec<&'a str>,
    /// Each picked element's heading: `<n> <host> › <title>`.
    pub(crate) elements: Vec<&'a str>,
}

/// Undoes [`Draft::prompt`].
pub(crate) fn split_attachments(prompt: &str) -> Sent<'_> {
    let (rest, elements) = elements::split(prompt);
    let (rest, images) = split_block(rest, IMAGES);
    let (text, files) = split_block(rest, FILES);
    Sent { text, files, images, elements }
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
    dir.join(format!("{}{:016x}.txt", file_card::PASTED, hashed(text)))
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
fn save_behind<T: 'static>(
    owner: WeakEntity<T>,
    path: PathBuf,
    save: impl FnOnce(&Path) -> std::io::Result<()> + Send + 'static,
    failed: impl FnOnce(&mut T, &Path, String, &mut Context<T>) + 'static,
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
                owner
                    .update(cx, |o, cx| {
                        failed(o, &path, e.to_string(), cx);
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
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

type Write = Box<dyn FnOnce(&Path) -> std::io::Result<()> + Send>;

/// A paste kept as a file: the draft holds `image` with it, or only the file, and `save` writes it behind the UI.
struct Kept {
    path: PathBuf,
    image: Option<Arc<Image>>,
    save: Write,
    failed: &'static str,
}

fn kept(pasted: Pasted, dir: &Path) -> Option<Kept> {
    match pasted {
        Pasted::Long(text) => Some(Kept {
            path: pasted_path(&text, dir),
            image: None,
            save: Box::new(move |path| save_private(text.as_bytes(), path)),
            failed: "Couldn't save the pasted text",
        }),
        Pasted::Image(image) => {
            let image = Arc::new(image);
            Some(Kept {
                path: attachment_path(&image, dir),
                image: Some(image.clone()),
                save: Box::new(move |path| save_private(&image.bytes, path)),
                failed: "Couldn't save the pasted image",
            })
        }
        _ => None,
    }
}

impl Draft {
    fn hold(&mut self, kept: &Kept) -> bool {
        match &kept.image {
            Some(image) => self.attach(Attachment { path: kept.path.clone(), image: image.clone().into() }),
            None => self.attach_file(kept.path.clone()),
        }
    }
}

/// An owner of a draft a paste goes into.
pub(crate) trait Drafted: Sized + 'static {
    fn draft(&mut self) -> &mut Draft;
    /// An attachment that couldn't be written, titled `title`, left the draft.
    fn unsaved(&mut self, title: &'static str, error: String, cx: &mut Context<Self>);
}

impl Drafted for Composer {
    fn draft(&mut self) -> &mut Draft {
        &mut self.draft
    }

    fn unsaved(&mut self, title: &'static str, error: String, cx: &mut Context<Self>) {
        cx.emit(ComposerEvent::Unsaved(title, error));
    }
}

/// Takes a paste into `this`'s draft as `pasted` reads it, and says whether it did; text that pastes as itself is left
/// to the input.
pub(crate) fn paste_in<T: Drafted>(
    this: &WeakEntity<T>,
    input: &Entity<TextareaState>,
    dir: &Path,
    item: &ClipboardItem,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let state = input.read(cx);
    match pasted(item, &state.value()[state.selected_range()]) {
        Pasted::Text => false,
        Pasted::Files(paths) => {
            input.update(cx, |s, cx| s.drop_paths(paths, window, cx));
            true
        }
        Pasted::Link(link) => {
            input.update(cx, |s, cx| s.replace_text_in_range(None, &link, window, cx));
            true
        }
        pasted => {
            let Some(kept) = kept(pasted, dir) else { return false };
            let held = this.update(cx, |t, cx| {
                cx.notify();
                t.draft().hold(&kept)
            });
            let Ok(held) = held else { return false };
            if held {
                let Kept { path, save, failed, .. } = kept;
                let unsaved = move |t: &mut T, path: &Path, error: String, cx: &mut Context<T>| {
                    if t.draft().unsaved(path) {
                        t.unsaved(failed, error, cx);
                    }
                };
                save_behind(this.clone(), path, save, unsaved, window, cx);
            }
            true
        }
    }
}

/// For a prompt's `on_drop_paths`: dropped files attach to the draft `draft` picks, so none is typed in as its path.
pub(crate) fn drop_handler(
    desktop: WeakEntity<Desktop>,
    draft: impl Fn(&Desktop) -> Entity<ComposerState> + 'static,
) -> impl Fn(&[PathBuf], &mut Window, &mut App) -> Vec<PathBuf> {
    move |paths, _, cx| match desktop.upgrade() {
        Some(d) => d.update(cx, |d, cx| {
            draft(d).update(cx, |c, cx| {
                cx.notify();
                c.draft.drop_files(paths);
            });
            Vec::new()
        }),
        None => paths.to_vec(),
    }
}

impl Desktop {
    fn access_chip_of(&self, pick: &LaunchPick) -> AccessChip {
        if self.agents.provider_info(&pick.provider).never_asks() {
            AccessChip::Full(self.agent_name(&pick.provider))
        } else {
            let app = self.store.agents.access();
            AccessChip::Choices { app: app.into(), choices: launch::access_choices(app) }
        }
    }

    /// What `c`'s model pill and access chip draw from, for an owner that draws them itself.
    pub(crate) fn composer_chips(&self, c: &Entity<Composer>, pick: &LaunchPick, project: Option<String>, cx: &App) -> Chips {
        Chips { launch: self.launch(c, pick, project, cx), access: self.access_chip_of(pick) }
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
pub(crate) fn menu_list(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    width: f32,
    rows: impl IntoIterator<Item = impl IntoElement>,
) -> Stateful<Div> {
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
pub(crate) fn menu(id: &'static str, title: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Composer>) -> Div {
    let menu = menu_list(id, title, width, rows)
        // Menus hang over the page; without this a click on one also lands on what it covers.
        .occlude()
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.draft.menu = None;
            cx.notify();
        }));
    ui::dropdown(36., ui::menu_in(SharedString::from(format!("{id}-in")), menu))
}

#[cfg(test)]
mod tests {
    use super::{Attachment, Draft, Menu, Pasted, Sent, attachment_path, copied_files, pasted, pasted_image, pasted_path, save_private, split_attachments};
    use gpui_kit::{ClipboardEntry, ClipboardItem, ClipboardString, ExternalPaths, Image, ImageFormat};
    use std::path::Path;
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
    fn an_email_shown_in_the_account_popover_is_masked_again_once_it_reopens() {
        let mut draft = Draft::default();
        draft.toggle_account();
        assert!(draft.menu == Some(Menu::Usage { back: false }));
        draft.toggle_email("work");
        assert!(draft.reveals("work") && !draft.reveals("builtin"));
        draft.toggle_email("work");
        assert!(!draft.reveals("work"));
        draft.toggle_email("work");
        draft.toggle_account();
        assert!(draft.menu.is_none());
        draft.toggle_account();
        assert!(!draft.reveals("work"));
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
        let item =
            ClipboardItem { entries: vec![ClipboardEntry::ExternalPaths(shot.clone()), ClipboardEntry::String(ClipboardString::new("shot.png".into())), png] };
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
        let sent = |text, files, images| Sent { text, files, images, elements: vec![] };
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
        assert_eq!(
            split_attachments(&both.prompt("Fix CI")),
            Sent { text: "Fix CI", files: vec!["/r/main.rs", "/r/a b.pdf"], images: vec!["/h/a.png"], elements: vec![] }
        );
        let only = Draft { files, ..Draft::default() };
        assert_eq!(split_attachments(&only.prompt("")), Sent { text: "", files: vec!["/r/main.rs", "/r/a b.pdf"], images: vec![], elements: vec![] });
    }

    fn element(url: &str, text: &str, html: &str) -> web::Element {
        web::Element {
            url: url.into(),
            selector: "form > input".into(),
            name: "input.email".into(),
            text: text.into(),
            html: html.into(),
            width: 420,
            height: 41,
        }
    }

    #[test]
    fn picked_elements_follow_the_prompt_headed_by_their_number_host_and_words() {
        let draft = Draft { elements: vec![element("https://github.com/", "Enter your email", "<input>")], ..Draft::default() };
        assert_eq!(
            draft.prompt("Fix this"),
            "Fix this\n\n# Selected elements\n\n## 1 github.com › Enter your email\nURL: https://github.com/\nSelector: `form > input`\nSize: 420 × 41\nText: Enter your email\n```html\n<input>\n```"
        );
        assert!(draft.has_attachments());
    }

    #[test]
    fn an_element_picked_again_on_the_same_page_is_added_once() {
        let mut draft = Draft::default();
        assert!(draft.pick_element(element("https://a.dev/", "Hi", "<b>")));
        assert!(!draft.pick_element(element("https://a.dev/", "Hi", "<b>")));
        assert!(draft.pick_element(element("https://b.dev/", "Hi", "<b>")), "the same selector on another page is another element");
        assert_eq!(draft.elements.len(), 2);
    }

    #[test]
    fn a_sent_prompt_splits_back_into_its_elements_headings() {
        let draft = Draft {
            files: vec!["/r/main.rs".into()],
            elements: vec![element("https://github.com/", "Enter your email", "<p>\n## not a heading\n</p>"), element("https://github.com/", "", "<div>")],
            ..Draft::default()
        };
        let prompt = draft.prompt("Fix");
        let sent = split_attachments(&prompt);
        assert_eq!(
            sent,
            Sent {
                text: "Fix", files: vec!["/r/main.rs"], images: vec![], elements: vec!["1 github.com › Enter your email", "2 github.com › input.email"]
            }
        );
        let only = Draft { elements: vec![element("https://a.dev/", "Hi", "<b>")], ..Draft::default() };
        assert_eq!(split_attachments(&only.prompt("")).elements, vec!["1 a.dev › Hi"]);
        assert_eq!(split_attachments(&only.prompt("")).text, "");
    }

    #[test]
    fn files_attached_on_this_mac_go_to_a_host_agent_as_their_copies_there() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let mut draft = Draft { files: vec!["/Users/me/notes.md".into(), "//devbox/home/dev/code/app/README.md".into()], ..Draft::default() };
        draft.attach(Attachment { path: "/Users/me/.pocket/attachments/42.png".into(), image: image.into() });
        let dir = "//devbox/home/dev/.pocket/attachments";
        let copies = draft.host_copies(Path::new("/Users/me/.pocket/attachments"), dir);
        let named: Vec<&str> = copies.iter().map(|(_, to)| to.rsplit('/').next().unwrap()).collect();
        assert_eq!(copies.iter().map(|(from, _)| from.to_str().unwrap()).collect::<Vec<_>>(), ["/Users/me/.pocket/attachments/42.png", "/Users/me/notes.md"]);
        assert!(copies.iter().all(|(_, to)| to.starts_with(&format!("{dir}/"))));
        assert_eq!(named[0], "42.png");
        assert!(named[1].ends_with("-notes.md") && named[1].len() == "notes.md".len() + 17, "{}", named[1]);
        let prompt = draft.prompt_with("Look", &copies);
        assert!(
            prompt.contains("- /home/dev/.pocket/attachments/42.png") && prompt.contains(&format!("- /home/dev/.pocket/attachments/{}", named[1])),
            "{prompt}"
        );
        assert!(prompt.contains("- //devbox/home/dev/code/app/README.md") && !prompt.contains("/Users/me"), "{prompt}");
        assert!(draft.prompt("Look").contains("- /Users/me/notes.md"), "a send to this Mac after a copy for devbox names devbox's path");
    }

    #[cfg(unix)]
    #[test]
    fn copies_for_a_host_are_written_into_its_attachments_folder() {
        use base64::Engine;
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        let root = std::env::temp_dir().join(format!("pocket-copy-test-{}", std::process::id()));
        let here = root.join("attachments");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::write(here.join("7.png"), [9, 8, 7]).unwrap();
        let sock = root.join("h.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, ops) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let mut writer = stream.try_clone().unwrap();
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        let op: serde_json::Value = serde_json::from_str(&line).unwrap();
                        if op["op"] == "status" {
                            // A host whose pocketd hasn't moved to ~/.anywhere yet names its old folder.
                            writeln!(
                                writer,
                                "{}",
                                serde_json::json!({"ev": "status", "status": {"userHome": "/home/dev", "home": "/home/dev/.coding-pocket"}})
                            )
                            .unwrap();
                            continue;
                        }
                        writeln!(writer, "{}", serde_json::json!({"id": op["id"], "ev": op["op"]})).unwrap();
                        tx.send(op).unwrap();
                    }
                });
            }
        });
        let client = std::sync::Arc::new(hosts::remote::Client::new("copybox", sock));
        client.status().unwrap();
        hosts::remote::register(client);
        std::fs::write(root.join("drop.png"), [6]).unwrap();
        let mut draft = Draft { files: vec![here.join("7.png")], ..Draft::default() };
        draft.drop_files(&[root.join("drop.png")]);
        let copies = super::host_copies(&draft, "//copybox/home/dev/app", &here).unwrap().unwrap();
        copies.make().unwrap();
        let thumb = here.join(super::copy_name(&root.join("drop.png"), &here));
        assert_eq!(std::fs::read(&thumb).unwrap(), [6], "a dropped image's thumbnail is kept under its name on the host");
        hosts::remote::forget("copybox");
        let dir = "/home/dev/.coding-pocket/attachments";
        let op = || ops.recv_timeout(std::time::Duration::from_secs(5)).expect("the host got no op");
        let (mkdir, dropped, write) = (op(), op(), op());
        assert!(dropped["path"].as_str().unwrap().ends_with("-drop.png"), "{dropped}");
        assert_eq!((mkdir["op"].as_str(), mkdir["path"].as_str(), mkdir["parents"].as_bool()), (Some("fs.mkdir"), Some(dir), Some(true)));
        assert_eq!((write["op"].as_str(), write["path"].as_str()), (Some("fs.write"), Some(format!("{dir}/7.png").as_str())));
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(write["data"].as_str().unwrap()).unwrap(), [9, 8, 7]);
        assert!(super::host_copies(&draft, "/Users/me/app", &here).unwrap().is_none());
        assert_eq!(super::host_copies(&draft, "//gonebox/home/dev/app", &here).err().as_deref(), Some("gonebox"));
        assert!(super::host_copies(&Draft::default(), "//gonebox/home/dev/app", &here).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
