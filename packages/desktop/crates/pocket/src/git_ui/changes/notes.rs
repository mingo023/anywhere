use crate::desktop::Desktop;
use crate::git_ui::diff::{line_label, ordered, span};
use git::Repo;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn notes(&self, repo: &Repo, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let note = |id: ElementId, label: String, state: ui::State, text: String, path: String| {
            div()
                .id(id.clone())
                .px(px(10.))
                .py(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .rounded(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_1))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().font_family(MONO).text_size(px(11.5)).text_color(WAITING_TEXT).child(label))
                        .child(ui::status(id, state)),
                )
                .child(div().truncate().text_size(px(13.5)).text_color(TEXT_BODY).child(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(Some(path.clone()), cx)))
        };
        let mut notes: Vec<Stateful<Div>> = self
            .diff
            .comments
            .iter()
            .enumerate()
            .filter(|(_, c)| repo.files.iter().any(|f| f.path == c.path))
            .map(|(i, c)| note(ElementId::NamedInteger("note".into(), i as u64), line_label(c.lines), ui::State::Sent, c.text.clone(), c.path.clone()))
            .collect();
        let draft = self.diff.input.read(cx).value().trim().to_string();
        if let (true, false, Some(path), Some((lo, hi, _))) =
            (self.diff.pick.composing, draft.is_empty(), self.diff.file.clone(), self.diff.pick.range.and_then(|s| span(&self.diff.lines, ordered(s))))
        {
            notes.push(note("draft".into(), line_label((lo, hi)), ui::State::Draft, draft, path));
        }
        if notes.is_empty() {
            return Vec::new();
        }
        std::iter::once(ui::section_header("Comments", Some(notes.len())).into_any_element()).chain(notes.into_iter().map(IntoElement::into_any_element)).collect()
    }
}
