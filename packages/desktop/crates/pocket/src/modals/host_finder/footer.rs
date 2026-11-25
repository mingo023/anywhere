use super::nav::Hint;
use crate::desktop::Desktop;
use crate::modals::add_project::host_folder::child;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(super) fn finder_footer(&self, online: bool, cx: &mut Context<Self>) -> Div {
        let f = self.host_finder.finder.as_ref();
        let ask = self.finder_ask(cx);
        let path = f.filter(|f| online && !f.dir.is_empty()).map(|f| f.pretty(&f.dir)).unwrap_or_default();
        let hint = match (f, ask.as_ref().and_then(|a| a.hint.as_ref())) {
            (Some(_), Some(Hint::Taken)) => Some(("warning", "Already a project".to_string(), WAITING_TEXT)),
            (Some(_), Some(Hint::Repo)) => Some(("check", "Git repository".to_string(), SUCCESS)),
            (Some(_), Some(Hint::Plain)) => Some(("folder", "Folder, without git".to_string(), TEXT_3)),
            (Some(f), Some(Hint::Clones(name))) => {
                let into = if name.is_empty() { f.dir.clone() } else { child(&f.dir, name) };
                Some(("folder", format!("Clones into {}", f.pretty(&into)), TEXT_3))
            }
            _ => None,
        };
        let ready = ask.as_ref().is_some_and(|a| a.ready);
        let primary = ask.map_or("Use this folder", |a| a.primary);
        let cancel = ui::button("finder-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let choose = ui::button("finder-choose", Variant::Primary, None, primary)
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.finder_choose(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        div()
            .px(px(14.))
            .h(px(56.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().truncate().font_family(MONO).text_size(px(12.5)).child(path)).children(
                hint.map(|(glyph, text, tint)| {
                    div().flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(tint).child(icon(glyph, 12., tint)).child(text)
                }),
            ))
            .child(cancel)
            .child(choose)
    }
}
