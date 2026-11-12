use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use std::path::Path;
use theme::*;

/// What starts the name of the file a long paste is kept in.
pub(crate) const PASTED: &str = "pasted-";

/// What a card says of the file at `path`: its name and its kind. A long paste, kept among the attachments, has no
/// name of its own.
pub(crate) fn label(path: &Path) -> (String, String) {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    if name.starts_with(PASTED) && path.parent().and_then(Path::file_name).is_some_and(|d| d == "attachments") {
        return ("Pasted text".into(), "Text".into());
    }
    let kind = path.extension().map_or_else(|| "File".into(), |e| e.to_string_lossy().to_uppercase());
    (name, kind)
}

/// A file attached to a prompt, as tall as an attached image beside it: its icon, name and kind, and its path on hover.
pub(crate) fn file_card(id: impl Into<ElementId>, path: &Path) -> Stateful<Div> {
    let (name, kind) = label(path);
    let whole = path.display().to_string();
    let glyph = div().size(px(34.)).flex_none().flex().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(file_icon(&whole, false, false, 18.));
    let words = div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(div().truncate().text_size(px(13.)).line_height(px(17.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name))
        .child(div().truncate().text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_3).child(kind));
    div()
        .id(id)
        .h(px(56.))
        .max_w(px(240.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.))
        .pl(px(10.))
        .pr(px(14.))
        .rounded(px(10.))
        .bg(FILL_1)
        .border_1()
        .border_color(hairline(0.10))
        .child(glyph)
        .child(words)
        .tooltip(move |window, cx| Tooltip::new(whole.clone()).build(window, cx))
}

#[cfg(test)]
mod tests {
    use super::label;
    use std::path::Path;

    #[test]
    fn a_card_names_its_file_and_kind_and_a_long_paste_reads_as_pasted_text() {
        let said = |path: &str| label(Path::new(path));
        assert_eq!(said("/r/docs/Report Q3.pdf"), ("Report Q3.pdf".into(), "PDF".into()));
        assert_eq!(said("/r/Makefile"), ("Makefile".into(), "File".into()));
        assert_eq!(said("/h/attachments/pasted-00ff.txt"), ("Pasted text".into(), "Text".into()));
        assert_eq!(said("/r/pasted-notes.txt"), ("pasted-notes.txt".into(), "TXT".into()), "a file of the user's keeps its name");
    }
}
