use crate::add_to_chat::toast::added;
use crate::desktop::Desktop;
use crate::explorer::toast::{Notice, notice};
use crate::git_ui::pull_request::PrSide;
use crate::region::notify_after_draw;
use gpui_kit::*;
use theme::*;
use workspace::Tab;

/// The pane tab strip's height.
const STRIP: f32 = 40.;

/// What a toast says and what it floats clear of, from what `Desktop` owns.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Frame {
    /// A chat shows in the main area, so the toasts float under its pane strip instead of over its composer.
    pub over_chat: bool,
    pub deleting: Option<String>,
    /// The name of the agent a quote was just added to.
    pub added: Option<String>,
    pub tree: Option<Notice>,
}

struct Failure {
    message: String,
    /// What the toast's Copy puts on the clipboard, when the failure left text worth keeping.
    copy: Option<String>,
}

pub(crate) enum ToastEvent {
    UndoTrash,
}

impl EventEmitter<ToastEvent> for Toasts {}

/// The toasts floated over the main area's corner: the last error until dismissed or replaced, worktrees being deleted, a quote added to a chat, the Explorer's notice and what a PR action did.
pub(crate) struct Toasts {
    error: Option<Failure>,
    pr: Entity<PrSide>,
    frame: Frame,
    _subs: Vec<Subscription>,
}

impl Toasts {
    pub(crate) fn new(pr: Entity<PrSide>, cx: &mut Context<Self>) -> Self {
        let _subs = vec![cx.observe(&pr, |_, _, cx| cx.notify())];
        Self { error: None, pr, frame: Frame::default(), _subs }
    }

    pub(crate) fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(Failure { message, copy: None });
        cx.notify();
    }

    pub(crate) fn fail_with_copy(&mut self, message: String, copy: String, cx: &mut Context<Self>) {
        self.error = Some(Failure { message, copy: Some(copy) });
        cx.notify();
    }

    pub(crate) fn clear_error(&mut self, cx: &mut Context<Self>) {
        if self.error.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn hand(&mut self, frame: Frame, cx: &mut Context<Self>) {
        if self.frame != frame {
            self.frame = frame;
            notify_after_draw(cx);
        }
    }

    /// Floats `toast` `offset` in from the main area's bottom-right corner, or from its top-right under the pane strip while a chat shows, clear of the chat's composer.
    pub(crate) fn float<E: Styled>(&self, toast: E, offset: f32) -> E {
        let toast = toast.absolute().right(px(16.));
        if self.frame.over_chat { toast.top(px(STRIP + offset)) } else { toast.bottom(px(offset)) }
    }

    fn error_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let Failure { message, copy } = self.error.as_ref()?;
        let mark =
            div().mt(px(1.)).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FAILED).child(icon("x-bold", 9., WHITE));
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_error(cx)));
        let copy = copy.clone().map(|text| {
            ui::link("error-toast-copy", "Copy").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                this.clear_error(cx);
            }))
        });
        let toast = self
            .float(ui::pop(div()).id("error-toast"), if self.frame.deleting.is_some() { 56. } else { 16. })
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
            .child(
                div().flex_1().min_w_0().flex().flex_col().items_start().gap(px(6.)).text_size(px(13.)).text_color(TEXT).child(message.clone()).children(copy),
            )
            .child(dismiss);
        Some(toast.with_animation("error-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }

    /// Worktrees being deleted, floated under the error toast until their folders are gone from git.
    fn deleting_toast(&self) -> Option<impl IntoElement + use<>> {
        let label = self.frame.deleting.clone()?;
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

    fn added_toast(&self) -> Option<impl IntoElement + use<>> {
        let toast = self.float(added(self.frame.added.as_deref()?), 16.);
        Some(toast.with_animation("added-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }

    fn tree_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let toast = self.float(notice(self.frame.tree.as_ref()?, cx)?, 16.);
        Some(toast.with_animation("tree-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }

    /// What a PR action just did, floated over the window's corner.
    fn pr_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if self.error.is_some() {
            return None;
        }
        let raised = self.frame.deleting.is_some() || self.frame.added.is_some();
        let toast = self.pr.update(cx, |p, cx| p.toast(cx))?;
        Some(
            self.float(toast, if raised { 56. } else { 16. })
                .with_animation("pr-notice-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)),
        )
    }
}

impl Render for Toasts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .children(self.error_toast(cx))
            .children(self.deleting_toast())
            .children(self.added_toast())
            .children(self.tree_toast(cx))
            .children(self.pr_notice(cx))
    }
}

impl Desktop {
    pub(crate) fn fail(&self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.fail(message.into(), cx));
    }

    /// The toasts over the main area, handed what they say.
    pub(crate) fn toasts_view(&self, cx: &mut Context<Self>) -> Entity<Toasts> {
        let shown = self.session_tree(cx).and_then(|t| self.workspaces.read(cx).get(&t)).map(|w| w.shown()).unwrap_or_default();
        let frame = Frame {
            over_chat: over_chat(shown.iter().map(|(_, t)| *t), |t| self.chat_shown(t)),
            deleting: self.removals.label(),
            added: self.chat.read(cx).added().map(|p| self.agent_name(p)),
            tree: self.explorer.read(cx).notice.as_ref().map(|(n, _)| n.clone()),
        };
        self.toasts.update(cx, |t, cx| t.hand(frame, cx));
        self.toasts.clone()
    }

    pub(crate) fn on_toasts_event(&mut self, _: &Entity<Toasts>, ev: &ToastEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ToastEvent::UndoTrash => _ = self.undo_trash(cx),
        }
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
