use crate::composer::Draft;
use crate::composer::element_card::element_card;
use crate::composer::elements;
use crate::composer::file_card::file_card;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

/// The draft's attached images and files over the input; none when it has none.
pub(super) fn strip(draft: &Draft, cx: &mut Context<Desktop>) -> Option<Div> {
    if !draft.has_attachments() {
        return None;
    }
    let remove = |kind: &'static str, take: fn(&mut Draft, usize), i: usize| {
        div()
            .id((kind, i))
            .absolute()
            .top(px(-6.))
            .right(px(-6.))
            .size(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(POPOVER)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .cursor_pointer()
            .opacity(0.)
            .group_hover(SharedString::from(format!("{kind}:{i}")), |s| s.opacity(1.))
            .active(|s| s.opacity(0.6))
            .child(icon("circle-x", 14., TEXT_3))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                take(&mut this.chats.composer.draft, i);
                cx.notify();
            }))
    };
    let images = draft.images.iter().enumerate().map(|(i, a)| {
        div()
            .group(SharedString::from(format!("chat-image-remove:{i}")))
            .relative()
            .size(px(56.))
            .flex_none()
            .rounded(px(8.))
            .border_1()
            .border_color(hairline(0.10))
            .child(img(a.image.clone()).size(px(54.)).rounded(px(7.)).object_fit(ObjectFit::Cover))
            .child(remove("chat-image-remove", |d, i| _ = d.images.remove(i), i))
    });
    let files = draft.files.iter().enumerate().map(|(i, path)| {
        div().group(SharedString::from(format!("chat-file-remove:{i}"))).relative().flex_none().child(file_card(("chat-file", i), path)).child(remove("chat-file-remove", |d, i| _ = d.files.remove(i), i))
    });
    let elements = draft.elements.iter().enumerate().map(|(i, e)| {
        let card = element_card(("chat-element", i), &(i + 1).to_string(), web::host(&e.url), &elements::title(e), Some(e.clone()));
        div().group(SharedString::from(format!("chat-element-remove:{i}"))).relative().flex_none().child(card).child(remove("chat-element-remove", |d, i| _ = d.elements.remove(i), i))
    });
    Some(div().w_full().flex().flex_wrap().gap(px(8.)).px(px(16.)).pt(px(12.)).children(images).children(files).children(elements))
}
