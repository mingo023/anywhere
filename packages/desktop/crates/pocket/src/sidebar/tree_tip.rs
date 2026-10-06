use gpui_kit::*;
use theme::*;

/// A worktree row's hover tip: its branch, or its folder when detached.
pub(crate) struct TreeTip(pub(crate) String);

impl Render for TreeTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div()).px(px(8.)).py(px(5.)).text_size(px(12.)).text_color(TEXT_2).child(self.0.clone())
    }
}
