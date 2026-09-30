use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// The compact layout's sidebars, floated over a dimmed page.
    pub(crate) fn panel_view(&mut self, cx: &mut Context<Self>) -> Div {
        let dim = div()
            .id("panel-dim")
            .absolute()
            .inset_0()
            .bg(rgba(0x1111131a))
            .occlude()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.panel = false;
                cx.notify();
            }));
        let column = self.column_view(cx);
        // GPUI has no backdrop blur, so the translucent sidebars sit on the dimmed window colour instead of over the page's text.
        let sidebars = div().h_full().flex().bg(rgba(WINDOW)).shadow(vec![BoxShadow { offset: point(px(16.), px(0.)), ..ui::shadow(0x11111324, 0., 48.) }]).child(div().h_full().flex().bg(rgba(0x1111131a)).child(self.aside(cx)).child(column));
        div().absolute().top_0().bottom_0().left(px(56.)).right_0().flex().child(dim).child(sidebars)
    }
}
