use super::files;
use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;
use theme::*;

const COPIED_FOR: Duration = Duration::from_millis(2400);
/// Long enough to reach for Undo.
const TRASHED_FOR: Duration = Duration::from_secs(6);

pub(crate) enum Notice {
    Copied(String),
    Trashed { from: PathBuf, to: PathBuf },
}

/// `path` cut to its last 33 characters behind "…" when longer than 34, so the toast keeps its file name.
pub fn short(path: &str) -> String {
    let n = path.chars().count();
    if n <= 34 {
        return path.to_string();
    }
    format!("…{}", path.chars().skip(n - 33).collect::<String>())
}

impl Desktop {
    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        let showing = if matches!(notice, Notice::Copied(_)) { COPIED_FOR } else { TRASHED_FOR };
        let hide = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(showing).await;
            this.update(cx, |d, cx| {
                d.explorer.notice = None;
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
                    Some(to) => d.show_notice(Notice::Trashed { from: path, to }, cx),
                    None => d.error = Some(format!("Couldn’t move “{}” to the Trash", path.file_name().unwrap_or_default().to_string_lossy())),
                }
                d.relist_explorer(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Puts the last trashed item back; false when there's none to undo.
    pub(crate) fn undo_trash(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((Notice::Trashed { from, to }, _)) = self.explorer.notice.take() else { return false };
        cx.spawn(async move |this, cx| {
            let restored = cx.background_executor().spawn({
                let (from, to) = (from.clone(), to.clone());
                async move { files::restore(&to, &from) }
            });
            let restored = restored.await;
            this.update(cx, |d, cx| {
                if restored.is_err() {
                    d.error = Some(format!("“{}” already exists, so it stays in the Trash", from.file_name().unwrap_or_default().to_string_lossy()));
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
        let toast = ui::pop(div().id("tree-toast"))
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
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
            Notice::Trashed { from, .. } => toast
                .child(icon("trash", 14., TEXT_3))
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child("Moved")
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(from.file_name().unwrap_or_default().to_string_lossy().to_string()))
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
    use super::short;

    #[test]
    fn a_long_copied_path_keeps_its_end() {
        assert_eq!(short("src/a.rs"), "src/a.rs");
        let long = "/Users/mingo/.worktrees/coding-pocket/keen-maple/src/explorer.rs";
        assert_eq!(short(long), format!("…{}", &long[long.len() - 33..]));
        assert_eq!(short(long).chars().count(), 34);
    }
}
