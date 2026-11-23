use super::{Explorer, ExplorerEvent, Landing, files, name_of};
use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use theme::*;

const COPIED_FOR: Duration = Duration::from_millis(2400);
/// Long enough to reach for Undo.
const TRASHED_FOR: Duration = Duration::from_secs(6);
/// Long enough to read a sentence.
const READ_FOR: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) enum Notice {
    Copied(String),
    Trashed(PathBuf),
    Mentioned {
        provider: String,
        title: String,
    },
    /// A word for `agent`'s chat beside `icon`, as why it sent nothing; it shows in that chat's footer.
    InChat {
        agent: String,
        icon: &'static str,
        text: String,
    },
}

/// What the last trash moved, and the focused file it closed.
pub struct Trashed {
    from: PathBuf,
    to: PathBuf,
    open: Option<String>,
}

/// Whether a key press is ⌘Z meant for the tree; a focused text field, terminal or page, by the key contexts it sets, takes it as its own.
pub fn is_undo(key: &Keystroke, contexts: impl FnOnce() -> Vec<KeyContext>) -> bool {
    key.key == "z"
        && Keystroke::parse_with_os("secondary-z", keys::OS).is_ok_and(|z| z.modifiers == key.modifiers)
        && !contexts().iter().any(|c| c.contains("Input") || c.contains(keys::CONTEXT) || c.contains(crate::browser::CONTEXT))
}

/// `path` cut to its last 33 characters behind "…" when longer than 34, so the toast keeps its file name.
pub fn short(path: &str) -> String {
    let n = path.chars().count();
    if n <= 34 {
        return path.to_string();
    }
    format!("…{}", path.chars().skip(n - 33).collect::<String>())
}

/// Why `name` couldn't come back from the Trash.
pub fn unrestored(e: &std::io::Error, name: &str) -> String {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        format!("“{name}” already exists, so it stays in the Trash")
    } else {
        format!("Couldn’t put “{name}” back")
    }
}

impl Explorer {
    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        let showing = match notice {
            Notice::Trashed(_) => TRASHED_FOR,
            Notice::InChat { .. } => READ_FOR,
            _ => COPIED_FOR,
        };
        let hide = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(showing).await;
            this.update(cx, |e, cx| {
                // Undo outlives a later notice only until that one hides.
                (e.notice, e.undo) = (None, None);
                cx.notify();
            })
            .ok();
        });
        self.notice = Some((notice, hide));
        cx.notify();
    }

    pub(crate) fn trash_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let landed = cx.background_executor().spawn({
                let path = path.clone();
                async move { crate::util::trash(&path) }
            });
            let to = landed.await;
            this.update(cx, |_, cx| cx.emit(ExplorerEvent::Trashed { path, to })).ok();
        })
        .detach();
    }

    /// Notes what the Trash took, with the focused file `Desktop` closed for it, or says it couldn't.
    pub(crate) fn trashed(&mut self, path: PathBuf, landed: Option<(PathBuf, Option<String>)>, cx: &mut Context<Self>) {
        match landed {
            Some((to, open)) => {
                self.undo = Some(Trashed { from: path.clone(), to, open });
                self.show_notice(Notice::Trashed(path), cx);
            }
            None => cx.emit(ExplorerEvent::Failed(format!("Couldn’t move “{}” to the Trash", name_of(&path)))),
        }
        self.relist(cx);
        cx.notify();
    }

    /// Puts the last trashed item back; false when there's none to undo.
    pub(crate) fn undo_trash(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Trashed { from, to, open }) = self.undo.take() else { return false };
        self.notice = None;
        cx.spawn(async move |this, cx| {
            let restored = cx.background_executor().spawn({
                let (from, to) = (from.clone(), to.clone());
                async move { files::restore(&to, &from) }
            });
            let restored = restored.await.map_err(|e| unrestored(&e, &name_of(&from)));
            this.update(cx, |_, cx| cx.emit(ExplorerEvent::Landed(Landing::Restored { from, open, restored }))).ok();
        })
        .detach();
        cx.notify();
        true
    }

    /// Shows what an undo put back, or says why it couldn't.
    pub(super) fn restored(&mut self, from: PathBuf, open: Option<String>, restored: Result<(), String>, cx: &mut Context<Self>) {
        match restored {
            Ok(()) if self.in_root(&from) => {
                if let Some(open) = open {
                    cx.emit(ExplorerEvent::Open { path: open, pin: false });
                }
                self.flash_row(from, cx);
            }
            Ok(()) => {}
            Err(message) => cx.emit(ExplorerEvent::Failed(message)),
        }
        self.relist(cx);
        cx.notify();
    }
}

impl Desktop {
    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.explorer.update(cx, |e, cx| e.show_notice(notice, cx));
    }

    pub(crate) fn undo_trash(&mut self, cx: &mut Context<Self>) -> bool {
        self.explorer.update(cx, |e, cx| e.undo_trash(cx))
    }

    /// Closes the tabs of what went to the Trash; a file with unsaved edits stays open, so saving brings it back.
    /// Returns the focused file it closed, for an undo to reopen.
    pub(crate) fn close_trashed(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<String> {
        let preview = self.preview.read(cx);
        let open = preview.file(self.focused_pane()).filter(|f| Path::new(f).starts_with(path) && !preview.dirty(f)).map(String::from);
        let path = path.to_string_lossy();
        self.workspaces.values_mut().for_each(|w| w.close_files(&path, |f| preview.dirty(f)));
        self.load_active(cx);
        self.save_soon(cx);
        open
    }

    pub(crate) fn tabs_follow(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
        self.workspaces.values_mut().for_each(|w| w.move_files(&from, &to));
        self.preview.update(cx, |p, _| p.state.move_files(&from, &to));
        self.save_soon(cx);
    }

    pub(crate) fn tree_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let notice = self.explorer.read(cx).notice.as_ref().map(|(n, _)| n.clone())?;
        let toast = self
            .float(ui::pop(div().id("tree-toast")), 16., cx)
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(10.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .whitespace_nowrap();
        let toast = match &notice {
            Notice::Copied(text) => {
                toast.child(icon("check", 14., SUCCESS)).child("Copied").child(div().font_family(MONO).text_size(px(11.5)).text_color(TEXT).child(short(text)))
            }
            Notice::Mentioned { provider, title } => toast.child(provider_icon(provider, 13., TEXT_2)).child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child("Mentioned in")
                    .child(div().max_w(px(240.)).truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title.clone())),
            ),
            Notice::InChat { .. } => return None,
            Notice::Trashed(from) => toast
                .child(icon("trash", 14., TEXT_3))
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child("Moved")
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(name_of(from)))
                        .child("to Trash"),
                )
                .child(
                    div()
                        .id("tree-toast-undo")
                        .ml(px(4.))
                        .px(px(6.))
                        .py(px(2.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(ACCENT)
                        .font_weight(FontWeight::MEDIUM)
                        .hover(|s| s.bg(FILL_2))
                        .child("Undo")
                        .child(ui::kbd("⌘Z"))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.undo_trash(cx);
                        })),
                ),
        };
        Some(toast.with_animation("tree-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}

#[cfg(test)]
mod tests {
    use super::{is_undo, short, unrestored};
    use gpui_kit::{KeyContext, Keystroke};

    #[test]
    fn only_a_bare_cmd_z_outside_a_text_field_terminal_or_page_undoes_a_trash() {
        let at = |names: &[&str]| names.iter().map(|n| KeyContext::parse(n).unwrap()).collect::<Vec<_>>();
        let cmd_z = Keystroke::parse_with_os("secondary-z", keys::OS).unwrap();
        assert!(is_undo(&cmd_z, || at(&["Desktop"])));
        assert!(!is_undo(&cmd_z, || at(&["Desktop", "Input"])));
        assert!(!is_undo(&cmd_z, || at(&["Desktop", crate::browser::CONTEXT])));
        assert!(!is_undo(&Keystroke::parse_with_os("secondary-shift-z", keys::OS).unwrap(), || at(&["Desktop"])));
    }

    #[test]
    fn an_undo_onto_a_taken_name_says_the_item_stays_in_the_trash() {
        let taken = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
        assert_eq!(unrestored(&taken, "a.rs"), "“a.rs” already exists, so it stays in the Trash");
        assert_eq!(unrestored(&std::io::Error::other("x"), "a.rs"), "Couldn’t put “a.rs” back");
    }

    #[test]
    fn a_long_copied_path_keeps_its_end() {
        assert_eq!(short("src/a.rs"), "src/a.rs");
        let long = "/Users/mingo/.worktrees/coding-pocket/keen-maple/src/explorer.rs";
        assert_eq!(short(long), format!("…{}", &long[long.len() - 33..]));
        assert_eq!(short(long).chars().count(), 34);
    }
}
