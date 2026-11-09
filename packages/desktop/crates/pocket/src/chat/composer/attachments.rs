use crate::composer::Attachment;
use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::PathBuf;
use theme::*;

/// For the input's `on_drop_paths`: dropped images attach, other files go in as their paths.
pub(super) fn drop_images(desktop: WeakEntity<Desktop>) -> impl Fn(&[PathBuf], &mut Window, &mut App) -> Vec<PathBuf> {
    move |paths, _, cx| match desktop.upgrade() {
        Some(d) => d.update(cx, |d, cx| {
            cx.notify();
            d.chats.composer.draft.drop_images(paths)
        }),
        None => paths.to_vec(),
    }
}

/// The attached images over the input; none when there are none.
pub(super) fn strip(images: &[Attachment], cx: &mut Context<Desktop>) -> Option<Div> {
    if images.is_empty() {
        return None;
    }
    let tile = |(i, a): (usize, &Attachment)| {
        let group = SharedString::from(format!("chat-image:{i}"));
        let remove = div()
            .id(("chat-image-remove", i))
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
            .group_hover(group.clone(), |s| s.opacity(1.))
            .active(|s| s.opacity(0.6))
            .child(icon("circle-x", 14., TEXT_3))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.chats.composer.draft.images.remove(i);
                cx.notify();
            }));
        div()
            .group(group)
            .relative()
            .size(px(56.))
            .flex_none()
            .rounded(px(8.))
            .border_1()
            .border_color(hairline(0.10))
            .child(img(a.image.clone()).size(px(54.)).rounded(px(7.)).object_fit(ObjectFit::Cover))
            .child(remove)
    };
    Some(div().w_full().flex().flex_wrap().gap(px(8.)).px(px(16.)).pt(px(12.)).children(images.iter().enumerate().map(tile)))
}

