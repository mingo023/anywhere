pub(crate) mod launch;
pub(crate) mod model_menu;

use crate::desktop::Desktop;
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::*;
use launch::Pick;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use store::LaunchPick;
use theme::*;

/// Where a composer lives in `Desktop`, so its listeners can reach it.
pub(crate) type Slot = fn(&mut Desktop) -> &mut Composer;

/// The prompt box that starts an agent: the words, the agent, model and access to start it with, and pasted images.
pub(crate) struct Composer {
    pub(crate) prompt: Entity<TextareaState>,
    /// Narrows the model menu's list.
    search: Entity<InputState>,
    pub(crate) draft: Draft,
    slot: Slot,
    /// Closes the host's own menus as one of the composer's opens.
    opened: fn(&mut Desktop),
    /// Shows why a pasted image couldn't be saved.
    unsaved: fn(&mut Desktop, (String, String)),
}

impl Composer {
    pub(crate) fn new(prompt: Entity<TextareaState>, search: Entity<InputState>, slot: Slot, opened: fn(&mut Desktop), unsaved: fn(&mut Desktop, (String, String))) -> Self {
        Self { prompt, search, draft: Draft::default(), slot, opened, unsaved }
    }
}

/// The chip whose menu is open.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    Model,
    Access,
}

/// A composer's choices besides its words.
#[derive(Default)]
pub(crate) struct Draft {
    /// The agent picked here, and the Project it was picked for; another Project starts with its own.
    chosen: Option<(Option<String>, LaunchPick)>,
    pub(crate) menu: Option<Menu>,
    /// The model menu's side menu, opened by pointing at its row.
    pub(crate) side: Option<model_menu::Side>,
    /// The agent whose models the side menu lists, once one is pointed at.
    viewing: Option<&'static str>,
    pub(crate) images: Vec<Attachment>,
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

    fn choose(&mut self, project: Option<String>, pick: LaunchPick) {
        (self.chosen, self.menu) = (Some((project, pick)), None);
    }

    /// Whether `menu` is open now.
    pub(crate) fn toggle(&mut self, menu: Menu) -> bool {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        (self.side, self.viewing) = (None, None);
        self.menu.is_some()
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

    /// Attaches the dropped images where they lie and hands back the other files.
    pub(crate) fn drop_images(&mut self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let (images, rest): (Vec<PathBuf>, _) = paths.iter().cloned().partition(|p| is_image(p));
        for path in images {
            self.attach(Attachment { image: path.clone().into(), path });
        }
        rest
    }

    /// Drops the image whose file couldn't be written; false if this draft no longer held it.
    fn unsaved(&mut self, path: &Path) -> bool {
        let held = self.images.len();
        self.images.retain(|a| a.path != path);
        self.images.len() < held
    }

    /// The prompt goes to the agent as text, so images go in as paths it can read.
    pub(crate) fn prompt(&self, typed: &str) -> String {
        let typed = typed.trim();
        if self.images.is_empty() {
            return typed.to_string();
        }
        let list: Vec<String> = self.images.iter().map(|a| format!("- {}", a.path.display())).collect();
        let block = format!("{IMAGES}{}", list.join("\n"));
        if typed.is_empty() { block } else { format!("{typed}\n\n{block}") }
    }
}

const IMAGES: &str = "# Attached images\n\n";

/// Undoes [`Draft::prompt`]: the typed text and the attached image paths.
pub(crate) fn split_images(prompt: &str) -> (&str, Vec<&str>) {
    let at = match prompt.rfind(&format!("\n\n{IMAGES}")) {
        Some(i) => Some((i, i + 2)),
        None => prompt.starts_with(IMAGES).then_some((0, 0)),
    };
    let Some((end, block)) = at else { return (prompt, Vec::new()) };
    let paths: Option<Vec<&str>> = prompt[block + IMAGES.len()..].lines().map(|l| l.strip_prefix("- ")).collect();
    match paths {
        Some(paths) if !paths.is_empty() => (&prompt[..end], paths),
        _ => (prompt, Vec::new()),
    }
}

/// The image a paste carries when it has no text; text, copied files included, pastes as itself.
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

/// Owner-only, like the rest of Pocket's files: screenshots can hold secrets.
fn save_image(image: &Image, path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    std::fs::write(path, &image.bytes)
}

/// The image shows at once and its file is written behind it, so starting never waits on a write; the agent opens it long after.
/// Attaches a pasted image to the draft `draft` picks; `unsaved` shows why it couldn't be saved.
pub(crate) fn paste_image(
    desktop: &WeakEntity<Desktop>,
    draft: impl Fn(&mut Desktop) -> &mut Draft + 'static,
    unsaved: impl FnOnce(&mut Desktop, (String, String)) + 'static,
    item: &ClipboardItem,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let (Some(image), Some(d)) = (pasted_image(item), desktop.upgrade()) else { return false };
    let image = Arc::new(image);
    let path = attachment_path(&image, &d.read(cx).store.home().join("attachments"));
    let new = d.update(cx, |d, cx| {
        cx.notify();
        draft(d).attach(Attachment { path: path.clone(), image: image.clone().into() })
    });
    if !new {
        return true;
    }
    let save = cx.background_executor().spawn({
        let path = path.clone();
        async move { save_image(&image, &path) }
    });
    let desktop = desktop.clone();
    window
        .spawn(cx, async move |cx| {
            if let Err(e) = save.await {
                desktop
                    .update(cx, |d, cx| {
                        if draft(d).unsaved(&path) {
                            unsaved(d, ("Couldn't save the pasted image".into(), e.to_string()));
                        }
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    true
}

/// For the prompt's `on_drop_paths`: dropped images attach, other files go in as their paths.
pub(crate) fn drop_images(desktop: WeakEntity<Desktop>, slot: Slot) -> impl Fn(&[PathBuf], &mut Window, &mut App) -> Vec<PathBuf> {
    move |paths, _, cx| match desktop.upgrade() {
        Some(d) => d.update(cx, |d, cx| {
            cx.notify();
            slot(d).draft.drop_images(paths)
        }),
        None => paths.to_vec(),
    }
}

impl Desktop {
    /// The box around `textarea`; `pills` lead the attachments above it, `trailing` follows the chips.
    pub(crate) fn composer(&self, c: &Composer, project: Option<String>, textarea: Textarea, pills: Vec<AnyElement>, trailing: Vec<AnyElement>, cx: &mut Context<Self>) -> Div {
        let pick = c.draft.pick(project.as_deref(), || self.last_pick(project.as_deref()));
        let (slot, desktop) = (c.slot, cx.entity().downgrade());
        let attached = (!pills.is_empty() || !c.draft.images.is_empty()).then(|| {
            div().flex().items_center().gap(px(8.)).pt(px(14.)).px(px(16.)).children(pills).children(c.draft.images.iter().enumerate().map(|(i, a)| {
                let remove = div()
                    .id(("image-remove", i))
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
                        slot(this).draft.images.remove(i);
                        cx.notify();
                    }));
                div().relative().size(px(56.)).flex_none().rounded(px(10.)).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).child(img(a.image.clone()).size_full().rounded(px(10.)).object_fit(ObjectFit::Cover)).child(remove)
            }))
        });
        div()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .children(attached)
            // The textarea pads itself 8px × 10px and wraps 10px short of its edge; the frame restores the design's 14/16/4 and its line breaks.
            .child(div().pt(px(6.)).pl(px(6.)).mr(px(-6.)).child(textarea.appearance(false).text_size(px(15.)).on_paste(move |item, window, cx| paste_image(&desktop, move |d| &mut slot(d).draft, move |d, e| (slot(d).unsaved)(d, e), item, window, cx))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .pt(px(4.))
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .child(self.model_pill(c, &pick, project.clone(), cx))
                    .child(self.access_chip(c, &pick, project, cx))
                    .child(div().flex_1())
                    .children(trailing),
            )
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

    fn access_chip(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        let (slot, open) = (c.slot, c.draft.menu == Some(Menu::Access));
        let app = self.store.agents.access();
        let chip = launch::access_chip("composer-access", open, &pick.access, app, move |this, cx| this.toggle_composer(slot, Menu::Access, cx), cx);
        let menu = open.then(|| {
            let current = pick.clone();
            let choose: Pick = Rc::new(move |this, access, cx| this.choose_launch(slot, project.clone(), LaunchPick { access, ..current.clone() }, cx));
            menu("composer-access-menu", 300., launch::access_rows("composer-access-row", launch::access_choices(app), &pick.access, choose, cx), slot, cx)
        });
        div().relative().child(chip).children(menu)
    }
}

/// A chip's menu, closed by a press outside it.
fn menu(id: &'static str, width: f32, rows: Vec<AnyElement>, slot: Slot, cx: &mut Context<Desktop>) -> Div {
    let menu = ui::pop(div().id(id))
        .w(px(width))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(5.))
        .flex()
        .flex_col()
        .gap(px(2.))
        // Menus hang over the page; without this a click on one also lands on what it covers.
        .occlude()
        .children(rows)
        .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            slot(this).draft.menu = None;
            cx.notify();
        }));
    ui::dropdown(36., ui::menu_in(SharedString::from(format!("{id}-in")), menu))
}

#[cfg(test)]
mod tests {
    use super::{Attachment, Draft, Menu, attachment_path, pasted_image, save_image, split_images};
    use gpui_kit::{ClipboardItem, Image, ImageFormat};
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
    fn reopening_the_model_menu_closes_its_side_menu_and_points_at_the_picked_agent() {
        let mut draft = Draft::default();
        draft.toggle(Menu::Model);
        (draft.side, draft.viewing) = (Some(super::model_menu::Side::Model), Some("codex"));
        draft.toggle(Menu::Model);
        draft.toggle(Menu::Model);
        assert_eq!((draft.side, draft.viewing), (None, None));
    }

    #[test]
    fn an_image_paste_is_saved_to_a_private_file_and_text_pastes_as_itself() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("pocket-paste-test-{}", std::process::id()));
        let image = pasted_image(&ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]))).unwrap();
        let path = attachment_path(&image, &root.join("attachments"));
        save_image(&image, &path).unwrap();
        assert_eq!((path.extension().and_then(|e| e.to_str()), std::fs::read(&path).unwrap()), (Some("png"), vec![1, 2, 3]));
        assert_eq!(std::fs::metadata(root.join("attachments")).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(pasted_image(&ClipboardItem::new_string("fix ci".into())).is_none());
        std::fs::remove_dir_all(root).unwrap();
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
    fn dropped_images_attach_where_they_lie_and_other_files_are_handed_back() {
        let mut draft = Draft::default();
        let rest = draft.drop_images(&["/r/shot.PNG".into(), "/r/main.rs".into(), "/r/a b.jpg".into(), "/r/notes".into()]);
        assert_eq!(rest, [std::path::PathBuf::from("/r/main.rs"), "/r/notes".into()]);
        assert_eq!(draft.prompt(""), "# Attached images\n\n- /r/shot.PNG\n- /r/a b.jpg");
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
        assert_eq!(split_images(&draft.prompt("Fix CI")), ("Fix CI", vec!["/h/a.png", "/h/b c.png"]));
        assert_eq!(split_images(&draft.prompt("")), ("", vec!["/h/a.png", "/h/b c.png"]));
        assert_eq!(split_images("Fix CI"), ("Fix CI", vec![]));
        let typed = "Explain\n\n# Attached images\n\nthis heading";
        assert_eq!(split_images(typed), (typed, vec![]));
    }
}
