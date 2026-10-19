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

/// The input's colours for `text`: its command, its mentions, and each mention's `@` hidden under its icon.
pub(crate) fn decorations(text: &str) -> Vec<TextDecoration> {
    let marks = chat::marks(text);
    let mentions = marks.mentions.into_iter().flat_map(|r| [TextDecoration::new(r.start..r.start + 1, hidden()), TextDecoration::new(r.start + 1..r.end, tint(MENTION))]);
    marks.command.map(|r| TextDecoration::new(r, tint(COMMAND))).into_iter().chain(mentions).collect()
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
