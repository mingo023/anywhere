use crate::desktop::Desktop;
use gpui_kit::*;
use std::time::Duration;
use theme::*;
use ui::{self, Variant};

const SHOWN_FOR: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq, Debug)]
pub(super) struct Toast {
    seq: u64,
    message: String,
    /// The tickets Undo reopens.
    undo: Vec<String>,
}

impl Desktop {
    pub(super) fn show_board_toast(&mut self, message: String, undo: Vec<String>, cx: &mut Context<Self>) {
        self.board.toasts += 1;
        let seq = self.board.toasts;
        self.board.toast = Some(Toast { seq, message, undo });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SHOWN_FOR).await;
            this.update(cx, |d, cx| {
                if d.board.toast.as_ref().is_some_and(|t| t.seq == seq) {
                    d.board.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn undo_close(&mut self, cx: &mut Context<Self>) {
        for id in self.board.toast.take().map(|t| t.undo).unwrap_or_default() {
            self.outbox.local.board_reopen(&id);
        }
        cx.notify();
    }

    pub(super) fn board_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let toast = self.board.toast.as_ref()?;
        let undo = (!toast.undo.is_empty()).then(|| ui::button("board-undo", Variant::Ghost, None, "Undo").h(px(24.)).px(px(8.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.undo_close(cx))));
        let card = ui::pop(div()).w(px(340.)).min_h(px(44.)).pl(px(12.)).pr(px(8.)).py(px(8.)).flex().items_center().gap(px(8.)).rounded(px(10.)).text_size(px(12.5)).text_color(TEXT).child(icon("check", 13., SUCCESS)).child(div().flex_1().min_w_0().line_height(px(17.)).line_clamp(2).child(toast.message.clone())).children(undo);
        let above_notice = self.board.hidden > 0;
        Some(div().absolute().bottom(px(if above_notice { 54. } else { 14. })).right(px(16.)).child(card))
    }
}
