use gpui_kit::component::input::TextDecoration;
use gpui_kit::*;
use theme::*;

pub(super) fn tint(color: impl Into<Hsla>) -> HighlightStyle {
    HighlightStyle { color: Some(color.into()), ..Default::default() }
}

/// Hides what an icon covers; a transparent colour would blend into the text's own.
pub(super) fn hidden() -> HighlightStyle {
    HighlightStyle { fade_out: Some(1.), ..Default::default() }
}

/// The input's colours for `text`: its commands, its mentions with each `@` hidden under its icon, and its code: a
/// span's backticks hidden inside the shade `marks_overlay` paints, a block's fences dimmed. Its markdown shows as
/// `rich::highlight` has it.
pub(crate) fn decorations(text: &str) -> Vec<TextDecoration> {
    let (marks, code) = chat::marks_outside_code(text);
    let mentions = marks.mentions.into_iter().flat_map(|r| [TextDecoration::new(r.start..r.start + 1, hidden()), TextDecoration::new(r.start + 1..r.end, tint(MENTION))]);
    let code = code.into_iter().flat_map(|c| match c {
        chat::Code::Inline(r) => vec![TextDecoration::new(r.start..r.start + 1, hidden()), TextDecoration::new(r.end - 1..r.end, hidden())],
        chat::Code::Block { open, close, .. } => vec![TextDecoration::new(open, tint(TEXT_4)), TextDecoration::new(close, tint(TEXT_4))],
    });
    let rich = chat::rich::spans(text).into_iter().map(|(r, style)| TextDecoration::new(r, crate::composer::rich::highlight(style)));
    marks.commands.into_iter().map(|r| TextDecoration::new(r, tint(COMMAND))).chain(mentions).chain(code).chain(rich).collect()
}

/// Paints the file icon of each `path` over its `slot`, as tall as `size` at most; a folder's when the path ends in `/`.
pub(super) fn paint_icons<'a>(icons: impl IntoIterator<Item = (Bounds<Pixels>, &'a str)>, size: f32, window: &mut Window, cx: &mut App) {
    for (slot, path) in icons {
        let folder = path.ends_with('/');
        let source = Resource::Embedded(file_icon_path(path.trim_end_matches('/'), folder, false));
        let Some(Ok(image)) = window.use_asset::<ImgResourceLoader>(&source, cx) else { continue };
        let side = px(size).min(slot.size.height);
        let bounds = Bounds::new(point(slot.origin.x, slot.center().y - side / 2.), gpui_kit::size(side, side));
        window.paint_image(bounds, bounds, Corners::default(), image, 0, false).ok();
    }
}
