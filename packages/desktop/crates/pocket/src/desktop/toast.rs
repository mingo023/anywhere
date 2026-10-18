use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;
use workspace::Tab;

/// The pane tab strip's height.
const STRIP: f32 = 40.;

impl Desktop {
    /// Floats `toast` `offset` in from the main area's bottom-right corner, or from its top-right under the pane strip while a chat shows, clear of the chat's composer.
    pub(crate) fn float<E: Styled>(&self, toast: E, offset: f32) -> E {
        let shown = self.session_tree().and_then(|t| self.workspaces.get(&t)).map(|w| w.shown()).unwrap_or_default();
        let toast = toast.absolute().right(px(16.));
        if over_chat(shown.iter().map(|(_, t)| *t), |t| self.chat_shown(t)) { toast.top(px(STRIP + offset)) } else { toast.bottom(px(offset)) }
    }

    /// The last error, floated over the main area's corner until dismissed or replaced.
    pub(crate) fn error_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let mark = div().mt(px(1.)).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FAILED).child(icon("x-bold", 9., WHITE));
        let dismiss = div()
            .id("error-toast-dismiss")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.error = None;
                cx.notify();
            }));
        let copy = self.chats.unsent.as_ref().filter(|(message, _)| *message == error).map(|(_, text)| {
            let text = text.clone();
            ui::link("error-toast-copy", "Copy").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                (this.error, this.chats.unsent) = (None, None);
                cx.notify();
            }))
        });
        let toast = self
            .float(ui::pop(div()).id("error-toast"), if self.removals.label().is_some() { 56. } else { 16. })
            .w(px(300.))
            .pl(px(12.))
            .pr(px(10.))
            .py(px(11.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(14.))
            .occlude()
            .child(mark)
            .child(div().flex_1().min_w_0().flex().flex_col().items_start().gap(px(6.)).text_size(px(13.)).text_color(TEXT).child(error).children(copy))
            .child(dismiss);
        Some(toast.with_animation("error-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }

    /// Worktrees being deleted, floated under the error toast until their folders are gone from git.
    pub(crate) fn deleting_toast(&self) -> Option<impl IntoElement> {
        let label = self.removals.label()?;
        let toast = self
            .float(ui::pop(div()), 16.)
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(10.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .whitespace_nowrap()
            .child(spinner("deleting-toast-spinner", 12., TEXT_3))
            .child(label);
        Some(toast.with_animation("deleting-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}

/// Whether a chat shows in one of the `shown` tabs.
fn over_chat<'a>(shown: impl IntoIterator<Item = &'a Tab>, chat: impl Fn(&str) -> bool) -> bool {
    shown.into_iter().any(|t| matches!(t, Tab::Term(id) if chat(id)))
}

#[cfg(test)]
mod tests {
    use super::over_chat;
    use workspace::Tab;

    #[test]
    fn toasts_move_up_off_a_chat_on_screen() {
        let tabs = [Tab::Web(1), Tab::Term("t1".into())];
        assert!(over_chat(&tabs, |t| t == "t1"));
        assert!(!over_chat(&tabs, |_| false), "a terminal tab showing the terminal, not its chat, leaves them in the corner");
        assert!(!over_chat(&[Tab::Web(1)], |_| true));
    }
}
