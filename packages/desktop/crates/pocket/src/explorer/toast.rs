use super::{files, name_of};
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

pub(crate) enum Notice {
    Copied(String),
    Trashed(PathBuf),
    Mentioned { provider: String, title: String },
    /// Why the chat sent nothing to `agent`, pointing at its terminal. It shows in that chat's footer.
    InTerminal { agent: String, text: String },
}

/// What the last trash moved, and the focused file it closed.
pub struct Trashed {
    from: PathBuf,
    to: PathBuf,
    open: Option<String>,
}

/// Whether a key press is ⌘Z meant for the tree; a focused text field, terminal or page, by the key contexts it sets, takes it as its own.
pub fn is_undo(key: &Keystroke, contexts: impl FnOnce() -> Vec<KeyContext>) -> bool {
    key.key == "z" && key.modifiers == Modifiers::command() && !contexts().iter().any(|c| c.contains("Input") || c.contains(keys::CONTEXT) || c.contains(crate::browser::CONTEXT))
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

impl Desktop {
    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        let showing = match notice {
            Notice::Trashed(_) => TRASHED_FOR,
            Notice::InTerminal { .. } => READ_FOR,
            _ => COPIED_FOR,
        };
        let hide = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(showing).await;
            this.update(cx, |d, cx| {
                // Undo outlives a later notice only until that one hides.
                (d.explorer.notice, d.explorer.undo) = (None, None);
                cx.notify();
            })
            .ok();
        });
        self.explorer.notice = Some((notice, hide));
        cx.notify();
    }

    pub(crate) fn trash_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let landed = cx.background_executor().spawn({
                let path = path.clone();
                async move { crate::util::trash(&path) }
            });
            let landed = landed.await;
            this.update(cx, |d, cx| {
                match landed {
                    Some(to) => {
                        let open = d.close_trashed(&path, cx);
                        d.explorer.undo = Some(Trashed { from: path.clone(), to, open });
                        d.show_notice(Notice::Trashed(path), cx);
                    }
                    None => d.error = Some(format!("Couldn’t move “{}” to the Trash", name_of(&path))),
                }
                d.relist_explorer(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Closes the tabs of what went to the Trash; a file with unsaved edits stays open, so saving brings it back.
    /// Returns the focused file it closed, for an undo to reopen.
    fn close_trashed(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<String> {
        let preview = &self.preview;
        let open = preview.file(self.focused_pane()).filter(|f| Path::new(f).starts_with(path) && !preview.dirty(f)).map(String::from);
        let path = path.to_string_lossy();
        self.workspaces.values_mut().for_each(|w| w.close_files(&path, |f| preview.dirty(f)));
        self.load_active(cx);
        self.save_soon(cx);
        open
    }

    /// Puts the last trashed item back; false when there's none to undo.
    pub(crate) fn undo_trash(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Trashed { from, to, open }) = self.explorer.undo.take() else { return false };
        self.explorer.notice = None;
        cx.spawn(async move |this, cx| {
            let restored = cx.background_executor().spawn({
                let (from, to) = (from.clone(), to.clone());
                async move { files::restore(&to, &from) }
            });
            let restored = restored.await;
            this.update(cx, |d, cx| {
                match restored {
                    Ok(()) if d.in_explore_root(&from) => {
                        if let Some(open) = open {
                            d.open_file(open, false, cx);
                        }
                        d.flash_row(from, cx);
                    }
                    Ok(()) => {}
                    Err(e) => d.error = Some(unrestored(&e, &name_of(&from))),
                }
                d.relist_explorer(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    pub(crate) fn tree_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (notice, _) = self.explorer.notice.as_ref()?;
        let toast = self
            .float(ui::pop(div().id("tree-toast")), 16.)
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(10.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .whitespace_nowrap();
        let toast = match notice {
            Notice::Copied(text) => toast.child(icon("check", 14., SUCCESS)).child("Copied").child(div().font_family(MONO).text_size(px(11.5)).text_color(TEXT).child(short(text))),
            Notice::Mentioned { provider, title } => toast
                .child(provider_icon(provider, 13., TEXT_2))
                .child(div().flex().gap(px(4.)).child("Mentioned in").child(div().max_w(px(240.)).truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title.clone()))),
            Notice::InTerminal { .. } => return None,
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
        let cmd_z = Keystroke::parse("cmd-z").unwrap();
        assert!(is_undo(&cmd_z, || at(&["Desktop"])));
        assert!(!is_undo(&cmd_z, || at(&["Desktop", "Input"])));
        assert!(!is_undo(&cmd_z, || at(&["Desktop", crate::browser::CONTEXT])));
        assert!(!is_undo(&Keystroke::parse("cmd-shift-z").unwrap(), || at(&["Desktop"])));
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
